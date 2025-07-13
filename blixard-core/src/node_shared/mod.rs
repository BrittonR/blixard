//! Shared node state management module
//!
//! This module provides the core shared state types used throughout the Blixard system:
//! - SharedNodeState: Central node state container
//! - PeerInfo: Peer information for cluster management

use crate::{
    common::async_utils::{quick_read, quick_read_extract, OptionalArc, AtomicCounter},
    error::{BlixardError, BlixardResult},
    raft::proposals::WorkerCapabilities,
    raft_manager::ConfChangeType,
    types::{NodeConfig, NodeTopology},
};
use redb::Database;
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{RwLock as AsyncRwLock, Mutex};
use iroh::{Endpoint, NodeAddr};

// Type aliases for complex types
type DatabaseHandle = OptionalArc<Database>;
type ClusterMembers = Arc<AsyncRwLock<HashMap<u64, PeerInfo>>>;
type LeaderIdState = Arc<AsyncRwLock<Option<u64>>>;

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
    database: DatabaseHandle,

    /// Whether the node is initialized and ready to serve requests
    is_initialized: AtomicCounter,

    /// Current cluster membership information
    cluster_members: ClusterMembers,

    /// Current leader node ID (if known)
    leader_id: LeaderIdState,

    /// Local node status flags
    is_leader: AtomicCounter,

    /// Iroh endpoint for P2P communication (when available)
    iroh_endpoint: Arc<Mutex<Option<Endpoint>>>,

    /// Our Iroh node address for peer discovery
    iroh_node_addr: Arc<Mutex<Option<NodeAddr>>>,

    /// Peer connector for managing P2P connections
    peer_connector: Arc<Mutex<Option<Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>>>>,
}

