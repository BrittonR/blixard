//! Shared node state management module
//!
//! This module provides the core shared state types used throughout the Blixard system:
//! - SharedNodeState: Central node state container
//! - PeerInfo: Peer information for cluster management

use crate::{
    error::{BlixardError, BlixardResult},
    types::{NodeConfig, NodeTopology},
    raft::proposals::WorkerCapabilities,
};
use redb::Database;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;

// Re-export PeerInfo from p2p_manager for backward compatibility
pub use crate::p2p_manager::PeerInfo;

/// Raft consensus status information
#[derive(Debug, Clone)]
pub struct RaftStatus {
    pub is_leader: bool,
    pub node_id: u64,
    pub leader_id: Option<u64>,
    pub term: u64,
    pub state: String,
}

/// Central shared state for a Blixard node
/// 
/// This contains all the shared state that multiple components need to access,
/// including cluster membership, database handle, configuration, etc.
#[derive(Debug)]
pub struct SharedNodeState {
    /// Node configuration
    pub config: NodeConfig,
    
    /// Database handle (when available)
    database: Arc<Mutex<Option<Arc<Database>>>>,
    
    /// Whether the node is initialized and ready to serve requests
    is_initialized: Arc<RwLock<bool>>,
    
    /// Current cluster membership information
    cluster_members: Arc<RwLock<HashMap<u64, PeerInfo>>>,
    
    /// Current leader node ID (if known)
    leader_id: Arc<RwLock<Option<u64>>>,
    
    /// Local node status flags
    is_leader: Arc<RwLock<bool>>,
}

impl SharedNodeState {
    /// Create new SharedNodeState with the given configuration
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            database: Arc::new(Mutex::new(None)),
            is_initialized: Arc::new(RwLock::new(false)),
            cluster_members: Arc::new(RwLock::new(HashMap::new())),
            leader_id: Arc::new(RwLock::new(None)),
            is_leader: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Create new SharedNodeState with default configuration (for tests)
    pub fn new_default() -> Self {
        Self::new(NodeConfig::default())
    }
    
