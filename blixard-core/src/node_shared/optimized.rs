//! Optimized shared node state with reduced lock contention
//!
//! This module provides optimizations for SharedNodeState:
//! - Fine-grained locking to reduce contention
//! - Lock-free reads for frequently accessed data
//! - Cached values for hot paths
//! - Atomic operations where possible

use crate::{
    p2p_manager::PeerInfo,
    types::NodeConfig,
};
use parking_lot::{RwLock, Mutex};
use redb::Database;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

/// Optimized Raft status with atomic fields
#[derive(Debug)]
pub struct AtomicRaftStatus {
    pub is_leader: AtomicBool,
    pub node_id: u64,  // Never changes, no need for atomic
    pub leader_id: AtomicU64,  // 0 means None
    pub term: AtomicU64,
}

impl AtomicRaftStatus {
    fn new(node_id: u64) -> Self {
        Self {
            is_leader: AtomicBool::new(false),
            node_id,
            leader_id: AtomicU64::new(0),
            term: AtomicU64::new(0),
        }
    }
    
    /// Get a snapshot of the status
    pub fn snapshot(&self) -> crate::node_shared::RaftStatus {
        let leader_id_val = self.leader_id.load(Ordering::Acquire);
        crate::node_shared::RaftStatus {
            is_leader: self.is_leader.load(Ordering::Acquire),
            node_id: self.node_id,
            leader_id: if leader_id_val == 0 { None } else { Some(leader_id_val) },
            term: self.term.load(Ordering::Acquire),
            state: if self.is_leader.load(Ordering::Acquire) { 
                "Leader".to_string() 
            } else { 
                "Follower".to_string() 
            },
        }
    }
}

/// Optimized shared state with fine-grained locking
pub struct OptimizedSharedNodeState {
    /// Immutable node configuration (no locking needed)
    config: Arc<NodeConfig>,
    
    /// Database handle (infrequently accessed, regular mutex is fine)
    database: Mutex<Option<Arc<Database>>>,
    
    /// Atomic flags for lock-free reads
    is_initialized: AtomicBool,
    
    /// Raft status with atomic fields
    raft_status: Arc<AtomicRaftStatus>,
    
    /// Cluster members with read-write lock
    /// Using parking_lot for better performance
    cluster_members: RwLock<HashMap<u64, PeerInfo>>,
    
    /// Cached cluster member count for fast access
    cluster_size: AtomicU64,
}

impl OptimizedSharedNodeState {
    /// Create new optimized shared state
    pub fn new(config: NodeConfig) -> Self {
        let node_id = config.id;
        Self {
            config: Arc::new(config),
            database: Mutex::new(None),
            is_initialized: AtomicBool::new(false),
            raft_status: Arc::new(AtomicRaftStatus::new(node_id)),
            cluster_members: RwLock::new(HashMap::new()),
            cluster_size: AtomicU64::new(0),
        }
    }
    
    /// Get the node configuration (no locking)
    #[inline]
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }
    
    /// Get the node ID (no locking)
    #[inline]
    pub fn node_id(&self) -> u64 {
        self.config.id
    }
    
    /// Check if initialized (lock-free)
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.is_initialized.load(Ordering::Acquire)
    }
    
    /// Set initialized state (lock-free)
    #[inline]
    pub fn set_initialized(&self, initialized: bool) {
        self.is_initialized.store(initialized, Ordering::Release);
    }
    
    /// Check if leader (lock-free)
    #[inline]
    pub fn is_leader(&self) -> bool {
        self.raft_status.is_leader.load(Ordering::Acquire)
    }
    
    /// Set leader state (lock-free)
    #[inline]
    pub fn set_leader(&self, is_leader: bool) {
        self.raft_status.is_leader.store(is_leader, Ordering::Release);
    }
    
    /// Get leader ID (lock-free)
    #[inline]
    pub fn leader_id(&self) -> Option<u64> {
        let id = self.raft_status.leader_id.load(Ordering::Acquire);
        if id == 0 { None } else { Some(id) }
    }
    
    /// Set leader ID (lock-free)
    #[inline]
    pub fn set_leader_id(&self, leader_id: Option<u64>) {
        self.raft_status.leader_id.store(
            leader_id.unwrap_or(0), 
            Ordering::Release
        );
    }
    
    /// Get current term (lock-free)
    #[inline]
    pub fn term(&self) -> u64 {
        self.raft_status.term.load(Ordering::Acquire)
    }
    
    /// Set current term (lock-free)
    #[inline]
    pub fn set_term(&self, term: u64) {
        self.raft_status.term.store(term, Ordering::Release);
    }
    
    /// Get cluster size (lock-free)
    #[inline]
    pub fn cluster_size(&self) -> u64 {
        self.cluster_size.load(Ordering::Acquire)
    }
    
    /// Get database handle
    pub fn database(&self) -> Option<Arc<Database>> {
        self.database.lock().clone()
    }
    
    /// Set database handle
    pub fn set_database(&self, db: Option<Arc<Database>>) {
        *self.database.lock() = db;
    }
    
    /// Get a specific cluster member (optimized for lookups)
    #[inline]
    pub fn get_cluster_member(&self, node_id: u64) -> Option<PeerInfo> {
        self.cluster_members.read().get(&node_id).cloned()
    }
    
    /// Check if a node is in the cluster (fast path)
    #[inline]
    pub fn has_cluster_member(&self, node_id: u64) -> bool {
        self.cluster_members.read().contains_key(&node_id)
    }
    
    /// Add or update a cluster member
    pub fn add_cluster_member(&self, node_id: u64, peer_info: PeerInfo) {
        let mut members = self.cluster_members.write();
        let is_new = !members.contains_key(&node_id);
        members.insert(node_id, peer_info);
        if is_new {
            self.cluster_size.fetch_add(1, Ordering::AcqRel);
        }
    }
    
    /// Remove a cluster member
    pub fn remove_cluster_member(&self, node_id: u64) -> Option<PeerInfo> {
        let mut members = self.cluster_members.write();
        let removed = members.remove(&node_id);
        if removed.is_some() {
            self.cluster_size.fetch_sub(1, Ordering::AcqRel);
        }
        removed
    }
    
    /// Get all cluster members (avoid in hot paths)
    pub fn cluster_members(&self) -> HashMap<u64, PeerInfo> {
        self.cluster_members.read().clone()
    }
    
    /// Execute a read operation on cluster members
    pub fn with_cluster_members<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<u64, PeerInfo>) -> R,
    {
        f(&*self.cluster_members.read())
    }
    
    /// Get Raft status snapshot
    pub fn get_raft_status(&self) -> crate::node_shared::RaftStatus {
        self.raft_status.snapshot()
    }
    
    /// Get all peer info (optimized to avoid cloning when possible)
    pub fn get_peers(&self) -> Vec<PeerInfo> {
        self.cluster_members.read().values().cloned().collect()
    }
    
    /// Get peer count without locking the full map
    #[inline]
    pub fn peer_count(&self) -> usize {
        self.cluster_size.load(Ordering::Acquire) as usize
    }
}

