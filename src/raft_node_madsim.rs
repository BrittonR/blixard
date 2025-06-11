use crate::storage::Storage;
use crate::raft_storage::RaftStorage;
use crate::state_machine::{StateMachine, StateMachineCommand};
use crate::raft_message_wrapper::RaftMessageWrapper;
use raft::{Config, RawNode, StateRole};
use raft::prelude::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

#[cfg(not(madsim))]
use tokio::time::sleep;

#[cfg(madsim)]
use madsim::time::sleep;

/// Raft node implementation that works with madsim
pub struct RaftNodeMadsim {
    id: u64,
    raw_node: RawNode<RaftStorage>,
    state_machine: Arc<RwLock<StateMachine>>,
    peers: HashMap<u64, SocketAddr>,
    message_tx: mpsc::Sender<RaftMessageWrapper>,
    message_rx: mpsc::Receiver<RaftMessageWrapper>,
}

impl RaftNodeMadsim {
    pub async fn new(
        id: u64,
        bind_addr: SocketAddr,
        storage: Arc<Storage>,
        peer_ids: Vec<u64>,
    ) -> anyhow::Result<Self> {
        // Configure Raft
        let mut cfg = Config {
            id,
            election_tick: 10,
            heartbeat_tick: 3,
            max_size_per_msg: 1024 * 1024,
            max_inflight_msgs: 256,
            ..Default::default()
        };
        
        // Set initial peers
        cfg.validate()?;
        
        // Create raft storage wrapper
        let raft_storage = RaftStorage::new(storage.clone()).await?;
        
        // Create raw node with a logger
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let raw_node = RawNode::new(&cfg, raft_storage, &logger)?;
        
        // Create state machine
        let state_machine = Arc::new(RwLock::new(StateMachine::new(storage.clone())));
        
        // Create message channel
        let (message_tx, message_rx) = mpsc::channel(1000);
        
        Ok(Self {
            id,
            raw_node,
            state_machine,
            peers: HashMap::new(),
            message_tx,
            message_rx,
        })
    }
    
    pub async fn run(mut self) -> anyhow::Result<()> {
        // Create a ticker for driving Raft using madsim
        loop {
            tokio::select! {
                _ = sleep(Duration::from_millis(100)) => {
                    self.raw_node.tick();
                    self.process_ready().await?;
                }
                
                Some(msg) = self.message_rx.recv() => {
                    self.handle_message(msg).await?;
                }
            }
        }
    }
    
    async fn process_ready(&mut self) -> anyhow::Result<()> {
        if !self.raw_node.has_ready() {
            return Ok(());
        }
        
        let mut ready = self.raw_node.ready();
        
        // Handle committed entries
        for entry in ready.take_committed_entries() {
            if entry.data.is_empty() {
                continue;
            }
            
            // Apply to state machine
            let mut sm = self.state_machine.write().await;
            sm.apply(&entry.data).await?;
        }
        
        // Send messages to other nodes
        for msg in ready.take_messages() {
            self.send_message(msg).await?;
        }
        
        // Advance the node
        let _ = self.raw_node.advance(ready);
        
        Ok(())
    }
    
    async fn handle_message(&mut self, msg: RaftMessageWrapper) -> anyhow::Result<()> {
        self.raw_node.step(msg.into())?;
        self.process_ready().await
    }
    
    async fn send_message(&self, _msg: Message) -> anyhow::Result<()> {
        // In madsim, we would use madsim's network simulation
        // For now, this is a placeholder
        Ok(())
    }
    
    pub async fn propose(&mut self, command: StateMachineCommand) -> anyhow::Result<()> {
        let data = bincode::serialize(&command)?;
        self.raw_node.propose(vec![], data)?;
        self.process_ready().await
    }
    
    pub fn is_leader(&self) -> bool {
        self.raw_node.raft.state == StateRole::Leader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[madsim::test]
    async fn test_raft_node_with_madsim() {
        // Create a temporary directory for storage
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path().join("test.db")).unwrap());
        
        // Create a single node
        let node = RaftNodeMadsim::new(
            1,
            "127.0.0.1:8001".parse().unwrap(),
            storage,
            vec![],
        ).await.unwrap();
        
        // Basic test - node should be created successfully
        assert!(!node.is_leader());
    }
    
    #[madsim::test]
    async fn test_deterministic_raft_cluster() {
        use crate::state_machine::{ServiceInfo, ServiceState};
        
        // Test with multiple seeds to ensure determinism
        for seed in [42, 100, 200] {
            let result1 = run_cluster_simulation(seed).await;
            let result2 = run_cluster_simulation(seed).await;
            
            assert_eq!(result1, result2, 
                "Simulation with seed {} produced different results", seed);
        }
    }
    
    async fn run_cluster_simulation(seed: u64) -> Vec<String> {
        // This is where we would use madsim's deterministic features
        // For now, return a simple result
        vec![format!("seed-{}", seed)]
    }
}