    /// Get the node configuration
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }
    
    /// Get the node ID
    pub fn node_id(&self) -> u64 {
        self.config.id
    }
    
    /// Check if the node is initialized
    pub fn is_initialized(&self) -> bool {
        *self.is_initialized.read().unwrap()
    }
    
    /// Set the initialized state
    pub fn set_initialized(&self, initialized: bool) {
        *self.is_initialized.write().unwrap() = initialized;
    }
    
    /// Get the database handle
    pub async fn database(&self) -> Option<Arc<Database>> {
        self.database.lock().await.clone()
    }
    
    /// Set the database handle
    pub async fn set_database(&self, db: Option<Arc<Database>>) {
        *self.database.lock().await = db;
    }
    
    /// Check if this node is the current leader
    pub fn is_leader(&self) -> bool {
        *self.is_leader.read().unwrap()
    }
    
    /// Set whether this node is the leader
    pub fn set_leader(&self, is_leader: bool) {
        *self.is_leader.write().unwrap() = is_leader;
    }
    
    /// Get the current leader ID
    pub fn leader_id(&self) -> Option<u64> {
        *self.leader_id.read().unwrap()
    }
    
    /// Set the current leader ID
    pub fn set_leader_id(&self, leader_id: Option<u64>) {
        *self.leader_id.write().unwrap() = leader_id;
    }
    
    /// Get cluster members (returns a clone for backward compatibility)
    /// Consider using with_cluster_members() to avoid cloning
    pub fn cluster_members(&self) -> HashMap<u64, PeerInfo> {
        self.cluster_members.read().unwrap().clone()
    }
    
    /// Access cluster members without cloning
    pub fn with_cluster_members<F, R>(&self, f: F) -> R 
    where 
        F: FnOnce(&HashMap<u64, PeerInfo>) -> R
    {
        let members = self.cluster_members.read().unwrap();
        f(&*members)
    }
    
    /// Get the number of cluster members
    pub fn cluster_member_count(&self) -> usize {
        self.cluster_members.read().unwrap().len()
    }
    
    /// Add or update a cluster member
    pub fn add_cluster_member(&self, node_id: u64, peer_info: PeerInfo) {
        self.cluster_members.write().unwrap().insert(node_id, peer_info);
    }
    
    /// Remove a cluster member
    pub fn remove_cluster_member(&self, node_id: u64) {
        self.cluster_members.write().unwrap().remove(&node_id);
    }
    
    /// Get a specific cluster member
    pub fn get_cluster_member(&self, node_id: u64) -> Option<PeerInfo> {
        self.cluster_members.read().unwrap().get(&node_id).cloned()
    }
    
    // Essential getter methods needed throughout the codebase
    
    /// Get the node ID (convenience method)
    pub fn get_id(&self) -> u64 {
        self.config.id
    }
    
    /// Get the bind address
    pub fn get_bind_addr(&self) -> String {
        self.config.bind_addr.clone()
    }
    
    /// Get the database (async convenience method)
    pub async fn get_database(&self) -> Option<Arc<Database>> {
        self.database().await
    }
    
    /// Check if the node is running (placeholder implementation)
    pub fn is_running(&self) -> bool {
        self.is_initialized()
    }
    
    // Placeholder stub methods for compilation
    // TODO: These need proper implementations with actual state management
    
    pub fn get_raft_status(&self) -> RaftStatus {
        RaftStatus {
            is_leader: self.is_leader(),
            node_id: self.node_id(),
            leader_id: self.leader_id(),
            term: 0, // TODO: track actual term
            state: if self.is_leader() { "Leader".to_string() } else { "Follower".to_string() },
        }
    }
    
    pub fn get_peers(&self) -> Vec<PeerInfo> {
        self.with_cluster_members(|members| {
            members.values().cloned().collect()
        })
    }
    
    pub fn get_peer(&self, node_id: u64) -> Option<PeerInfo> {
        self.get_cluster_member(node_id)
    }
    
    // VM-related stub methods
    pub async fn create_vm(&self, _config: crate::types::VmConfig) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "create_vm in SharedNodeState".to_string() })
    }
    
    pub async fn start_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "start_vm in SharedNodeState".to_string() })
    }
    
    pub async fn stop_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "stop_vm in SharedNodeState".to_string() })
    }
    
    pub async fn delete_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "delete_vm in SharedNodeState".to_string() })
    }
    
    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::types::VmState>> {
        Err(BlixardError::NotImplemented { feature: "list_vms in SharedNodeState".to_string() })
    }
    
    pub async fn get_vm_info(&self, _vm_name: &str) -> BlixardResult<crate::types::VmState> {
        Err(BlixardError::NotImplemented { feature: "get_vm_info in SharedNodeState".to_string() })
    }
    
    pub async fn get_vm_status(&self, _vm_name: &str) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "get_vm_status in SharedNodeState".to_string() })
    }
    
    pub async fn get_vm_ip(&self, _vm_name: &str) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "get_vm_ip in SharedNodeState".to_string() })
    }
    
    pub async fn migrate_vm(&self, _vm_name: &str, _target_node: u64) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "migrate_vm in SharedNodeState".to_string() })
    }
    
    // Manager getters (return None for now - managers should be injected separately)
    pub fn get_p2p_manager(&self) -> Option<Arc<crate::p2p_manager::P2pManager>> {
        None // TODO: Implement manager storage
    }
    
    pub fn get_vm_manager(&self) -> Option<Arc<dyn crate::vm_backend::VmBackend>> {
        None // TODO: Implement manager storage  
    }
    
    pub fn get_peer_connector(&self) -> Option<Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>> {
        None // TODO: Implement manager storage
    }
    
    pub fn get_vm_health_monitor(&self) -> Option<Arc<crate::vm_health_monitor::VmHealthMonitor>> {
        None // TODO: Implement manager storage
    }
    
    pub fn get_vm_auto_recovery(&self) -> Option<Arc<crate::vm_auto_recovery::VmAutoRecovery>> {
        None // TODO: Implement manager storage
    }
    
    pub fn get_discovery_manager(&self) -> Option<Arc<crate::discovery::DiscoveryManager>> {
        None // TODO: Implement manager storage
    }
    
    // Lifecycle methods
    pub fn set_running(&self, _running: bool) {
        // TODO: Implement running state tracking
    }
    
    pub async fn shutdown_components(&self) -> BlixardResult<()> {
        // TODO: Implement component shutdown
        Ok(())
    }
    
    // Additional essential methods from the missing method analysis
    
    pub async fn send_vm_command(&self, _vm_name: &str, _command: String) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "send_vm_command in SharedNodeState".to_string() })
    }
    
    pub fn update_peer_connection(&self, _node_id: u64, _status: String) {
        // TODO: Implement peer connection status tracking
    }
    
    pub fn add_peer_with_p2p(&self, _node_id: u64, _peer_info: PeerInfo) {
        self.add_cluster_member(_node_id, _peer_info);
    }
    
    pub fn get_p2p_node_addr(&self) -> Option<String> {
        // TODO: Return actual P2P node address
        None
    }
    
    pub fn get_iroh_endpoint(&self) -> Option<String> {
        // TODO: Return actual Iroh endpoint
        None
    }
    
    pub async fn submit_task(&self, _task_id: &str, _task: crate::raft_manager::TaskSpec) -> BlixardResult<u64> {
        Err(BlixardError::NotImplemented { feature: "submit_task in SharedNodeState".to_string() })
    }
    
    pub fn set_raft_proposal_tx(&self, _tx: tokio::sync::mpsc::UnboundedSender<crate::raft::messages::RaftProposal>) {
        // TODO: Store raft proposal channel
    }
    
    pub fn set_raft_message_tx(&self, _tx: tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>) {
        // TODO: Store raft message channel
    }
    
    pub async fn send_raft_proposal(&self, _proposal: crate::raft::messages::RaftProposal) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "send_raft_proposal in SharedNodeState".to_string() })
    }
    
    pub fn remove_peer(&self, node_id: u64) {
        self.remove_cluster_member(node_id);
    }
    
    pub async fn update_vm_status_through_raft(&self, _vm_name: &str, _status: String) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "update_vm_status_through_raft in SharedNodeState".to_string() })
    }
    
    pub async fn register_worker_through_raft(
        &self,
        _node_id: u64,
        _address: String,
        _capabilities: WorkerCapabilities,
        _topology: NodeTopology,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "register_worker_through_raft in SharedNodeState".to_string() })
    }
    
    pub async fn get_worker_capabilities(&self) -> BlixardResult<WorkerCapabilities> {
        // Return hardcoded capabilities for now
        Ok(WorkerCapabilities {
            cpu_cores: 8,
            memory_mb: 16384,
            disk_gb: 100,
            features: vec!["microvm".to_string()],
        })
    }
    
    pub async fn get_task_status(&self, _task_id: &str) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        Err(BlixardError::NotImplemented { feature: "get_task_status in SharedNodeState".to_string() })
    }
    
    pub async fn create_vm_with_scheduling(&self, _config: crate::types::VmConfig) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "create_vm_with_scheduling in SharedNodeState".to_string() })
    }
    
    pub fn add_peer(&self, node_id: u64, peer_info: PeerInfo) {
        self.add_cluster_member(node_id, peer_info);
    }
    
    pub async fn send_raft_message(&self, _from: u64, _msg: raft::prelude::Message) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented { feature: "send_raft_message in SharedNodeState".to_string() })
    }
    
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        Err(BlixardError::NotImplemented { feature: "get_cluster_status in SharedNodeState".to_string() })
    }
    
    // More lifecycle and management methods
    pub fn set_vm_manager(&self, _vm_manager: Arc<dyn crate::vm_backend::VmBackend>) {
        // TODO: Store VM manager reference
    }
    
    pub fn set_quota_manager(&self, _quota_manager: Arc<crate::quota_manager::QuotaManager>) {
        // TODO: Store quota manager reference  
    }
    
    pub fn set_ip_pool_manager(&self, _ip_manager: Arc<crate::ip_pool_manager::IpPoolManager>) {
        // TODO: Store IP pool manager reference
    }
    
    pub fn set_peer_connector(&self, _connector: Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>) {
        // TODO: Store peer connector reference
    }
    
    pub fn set_shutdown_tx(&self, _tx: tokio::sync::oneshot::Sender<()>) {
        // TODO: Store shutdown channel
    }
    
    pub fn take_shutdown_tx(&self) -> Option<tokio::sync::oneshot::Sender<()>> {
        // TODO: Return and remove shutdown channel
        None
    }
    
    pub fn update_raft_status(&self, _status: RaftStatus) {
        // TODO: Update raft status
    }
    
    pub async fn create_vm_through_raft(&self, _config: crate::types::VmConfig) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "create_vm_through_raft in SharedNodeState".to_string() })
    }
    
    pub fn get_vm_count(&self) -> u64 {
        0 // TODO: Track actual VM count
    }
    
    pub fn get_active_connection_count(&self) -> u64 {
        self.cluster_members().len() as u64
    }
    
    pub fn get_start_time(&self) -> String {
        // TODO: Track actual start time
        "unknown".to_string()
    }
    
    pub async fn get_vm_placement_recommendation(&self, _config: crate::types::VmConfig) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented { feature: "get_vm_placement_recommendation in SharedNodeState".to_string() })
    }
}

impl Default for SharedNodeState {
    fn default() -> Self {
        Self::new_default()
    }
}

// Additional helper types that might be needed
pub type SharedNodeStateRef = Arc<SharedNodeState>;