impl SharedNodeState {
    /// Create new SharedNodeState with the given configuration
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            database: DatabaseHandle::new(),
            is_initialized: AtomicCounter::new(0),
            cluster_members: ClusterMembers::new(AsyncRwLock::new(HashMap::new())),
            leader_id: LeaderIdState::new(AsyncRwLock::new(None)),
            is_leader: AtomicCounter::new(0),
            iroh_endpoint: Arc::new(Mutex::new(None)),
            iroh_node_addr: Arc::new(Mutex::new(None)),
            peer_connector: Arc::new(Mutex::new(None)),
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
        self.is_initialized.get() > 0
    }

    /// Set the initialized state
    pub fn set_initialized(&self, initialized: bool) {
        self.is_initialized.set(if initialized { 1 } else { 0 });
    }

    /// Get the database handle
    pub async fn database(&self) -> Option<Arc<Database>> {
        self.database.get().await
    }

    /// Set the database handle
    pub async fn set_database(&self, db: Option<Arc<Database>>) {
        self.database.set(db).await;
    }

    /// Check if this node is the current leader
    pub fn is_leader(&self) -> bool {
        self.is_leader.get() > 0
    }

    /// Set whether this node is the leader
    pub fn set_leader(&self, is_leader: bool) {
        self.is_leader.set(if is_leader { 1 } else { 0 });
    }

    /// Get the current leader ID
    pub async fn leader_id(&self) -> Option<u64> {
        quick_read_extract(&self.leader_id, |id| *id).await
    }

    /// Set the current leader ID
    pub async fn set_leader_id(&self, leader_id: Option<u64>) {
        let mut guard = self.leader_id.write().await;
        *guard = leader_id;
    }

    /// Get cluster members (returns a clone for backward compatibility)
    /// Consider using with_cluster_members() to avoid cloning
    pub async fn cluster_members(&self) -> HashMap<u64, PeerInfo> {
        quick_read(&self.cluster_members, |members| members.clone()).await
    }

    /// Access cluster members without cloning
    pub async fn with_cluster_members<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<u64, PeerInfo>) -> R,
    {
        quick_read_extract(&self.cluster_members, f).await
    }

    /// Get the number of cluster members
    pub async fn cluster_member_count(&self) -> usize {
        quick_read_extract(&self.cluster_members, |members| members.len()).await
    }

    /// Add or update a cluster member
    pub async fn add_cluster_member(&self, node_id: u64, peer_info: PeerInfo) {
        let mut guard = self.cluster_members.write().await;
        guard.insert(node_id, peer_info);
    }

    /// Remove a cluster member
    pub async fn remove_cluster_member(&self, node_id: u64) {
        let mut guard = self.cluster_members.write().await;
        guard.remove(&node_id);
    }

    /// Get a specific cluster member
    pub async fn get_cluster_member(&self, node_id: u64) -> Option<PeerInfo> {
        quick_read_extract(&self.cluster_members, |members| members.get(&node_id).cloned()).await
    }

    // Essential getter methods needed throughout the codebase

    /// Get the node ID (convenience method)
    pub fn get_id(&self) -> u64 {
        self.config.id
    }

    /// Get the bind address
    pub fn get_bind_addr(&self) -> String {
        self.config.bind_addr.to_string()
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

    pub async fn get_raft_status(&self) -> RaftStatus {
        RaftStatus {
            is_leader: self.is_leader(),
            node_id: self.node_id(),
            leader_id: self.leader_id().await,
            term: 0, // TODO: track actual term
            state: if self.is_leader() {
                "Leader".to_string()
            } else {
                "Follower".to_string()
            },
        }
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.with_cluster_members(|members| members.values().cloned().collect()).await
    }

    pub async fn get_peer(&self, node_id: u64) -> Option<PeerInfo> {
        self.get_cluster_member(node_id).await
    }

    // VM-related stub methods
    pub async fn create_vm(&self, _config: crate::types::VmConfig) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "create_vm in SharedNodeState".to_string(),
        })
    }

    pub async fn start_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "start_vm in SharedNodeState".to_string(),
        })
    }

    pub async fn stop_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "stop_vm in SharedNodeState".to_string(),
        })
    }

    pub async fn delete_vm(&self, _vm_name: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "delete_vm in SharedNodeState".to_string(),
        })
    }

    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::types::VmState>> {
        Err(BlixardError::NotImplemented {
            feature: "list_vms in SharedNodeState".to_string(),
        })
    }

    pub async fn get_vm_info(&self, _vm_name: &str) -> BlixardResult<crate::types::VmState> {
        Err(BlixardError::NotImplemented {
            feature: "get_vm_info in SharedNodeState".to_string(),
        })
    }

    pub async fn get_vm_status(&self, _vm_name: &str) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "get_vm_status in SharedNodeState".to_string(),
        })
    }

    pub async fn get_vm_ip(&self, _vm_name: &str) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "get_vm_ip in SharedNodeState".to_string(),
        })
    }

    pub async fn migrate_vm(&self, _vm_name: &str, _target_node: u64) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "migrate_vm in SharedNodeState".to_string(),
        })
    }

    // Manager getters (return None for now - managers should be injected separately)
    pub fn get_p2p_manager(&self) -> Option<Arc<crate::p2p_manager::P2pManager>> {
        None // TODO: Implement manager storage
    }

    pub fn get_vm_manager(&self) -> Option<Arc<dyn crate::vm_backend::VmBackend>> {
        None // TODO: Implement manager storage
    }

    pub fn get_peer_connector(
        &self,
    ) -> Option<Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>> {
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
        Err(BlixardError::NotImplemented {
            feature: "send_vm_command in SharedNodeState".to_string(),
        })
    }

    pub fn update_peer_connection(&self, _node_id: u64, _status: String) {
        // TODO: Implement peer connection status tracking
    }

    pub async fn add_peer_with_p2p(&self, _node_id: u64, _peer_info: PeerInfo) {
        self.add_cluster_member(_node_id, _peer_info).await;
    }

    pub async fn get_p2p_node_addr(&self) -> Option<NodeAddr> {
        let addr = self.iroh_node_addr.lock().await;
        addr.clone()
    }

    pub async fn get_iroh_endpoint(&self) -> Option<Endpoint> {
        let endpoint = self.iroh_endpoint.lock().await;
        endpoint.clone()
    }

    /// Set the Iroh endpoint
    pub async fn set_iroh_endpoint(&self, endpoint: Option<Endpoint>) {
        let mut stored_endpoint = self.iroh_endpoint.lock().await;
        *stored_endpoint = endpoint.clone();
        
        // If we have an endpoint, also store the node address
        if let Some(ref ep) = endpoint {
            let node_id = ep.node_id();
            let node_addr = NodeAddr::new(node_id);
            let mut stored_addr = self.iroh_node_addr.lock().await;
            *stored_addr = Some(node_addr);
        }
    }

    pub async fn submit_task(
        &self,
        _task_id: &str,
        _task: crate::raft_manager::TaskSpec,
    ) -> BlixardResult<u64> {
        Err(BlixardError::NotImplemented {
            feature: "submit_task in SharedNodeState".to_string(),
        })
    }

    pub fn set_raft_proposal_tx(
        &self,
        _tx: tokio::sync::mpsc::UnboundedSender<crate::raft::messages::RaftProposal>,
    ) {
        // TODO: Store raft proposal channel
    }

    pub fn set_raft_message_tx(
        &self,
        _tx: tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    ) {
        // TODO: Store raft message channel
    }

    pub async fn send_raft_proposal(
        &self,
        _proposal: crate::raft::messages::RaftProposal,
    ) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "send_raft_proposal in SharedNodeState".to_string(),
        })
    }

    pub async fn remove_peer(&self, node_id: u64) {
        self.remove_cluster_member(node_id).await;
    }

    pub async fn update_vm_status_through_raft(
        &self,
        _vm_name: &str,
        _status: String,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "update_vm_status_through_raft in SharedNodeState".to_string(),
        })
    }

    pub async fn register_worker_through_raft(
        &self,
        _node_id: u64,
        _address: String,
        _capabilities: WorkerCapabilities,
        _topology: NodeTopology,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "register_worker_through_raft in SharedNodeState".to_string(),
        })
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

    pub async fn get_task_status(
        &self,
        _task_id: &str,
    ) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        Err(BlixardError::NotImplemented {
            feature: "get_task_status in SharedNodeState".to_string(),
        })
    }

    /// Schedule VM placement using the intelligent scheduler
    ///
    /// This method delegates to the VM manager for scheduling decisions
    pub async fn schedule_vm_placement(
        &self,
        vm_config: &crate::types::VmConfig,
        strategy: crate::vm_scheduler::PlacementStrategy,
    ) -> BlixardResult<crate::vm_scheduler::PlacementDecision> {
        // Get database handle
        let database = self
            .database()
            .await
            .ok_or_else(|| BlixardError::DatabaseError {
                operation: "get database for VM scheduling".to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Database not available",
                )),
            })?;

        // Create VM scheduler and schedule placement
        let scheduler = crate::vm_scheduler::VmScheduler::new(database);
        scheduler.schedule_vm_placement(vm_config, strategy).await
    }

    pub async fn create_vm_with_scheduling(
        &self,
        _config: crate::types::VmConfig,
    ) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "create_vm_with_scheduling in SharedNodeState".to_string(),
        })
    }

    pub async fn add_peer(&self, node_id: u64, peer_info: PeerInfo) {
        self.add_cluster_member(node_id, peer_info).await;
    }

    pub async fn send_raft_message(
        &self,
        _from: u64,
        _msg: raft::prelude::Message,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "send_raft_message in SharedNodeState".to_string(),
        })
    }

    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        Err(BlixardError::NotImplemented {
            feature: "get_cluster_status in SharedNodeState".to_string(),
        })
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

    pub fn set_peer_connector(
        &self,
        connector: Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>,
    ) {
        tokio::spawn({
            let peer_connector = self.peer_connector.clone();
            async move {
                let mut stored_connector = peer_connector.lock().await;
                *stored_connector = Some(connector);
            }
        });
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

    pub async fn create_vm_through_raft(
        &self,
        _config: crate::types::VmConfig,
    ) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "create_vm_through_raft in SharedNodeState".to_string(),
        })
    }

    pub fn get_vm_count(&self) -> u64 {
        0 // TODO: Track actual VM count
    }

    pub async fn get_active_connection_count(&self) -> u64 {
        self.cluster_member_count().await as u64
    }

    pub fn get_start_time(&self) -> String {
        // TODO: Track actual start time
        "unknown".to_string()
    }

    pub async fn get_vm_placement_recommendation(
        &self,
        _config: crate::types::VmConfig,
    ) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "get_vm_placement_recommendation in SharedNodeState".to_string(),
        })
    }

    // Missing Raft-related methods
    pub async fn propose_conf_change_with_p2p(
        &self,
        _change_type: ConfChangeType,
        _node_id: u64,
        _context: String,
        _p2p_node_id: Option<String>,
        _p2p_addresses: Vec<String>,
        _p2p_relay_url: Option<String>,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "propose_conf_change_with_p2p in SharedNodeState".to_string(),
        })
    }

    pub async fn propose_conf_change(
        &self,
        _change_type: ConfChangeType,
        _node_id: u64,
        _context: String,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "propose_conf_change in SharedNodeState".to_string(),
        })
    }

    pub async fn get_current_voters(&self) -> BlixardResult<Vec<u64>> {
        Err(BlixardError::NotImplemented {
            feature: "get_current_voters in SharedNodeState".to_string(),
        })
    }
}

impl Default for SharedNodeState {
    fn default() -> Self {
        Self::new_default()
    }
}

// Additional helper types that might be needed
pub type SharedNodeStateRef = Arc<SharedNodeState>;
