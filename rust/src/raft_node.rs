use anyhow::{Result, Context};
use raft::prelude::*;
use raft::{Config, RawNode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn, error};

use crate::storage::Storage;
use crate::types::*;

/// Raft node implementation
pub struct RaftNode {
    node_id: u64,
    raw_node: RawNode<MemStorage>,
    storage: Arc<Storage>,
    peers: HashMap<u64, String>, // node_id -> address
    
    // Channels for communication
    proposal_tx: mpsc::Sender<Vec<u8>>,
    proposal_rx: mpsc::Receiver<Vec<u8>>,
}

impl RaftNode {
    pub fn new(node_id: u64, storage: Arc<Storage>, peers: Vec<u64>) -> Result<Self> {
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
        
        // Initialize storage
        let mut raft_storage = MemStorage::default();
        
        // Bootstrap if this is the first node
        if node_id == 1 && peers.is_empty() {
            raft_storage.initialize_with_conf_state(
                vec![node_id],
                vec![]
            );
        }
        
        // Create the Raft node
        let raw_node = RawNode::new(&config, raft_storage)?;
        
        // Create proposal channel
        let (proposal_tx, proposal_rx) = mpsc::channel(256);
        
        Ok(Self {
            node_id,
            raw_node,
            storage,
            peers: HashMap::new(),
            proposal_tx,
            proposal_rx,
        })
    }
    
    /// Get a handle to propose commands
    pub fn get_proposal_handle(&self) -> mpsc::Sender<Vec<u8>> {
        self.proposal_tx.clone()
    }
    
    /// Main Raft processing loop
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Raft node {}", self.node_id);
        
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(100));
        
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Tick the Raft state machine
                    self.raw_node.tick();
                }
                
                Some(proposal) = self.proposal_rx.recv() => {
                    // Handle new proposals
                    self.handle_proposal(proposal)?;
                }
            }
            
            // Process ready state
            if self.raw_node.has_ready() {
                self.process_ready().await?;
            }
        }
    }
    
    /// Handle a proposal from the application
    fn handle_proposal(&mut self, data: Vec<u8>) -> Result<()> {
        debug!("Proposing command of {} bytes", data.len());
        self.raw_node.propose(vec![], data)?;
        Ok(())
    }
    
    /// Process Raft ready state
    async fn process_ready(&mut self) -> Result<()> {
        let mut ready = self.raw_node.ready();
        
        // Save entries to storage
        if !ready.entries().is_empty() {
            let storage = self.raw_node.raft.raft_log.store.clone();
            storage.append(ready.entries())?;
        }
        
        // Send messages to other nodes
        for msg in ready.take_messages() {
            self.send_message(msg).await?;
        }
        
        // Apply snapshot
        if let Some(snapshot) = ready.snapshot() {
            self.apply_snapshot(snapshot)?;
        }
        
        // Apply committed entries
        for entry in ready.take_committed_entries() {
            self.apply_entry(entry).await?;
        }
        
        // Advance the Raft state machine
        let mut light_ready = self.raw_node.advance(ready);
        
        // Send any additional messages
        for msg in light_ready.take_messages() {
            self.send_message(msg).await?;
        }
        
        // Commit entries to storage
        if let Some(commit) = light_ready.commit_index() {
            let storage = self.raw_node.raft.raft_log.store.clone();
            storage.commit_to(commit)?;
        }
        
        self.raw_node.advance_apply();
        
        Ok(())
    }
    
    /// Apply a committed entry to the state machine
    async fn apply_entry(&mut self, entry: Entry) -> Result<()> {
        if entry.data.is_empty() {
            // Empty entry, skip
            return Ok(());
        }
        
        debug!("Applying entry: index={}, term={}", entry.index, entry.term);
        
        // Deserialize the command
        let command: VmCommand = bincode::deserialize(&entry.data)
            .context("Failed to deserialize command")?;
        
        // Apply the command
        match command {
            VmCommand::Create { config, node_id } => {
                let vm_state = VmState {
                    name: config.name.clone(),
                    config,
                    status: VmStatus::Creating,
                    node_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                self.storage.save_vm(&vm_state)?;
                info!("Created VM: {}", vm_state.name);
            }
            
            VmCommand::UpdateStatus { name, status } => {
                if let Some(mut vm) = self.storage.get_vm(&name)? {
                    vm.status = status;
                    vm.updated_at = chrono::Utc::now();
                    self.storage.save_vm(&vm)?;
                    info!("Updated VM {} status to {:?}", name, status);
                } else {
                    warn!("VM {} not found for status update", name);
                }
            }
            
            VmCommand::Delete { name } => {
                self.storage.delete_vm(&name)?;
                info!("Deleted VM: {}", name);
            }
            
            _ => {
                // TODO: Implement other commands
                warn!("Unhandled command: {:?}", command);
            }
        }
        
        Ok(())
    }
    
    /// Send a message to another node
    async fn send_message(&self, msg: Message) -> Result<()> {
        debug!("Sending message to node {}: {:?}", msg.to, msg.msg_type);
        
        // TODO: Implement actual network sending
        // For now, just log it
        if let Some(addr) = self.peers.get(&msg.to) {
            debug!("Would send to {}", addr);
        } else {
            warn!("Unknown peer: {}", msg.to);
        }
        
        Ok(())
    }
    
    /// Apply a snapshot
    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        info!("Applying snapshot at index {}", snapshot.get_metadata().index);
        
        // TODO: Implement snapshot application
        // This would restore the entire state from the snapshot
        
        Ok(())
    }
    
    /// Handle incoming Raft message
    pub fn receive_message(&mut self, msg: Message) -> Result<()> {
        debug!("Received message from node {}: {:?}", msg.from, msg.msg_type);
        self.raw_node.step(msg)?;
        Ok(())
    }
}

/// Raft state machine that can be applied to storage
#[derive(Debug)]
pub struct StateMachine {
    storage: Arc<Storage>,
}

impl StateMachine {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
    
    /// Apply a command to the state machine
    pub async fn apply(&mut self, command: VmCommand) -> Result<()> {
        match command {
            VmCommand::Create { config, node_id } => {
                let vm_state = VmState {
                    name: config.name.clone(),
                    config,
                    status: VmStatus::Creating,
                    node_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                self.storage.save_vm(&vm_state)?;
            }
            
            VmCommand::Start { name } => {
                if let Some(mut vm) = self.storage.get_vm(&name)? {
                    vm.status = VmStatus::Starting;
                    vm.updated_at = chrono::Utc::now();
                    self.storage.save_vm(&vm)?;
                }
            }
            
            VmCommand::Stop { name } => {
                if let Some(mut vm) = self.storage.get_vm(&name)? {
                    vm.status = VmStatus::Stopping;
                    vm.updated_at = chrono::Utc::now();
                    self.storage.save_vm(&vm)?;
                }
            }
            
            VmCommand::Delete { name } => {
                self.storage.delete_vm(&name)?;
            }
            
            VmCommand::UpdateStatus { name, status } => {
                if let Some(mut vm) = self.storage.get_vm(&name)? {
                    vm.status = status;
                    vm.updated_at = chrono::Utc::now();
                    self.storage.save_vm(&vm)?;
                }
            }
        }
        
        Ok(())
    }
}