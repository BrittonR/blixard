//! Additional types needed for simulation tests
//! 
//! These types mirror those in blixard-core but are independent to avoid
//! tonic/madsim-tonic conflicts.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Shared node state for simulation tests
#[derive(Debug)]
pub struct SharedNodeState {
    pub node_id: u64,
    pub peers: Arc<Mutex<Vec<PeerInfo>>>,
    pub raft_status: Arc<Mutex<Option<RaftStatus>>>,
    pub is_initialized: Arc<Mutex<bool>>,
}

impl SharedNodeState {
    pub fn new(config: &crate::NodeConfig) -> Self {
        Self {
            node_id: config.id,
            peers: Arc::new(Mutex::new(Vec::new())),
            raft_status: Arc::new(Mutex::new(None)),
            is_initialized: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn add_peer(&self, peer: PeerInfo) {
        let mut peers = self.peers.lock().unwrap();
        if !peers.iter().any(|p| p.id == peer.id) {
            peers.push(peer);
        }
    }

    pub async fn remove_peer(&self, peer_id: u64) {
        let mut peers = self.peers.lock().unwrap();
        peers.retain(|p| p.id != peer_id);
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.lock().unwrap().clone()
    }

    pub async fn update_raft_status(&self, status: RaftStatus) {
        *self.raft_status.lock().unwrap() = Some(status);
    }

    pub async fn is_leader(&self) -> bool {
        self.raft_status.lock().unwrap()
            .as_ref()
            .map(|s| s.is_leader)
            .unwrap_or(false)
    }
}

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: u64,
    pub addr: String,
}

/// Raft status information
#[derive(Debug, Clone)]
pub struct RaftStatus {
    pub is_leader: bool,
    pub term: u64,
    pub commit_index: u64,
    pub last_applied: u64,
    pub leader_id: Option<u64>,
}

/// Task specification for the task scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub name: String,
    pub resources: ResourceRequirements,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Resource requirements for tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu: f64,
    pub memory: u64,
    pub disk: u64,
}

/// Error types for simulation tests
#[derive(Debug, thiserror::Error)]
pub enum BlixardError {
    #[error("Not implemented: {feature}")]
    NotImplemented { feature: String },
    
    #[error("Node error: {message}")]
    Node { message: String },
    
    #[error("Raft error: {message}")]
    Raft { message: String },
    
    #[error("Storage error: {message}")]
    Storage { message: String },
    
    #[error("Network error: {message}")]
    Network { message: String },
    
    #[error("Invalid state: {message}")]
    InvalidState { message: String },
    
    #[error("Timeout: {message}")]
    Timeout { message: String },
    
    #[error("Other error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type BlixardResult<T> = Result<T, BlixardError>;