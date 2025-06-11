use anyhow::Result;
use raft::prelude::*;
use raft::{Config, RawNode, StateRole};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

#[cfg(feature = "failpoints")]
use fail::fail_point;

use crate::network::RaftNetwork;
use crate::raft_storage::RaftStorage;
use crate::state_machine::{StateMachine, StateMachineCommand};
use crate::storage::Storage;
// Temporarily comment out until we fix the import issue
// use crate::runtime_abstraction;

/// Raft node implementation with persistent storage and state machine
pub struct RaftNode {
    node_id: u64,
    raw_node: RawNode<RaftStorage>,
    storage: Arc<Storage>,
    state_machine: Arc<RwLock<StateMachine>>,
    peers: HashMap<u64, SocketAddr>, // node_id -> address
    network: Arc<RaftNetwork>,

    // Channels for communication
    proposal_tx: mpsc::Sender<StateMachineCommand>,
    proposal_rx: mpsc::Receiver<StateMachineCommand>,

    // Network message channel
    message_tx: mpsc::Sender<Message>,
    message_rx: mpsc::Receiver<Message>,
}

impl RaftNode {
    pub async fn new(
        node_id: u64,
        bind_addr: SocketAddr,
        storage: Arc<Storage>,
        _peers: Vec<u64>,
    ) -> Result<Self> {
        // Create Raft configuration
        let config = Config {
            id: node_id,
            election_tick: 10,
            heartbeat_tick: 3,
            applied: 0,
            max_size_per_msg: 1024 * 1024,
            max_inflight_msgs: 256,
            ..Default::default()
        };

        // Initialize persistent Raft storage
        let raft_storage = RaftStorage::new(storage.clone()).await?;

        // Bootstrap the cluster configuration
        // For testing: all nodes start with the same configuration
        // In production, use dynamic membership changes
        if !_peers.is_empty() {
            let mut conf_state = ConfState::default();
            // For testing, assume a 3-node cluster
            conf_state.set_voters(vec![1, 2, 3]);
            raft_storage.save_conf_state(&conf_state).await?;
            info!(
                "Node {} bootstrapped with voters: {:?}",
                node_id, conf_state.voters
            );
        } else {
            // Single node mode
            let mut conf_state = ConfState::default();
            conf_state.set_voters(vec![node_id]);
            raft_storage.save_conf_state(&conf_state).await?;
            info!("Node {} bootstrapped as single node", node_id);
        }

        // Create the Raft node
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let raw_node = RawNode::new(&config, raft_storage, &logger)?;

        // Create state machine
        let state_machine = Arc::new(RwLock::new(StateMachine::new(storage.clone())));

        // Create channels
        let (proposal_tx, proposal_rx) = mpsc::channel(256);
        let (message_tx, message_rx) = mpsc::channel(1024);

        // Create network layer
        let network = Arc::new(RaftNetwork::new(node_id, bind_addr, message_tx.clone()));

        // Start network listener in background
        let network_clone = network.clone();
        tokio::spawn(async move {
            if let Err(e) = network_clone.listen().await {
                tracing::error!("Network listener error: {}", e);
            }
        });

        Ok(Self {
            node_id,
            raw_node,
            storage,
            state_machine,
            peers: HashMap::new(),
            network,
            proposal_tx,
            proposal_rx,
            message_tx,
            message_rx,
        })
    }

    /// Get a handle to propose commands
    pub fn get_proposal_handle(&self) -> mpsc::Sender<StateMachineCommand> {
        self.proposal_tx.clone()
    }

    /// Get a handle to send Raft messages
    pub fn get_message_handle(&self) -> mpsc::Sender<Message> {
        self.message_tx.clone()
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> bool {
        self.raw_node.raft.state == StateRole::Leader
    }

    /// Main Raft processing loop
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Raft node {}", self.node_id);

        // Node 1 should campaign immediately to become leader
        if self.node_id == 1 {
            info!("Node 1 starting election campaign");
            self.raw_node.campaign()?;
        }

        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Tick the Raft state machine
                    self.raw_node.tick();
                }

                Some(command) = self.proposal_rx.recv() => {
                    // Handle new proposals
                    self.handle_proposal(command).await?;
                }