/// Wrapper to provide compatibility with existing SharedNodeState interface
pub struct OptimizedNodeStateWrapper {
    inner: Arc<OptimizedSharedNodeState>,
}

impl OptimizedNodeStateWrapper {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            inner: Arc::new(OptimizedSharedNodeState::new(config)),
        }
    }
    
    /// Get reference to the optimized state
    pub fn optimized(&self) -> &OptimizedSharedNodeState {
        &self.inner
    }
    
    // Delegate all methods to the optimized implementation
    pub fn config(&self) -> &NodeConfig {
        self.inner.config()
    }
    
    pub fn node_id(&self) -> u64 {
        self.inner.node_id()
    }
    
    pub fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }
    
    pub fn set_initialized(&self, initialized: bool) {
        self.inner.set_initialized(initialized)
    }
    
    pub async fn database(&self) -> Option<Arc<Database>> {
        self.inner.database()
    }
    
    pub async fn set_database(&self, db: Option<Arc<Database>>) {
        self.inner.set_database(db)
    }
    
    pub fn is_leader(&self) -> bool {
        self.inner.is_leader()
    }
    
    pub fn set_leader(&self, is_leader: bool) {
        self.inner.set_leader(is_leader)
    }
    
    pub fn leader_id(&self) -> Option<u64> {
        self.inner.leader_id()
    }
    
    pub fn set_leader_id(&self, leader_id: Option<u64>) {
        self.inner.set_leader_id(leader_id)
    }
    
    pub fn cluster_members(&self) -> HashMap<u64, PeerInfo> {
        self.inner.cluster_members()
    }
    
    pub fn add_cluster_member(&self, node_id: u64, peer_info: PeerInfo) {
        self.inner.add_cluster_member(node_id, peer_info)
    }
    
    pub fn remove_cluster_member(&self, node_id: u64) {
        self.inner.remove_cluster_member(node_id);
    }
    
    pub fn get_cluster_member(&self, node_id: u64) -> Option<PeerInfo> {
        self.inner.get_cluster_member(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lock_free_operations() {
        let state = OptimizedSharedNodeState::new(NodeConfig {
            id: 1,
            bind_addr: "127.0.0.1:7001".to_string(),
            data_dir: "/tmp/test".into(),
        });
        
        // Test atomic operations
        assert!(!state.is_initialized());
        state.set_initialized(true);
        assert!(state.is_initialized());
        
        assert!(!state.is_leader());
        state.set_leader(true);
        assert!(state.is_leader());
        
        assert_eq!(state.leader_id(), None);
        state.set_leader_id(Some(42));
        assert_eq!(state.leader_id(), Some(42));
        
        assert_eq!(state.term(), 0);
        state.set_term(5);
        assert_eq!(state.term(), 5);
    }
    
    #[test]
    fn test_cluster_member_operations() {
        let state = OptimizedSharedNodeState::new(NodeConfig::default());
        
        assert_eq!(state.cluster_size(), 0);
        
        let peer1 = PeerInfo {
            node_id: "2".to_string(),
            address: "127.0.0.1:7002".to_string(),
            last_seen: chrono::Utc::now(),
            capabilities: vec![],
            shared_resources: std::collections::HashMap::new(),
            connection_quality: crate::p2p_manager::ConnectionQuality {
                latency_ms: 0,
                bandwidth_mbps: 100.0,
                packet_loss: 0.0,
                reliability_score: 1.0,
            },
            p2p_node_id: Some("test".to_string()),
        };
        
        state.add_cluster_member(2, peer1.clone());
        assert_eq!(state.cluster_size(), 1);
        assert!(state.has_cluster_member(2));
        
        let retrieved = state.get_cluster_member(2).unwrap();
        assert_eq!(retrieved.node_id, 2);
        
        state.remove_cluster_member(2);
        assert_eq!(state.cluster_size(), 0);
        assert!(!state.has_cluster_member(2));
    }
}