                Some(message) = self.message_rx.recv() => {
                    // Handle incoming Raft messages
                    self.handle_message(message).await?;
                }
            }

            // Process ready state
            if self.raw_node.has_ready() {
                self.process_ready().await?;
            }
        }
    }

    /// Handle a proposal from the application
    async fn handle_proposal(&mut self, command: StateMachineCommand) -> Result<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("raft::before_proposal", |_| Err(anyhow::anyhow!(
            "Injected proposal failure"
        )));

        if !self.is_leader() {
            warn!("Rejecting proposal - not the leader");
            return Ok(());
        }

        // Handle AddNode command specially as a configuration change
        if let StateMachineCommand::AddNode { node_id } = &command {
            info!("Proposing to add node {} to cluster", node_id);

            let mut cc = ConfChange::default();
            cc.set_node_id(*node_id);
            cc.set_change_type(ConfChangeType::AddNode);

            #[cfg(feature = "failpoints")]
            fail_point!("raft::propose_conf_change");

            self.raw_node.propose_conf_change(vec![], cc)?;
            return Ok(());
        }

        let data = bincode::serialize(&command)?;
        debug!("Proposing command: {:?}", command);

        #[cfg(feature = "failpoints")]
        fail_point!("raft::propose");

        self.raw_node.propose(vec![], data)?;
        Ok(())
    }

    /// Handle an incoming Raft message
    async fn handle_message(&mut self, message: Message) -> Result<()> {
        debug!("Received message from node {}", message.from);

        #[cfg(feature = "failpoints")]
        fail_point!("raft::handle_message", |_| Err(anyhow::anyhow!(
            "Injected message handling failure"
        )));

        self.raw_node.step(message)?;
        Ok(())
    }

    /// Process the ready state from Raft
    async fn process_ready(&mut self) -> Result<()> {
        let mut ready = self.raw_node.ready();

        // Check for state changes
        if let Some(ss) = ready.ss() {
            info!("Node {} state changed to {:?}", self.node_id, ss.raft_state);
        }

        // Save Raft state if changed
        if let Some(hs) = ready.hs() {
            // Access storage through raw_node
            let storage = self.raw_node.store();
            storage.save_hard_state(hs).await?;
        }

        // Save snapshots
        if !ready.snapshot().is_empty() {
            self.apply_snapshot(ready.snapshot()).await?;
        }

        // Append entries to log
        if !ready.entries().is_empty() {
            let storage = self.raw_node.store();
            storage.append(ready.entries()).await?;
        }

        // Send messages to other nodes
        let messages = ready.take_messages();
        if messages.is_empty() {
            debug!("Node {} ready has 0 messages", self.node_id);
        } else {
            info!(
                "Node {} ready has {} messages to send",
                self.node_id,
                messages.len()
            );
            for msg in messages {
                info!(
                    "Node {} sending message to node {}: {:?}",
                    self.node_id, msg.to, msg.msg_type
                );
                self.send_message(msg).await?;
            }
        }

        // Apply committed entries to state machine
        let committed_entries = ready.take_committed_entries();
        if !committed_entries.is_empty() {
            self.apply_entries(committed_entries).await?;
        }

        // Send persisted messages
        let persisted_messages = ready.take_persisted_messages();
        if !persisted_messages.is_empty() {
            info!(
                "Node {} has {} persisted messages to send",
                self.node_id,
                persisted_messages.len()
            );
            for msg in persisted_messages {
                info!(
                    "Node {} sending persisted message to node {}: {:?}",
                    self.node_id, msg.to, msg.msg_type
                );
                self.send_message(msg).await?;
            }
        }

        // Advance the Raft state machine
        let mut light_ready = self.raw_node.advance(ready);

        // Send more messages if needed
        let light_messages = light_ready.take_messages();
        if !light_messages.is_empty() {
            info!(
                "Node {} has {} light messages to send",
                self.node_id,
                light_messages.len()
            );
            for msg in light_messages {
                info!(
                    "Node {} sending light message to node {}: {:?}",
                    self.node_id, msg.to, msg.msg_type
                );
                self.send_message(msg).await?;
            }
        }

        // Apply more committed entries if any
        if !light_ready.committed_entries().is_empty() {
            self.apply_entries(light_ready.take_committed_entries())
                .await?;
        }

        // Advance again
        self.raw_node.advance_apply();

        Ok(())
    }

    /// Apply committed entries to the state machine
    async fn apply_entries(&mut self, entries: Vec<Entry>) -> Result<()> {
        for entry in entries {
            if entry.data.is_empty() {
                // Empty entry, skip
                continue;
            }

            debug!(
                "Applying entry at index {} type {:?}",
                entry.index, entry.entry_type
            );

            // Handle configuration changes
            if entry.get_entry_type() == EntryType::EntryConfChange {
                info!("Applying configuration change at index {}", entry.index);

                // For now, we'll just apply without decoding - the apply_conf_change
                // will handle the actual configuration update
                // In production, you'd decode the ConfChange here

                continue;
            }

            // Apply to state machine
            let mut state_machine = self.state_machine.write().await;
            match state_machine.apply(&entry.data).await {
                Ok(_) => {
                    info!("Applied entry at index {}", entry.index);
                }
                Err(e) => {
                    warn!("Failed to apply entry at index {}: {}", entry.index, e);
                }
            }
        }
        Ok(())
    }

    /// Apply a snapshot
    async fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        info!(
            "Applying snapshot at index {}",
            snapshot.get_metadata().index
        );

        // Apply snapshot to Raft storage
        let storage = self.raw_node.store();
        storage.apply_snapshot(snapshot.clone()).await?;

        // Restore state machine from snapshot
        let mut state_machine = self.state_machine.write().await;
        state_machine.restore_snapshot(snapshot.get_data()).await?;

        Ok(())
    }

    /// Send a message to another node
    async fn send_message(&self, msg: Message) -> Result<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("raft::before_send_message", |_| Err(anyhow::anyhow!(
            "Injected send failure"
        )));

        self.network.send_message(msg).await
    }

    /// Create a snapshot of the current state
    pub async fn create_snapshot(&mut self) -> Result<()> {
        let applied = self.raw_node.raft.raft_log.applied;
        let state_machine = self.state_machine.read().await;
        let data = state_machine.snapshot().await?;

        let conf_state = self.raw_node.raft.prs().conf().to_conf_state();
        let storage = self.raw_node.store();
        storage.create_snapshot(applied, conf_state, data).await?;

        info!("Created snapshot at index {}", applied);
        Ok(())
    }

    /// Register a peer's network address (doesn't require leader)
    pub async fn register_peer_address(&mut self, node_id: u64, addr: SocketAddr) {
        self.peers.insert(node_id, addr);
        self.network.add_peer(node_id, addr).await;
    }

    /// Add a peer to the cluster (requires being leader)
    pub async fn add_peer(&mut self, node_id: u64, addr: SocketAddr) -> Result<()> {
        if !self.is_leader() {
            return Err(anyhow::anyhow!("Not the leader"));
        }

        self.peers.insert(node_id, addr);
        self.network.add_peer(node_id, addr).await;

        // Propose configuration change
        let mut cc = ConfChangeV2::default();
        cc.set_transition(ConfChangeTransition::Auto);

        let mut change = ConfChangeSingle::default();
        change.set_change_type(ConfChangeType::AddNode);
        change.set_node_id(node_id);
        cc.set_changes(vec![change]);

        self.raw_node.propose_conf_change(vec![], cc)?;
        info!("Proposed adding node {} to cluster", node_id);

        Ok(())
    }

    /// Remove a peer from the cluster
    pub async fn remove_peer(&mut self, node_id: u64) -> Result<()> {
        if !self.is_leader() {
            return Err(anyhow::anyhow!("Not the leader"));
        }

        self.peers.remove(&node_id);
        self.network.remove_peer(node_id).await;

        // Propose configuration change
        let mut cc = ConfChangeV2::default();
        cc.set_transition(ConfChangeTransition::Auto);

        let mut change = ConfChangeSingle::default();
        change.set_change_type(ConfChangeType::RemoveNode);
        change.set_node_id(node_id);
        cc.set_changes(vec![change]);

        self.raw_node.propose_conf_change(vec![], cc)?;
        info!("Proposed removing node {} from cluster", node_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_raft_node_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path().join("test.db")).unwrap());
        let bind_addr = "127.0.0.1:0".parse().unwrap();
        let node = RaftNode::new(1, bind_addr, storage, vec![]).await.unwrap();

        assert_eq!(node.node_id, 1);
        assert!(node.peers.is_empty());
    }
}
