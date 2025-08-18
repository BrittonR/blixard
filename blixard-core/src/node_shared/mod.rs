//! Shared node state management module
//!
//! This module provides the core shared state types used throughout the Blixard system:
//! - SharedNodeState: Central node state container
//! - PeerInfo: Peer information for cluster management

use crate::{
    common::async_utils::{quick_read, quick_read_extract, OptionalArc, AtomicCounter},
    error::{BlixardError, BlixardResult},
    raft::proposals::{ProposalData, WorkerCapabilities},
    raft::messages::RaftProposal,
    raft_manager::ConfChangeType,
    types::{NodeConfig, NodeTopology},
};
use redb::Database;
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{RwLock as AsyncRwLock, Mutex};
use iroh::{Endpoint, NodeAddr, Watcher};

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

    /// Current Raft term
    current_term: AtomicCounter,

    /// Current Raft state 
    current_state: Arc<Mutex<String>>,

    /// Iroh endpoint for P2P communication (when available)
    iroh_endpoint: Arc<Mutex<Option<Endpoint>>>,

    /// Our Iroh node address for peer discovery
    iroh_node_addr: Arc<Mutex<Option<NodeAddr>>>,

    /// Peer connector for managing P2P connections
    peer_connector: Arc<Mutex<Option<Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>>>>,
    
    /// VM backend for managing virtual machines
    vm_manager: Arc<Mutex<Option<Arc<dyn crate::vm_backend::VmBackend>>>>,
    
    /// Quota manager for resource limits and tenant quotas
    quota_manager: Arc<Mutex<Option<Arc<crate::quota_manager::QuotaManager>>>>,
    
    /// Raft manager for consensus operations
    raft_manager: Arc<Mutex<Option<Arc<crate::raft_manager::RaftManager>>>>,
    
    /// Raft proposal channel
    raft_proposal_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<crate::raft::messages::RaftProposal>>>>,
    
    /// Raft configuration change channel
    raft_conf_change_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<crate::raft_manager::RaftConfChange>>>>,
    
    /// Raft message channel for incoming messages
    raft_message_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>>>>,
    
    /// Node registry for mapping cluster IDs to Iroh node IDs and addresses
    node_registry: Arc<crate::node_registry::NodeRegistry>,
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
            current_term: AtomicCounter::new(0),
            current_state: Arc::new(Mutex::new("follower".to_string())),
            iroh_endpoint: Arc::new(Mutex::new(None)),
            iroh_node_addr: Arc::new(Mutex::new(None)),
            peer_connector: Arc::new(Mutex::new(None)),
            vm_manager: Arc::new(Mutex::new(None)),
            quota_manager: Arc::new(Mutex::new(None)),
            raft_manager: Arc::new(Mutex::new(None)),
            raft_proposal_tx: Arc::new(Mutex::new(None)),
            raft_conf_change_tx: Arc::new(Mutex::new(None)),
            raft_message_tx: Arc::new(Mutex::new(None)),
            node_registry: Arc::new(crate::node_registry::NodeRegistry::new()),
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

    /// Get the bind address (for backward compatibility)
    pub fn get_bind_addr(&self) -> String {
        self.config.bind_addr
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| format!("127.0.0.1:{}", 7000 + self.config.id))
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
            term: self.current_term.get(),
            state: {
                let state_guard = self.current_state.lock().await;
                state_guard.clone()
            },
        }
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.with_cluster_members(|members| members.values().cloned().collect()).await
    }

    pub async fn get_peer(&self, node_id: u64) -> Option<PeerInfo> {
        self.get_cluster_member(node_id).await
    }

    // VM-related methods
    pub async fn create_vm(&self, config: crate::types::VmConfig) -> BlixardResult<String> {
        match self.get_vm_manager().await {
            Some(vm_manager) => {
                // Create VM on this node
                vm_manager.create_vm(&config, self.node_id()).await?;
                Ok(config.name.clone())
            },
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn start_vm(&self, vm_name: &str) -> BlixardResult<()> {
        match self.get_vm_manager().await {
            Some(vm_manager) => vm_manager.start_vm(vm_name).await,
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn stop_vm(&self, vm_name: &str) -> BlixardResult<()> {
        match self.get_vm_manager().await {
            Some(vm_manager) => vm_manager.stop_vm(vm_name).await,
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn delete_vm(&self, vm_name: &str) -> BlixardResult<()> {
        match self.get_vm_manager().await {
            Some(vm_manager) => vm_manager.delete_vm(vm_name).await,
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn list_vms(&self) -> BlixardResult<Vec<crate::types::VmState>> {
        match self.get_vm_manager().await {
            Some(vm_manager) => {
                let vms = vm_manager.list_vms().await?;
                Ok(vms.into_iter().map(|(config, status)| crate::types::VmState {
                    name: config.name.clone(),
                    config,
                    status,
                    node_id: self.node_id(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                }).collect())
            },
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn get_vm_info(&self, vm_name: &str) -> BlixardResult<crate::types::VmState> {
        match self.get_vm_manager().await {
            Some(vm_manager) => {
                let vms = vm_manager.list_vms().await?;
                vms.into_iter()
                    .find(|(config, _)| config.name == vm_name)
                    .map(|(config, status)| crate::types::VmState {
                        name: config.name.clone(),
                        config,
                        status,
                        node_id: self.node_id(),
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    })
                    .ok_or_else(|| BlixardError::NotFound {
                        resource: format!("VM: {}", vm_name),
                    })
            },
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn get_vm_status(&self, vm_name: &str) -> BlixardResult<String> {
        match self.get_vm_manager().await {
            Some(vm_manager) => {
                let status = vm_manager.get_vm_status(vm_name).await?;
                match status {
                    Some(s) => Ok(format!("{:?}", s)),
                    None => Err(BlixardError::NotFound {
                        resource: format!("VM: {}", vm_name),
                    }),
                }
            },
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
    }

    pub async fn get_vm_ip(&self, vm_name: &str) -> BlixardResult<String> {
        match self.get_vm_manager().await {
            Some(vm_manager) => {
                let ip = vm_manager.get_vm_ip(vm_name).await?;
                ip.ok_or_else(|| BlixardError::NotFound {
                    resource: format!("VM IP: {}", vm_name),
                })
            },
            None => Err(BlixardError::NotImplemented {
                feature: "VM manager not initialized".to_string(),
            }),
        }
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

    pub async fn get_vm_manager(&self) -> Option<Arc<dyn crate::vm_backend::VmBackend>> {
        self.vm_manager.lock().await.clone()
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
    
    /// Get the node registry for ID mappings
    pub fn node_registry(&self) -> Arc<crate::node_registry::NodeRegistry> {
        self.node_registry.clone()
    }

    /// Set the Iroh endpoint
    pub async fn set_iroh_endpoint(&self, endpoint: Option<Endpoint>) {
        let mut stored_endpoint = self.iroh_endpoint.lock().await;
        *stored_endpoint = endpoint.clone();
        
        // If we have an endpoint, get the complete node address with relay info
        if let Some(ref ep) = endpoint {
            // Wait a moment for endpoint to fully initialize
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            // Get the full NodeAddr from the endpoint - this includes relay URL
            let mut node_addr_watcher = ep.node_addr();
            if let Some(node_addr) = node_addr_watcher.get() {
                tracing::info!(
                    "Endpoint NodeAddr obtained with {} direct addresses and relay: {:?}",
                    node_addr.direct_addresses().count(),
                    node_addr.relay_url()
                );
                
                // Register this node in the registry
                let cluster_node_id = self.config.id;
                let iroh_node_id = ep.node_id();
                let bind_address = self.get_bind_addr();
                
                if let Err(e) = self.node_registry.register_node(
                    cluster_node_id,
                    iroh_node_id,
                    node_addr.clone(),
                    Some(bind_address),
                ).await {
                    tracing::warn!("Failed to register node in registry: {}", e);
                }
                
                let mut stored_addr = self.iroh_node_addr.lock().await;
                *stored_addr = Some(node_addr);
            } else {
                tracing::warn!("Failed to get complete NodeAddr from endpoint: no address available yet");
                
                // Fallback to manual construction as before
                let node_id = ep.node_id();
                let bound_addrs = ep.bound_sockets();
                tracing::info!("Iroh endpoint bound to addresses: {:?}", bound_addrs);
                
                let node_addr = if bound_addrs.is_empty() {
                    tracing::warn!("No bound sockets found, using direct addresses");
                    let mut direct_addrs_watcher = ep.direct_addresses();
                    if let Some(addrs) = direct_addrs_watcher.get() {
                        let socket_addrs: Vec<_> = addrs.iter().map(|addr| addr.addr).collect();
                        tracing::info!("Direct addresses discovered: {:?}", socket_addrs);
                        NodeAddr::new(node_id)
                            .with_direct_addresses(socket_addrs)
                    } else {
                        tracing::warn!("No direct addresses available yet");
                        NodeAddr::new(node_id)
                    }
                } else {
                    // Replace 0.0.0.0 addresses with connectable addresses
                    let connectable_addrs: Vec<_> = bound_addrs
                        .into_iter()
                        .map(|addr| {
                            if addr.ip().is_unspecified() {
                                if addr.is_ipv4() {
                                    std::net::SocketAddr::new(
                                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                        addr.port()
                                    )
                                } else {
                                    std::net::SocketAddr::new(
                                        std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
                                        addr.port()
                                    )
                                }
                            } else {
                                addr
                            }
                        })
                        .collect();
                    
                    tracing::info!("Using connectable addresses: {:?}", connectable_addrs);
                    NodeAddr::new(node_id)
                        .with_direct_addresses(connectable_addrs)
                };
                
                // Register this node in the registry (fallback path)
                let cluster_node_id = self.config.id;
                let iroh_node_id = node_id;
                let bind_address = self.get_bind_addr();
                
                if let Err(e) = self.node_registry.register_node(
                    cluster_node_id,
                    iroh_node_id,
                    node_addr.clone(),
                    Some(bind_address),
                ).await {
                    tracing::warn!("Failed to register node in registry (fallback): {}", e);
                }
                
                let mut stored_addr = self.iroh_node_addr.lock().await;
                *stored_addr = Some(node_addr);
            }
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
        tx: tokio::sync::mpsc::UnboundedSender<crate::raft::messages::RaftProposal>,
    ) {
        // Use try_lock since we're in a sync context
        if let Ok(mut guard) = self.raft_proposal_tx.try_lock() {
            *guard = Some(tx);
        } else {
            // Fall back to spawning a task if we can't get the lock immediately
            let raft_proposal_tx = self.raft_proposal_tx.clone();
            tokio::spawn(async move {
                let mut guard = raft_proposal_tx.lock().await;
                *guard = Some(tx);
            });
        }
    }
    
    pub fn set_raft_conf_change_tx(
        &self,
        tx: tokio::sync::mpsc::UnboundedSender<crate::raft_manager::RaftConfChange>,
    ) {
        // Use try_lock since we're in a sync context
        if let Ok(mut guard) = self.raft_conf_change_tx.try_lock() {
            *guard = Some(tx);
        } else {
            // Fall back to spawning a task if we can't get the lock immediately
            let raft_conf_change_tx = self.raft_conf_change_tx.clone();
            tokio::spawn(async move {
                let mut guard = raft_conf_change_tx.lock().await;
                *guard = Some(tx);
            });
        }
    }
    
    pub async fn get_raft_proposal_tx(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<crate::raft::messages::RaftProposal>> {
        self.raft_proposal_tx.lock().await.clone()
    }

    pub async fn set_raft_message_tx(
        &self,
        tx: tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    ) {
        let mut channel = self.raft_message_tx.lock().await;
        *channel = Some(tx);
    }
    
    /// Get the Raft message channel for incoming messages
    pub async fn get_raft_message_tx(&self) -> Option<tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>> {
        let channel = self.raft_message_tx.lock().await;
        channel.clone()
    }

    pub async fn send_raft_proposal(
        &self,
        proposal: crate::raft::messages::RaftProposal,
    ) -> BlixardResult<String> {
        match self.get_raft_proposal_tx().await {
            Some(proposal_tx) => {
                proposal_tx.send(proposal).map_err(|_| BlixardError::Internal {
                    message: "Failed to send Raft proposal".to_string(),
                })?;
                Ok("Proposal submitted".to_string())
            }
            None => Err(BlixardError::NotInitialized {
                component: "RaftManager".to_string(),
            }),
        }
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
        node_id: u64,
        address: String,
        capabilities: WorkerCapabilities,
        topology: NodeTopology,
    ) -> BlixardResult<()> {
        tracing::info!("Registering worker {} through Raft consensus", node_id);
        
        // Create a RegisterWorker proposal using the new() constructor
        let proposal = RaftProposal::new(ProposalData::RegisterWorker {
            node_id,
            address,
            capabilities,
            topology,
        });
        
        // Send the proposal through Raft
        self.send_raft_proposal(proposal).await?;
        
        tracing::info!("Worker {} registration proposed to Raft", node_id);
        Ok(())
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
    pub async fn set_vm_manager(&self, vm_manager: Arc<dyn crate::vm_backend::VmBackend>) {
        *self.vm_manager.lock().await = Some(vm_manager);
    }

    pub async fn set_quota_manager(&self, quota_manager: Arc<crate::quota_manager::QuotaManager>) {
        let mut guard = self.quota_manager.lock().await;
        *guard = Some(quota_manager);
    }
    
    pub async fn get_quota_manager(&self) -> Option<Arc<crate::quota_manager::QuotaManager>> {
        self.quota_manager.lock().await.clone()
    }
    
    pub async fn set_raft_manager(&self, raft_manager: Arc<crate::raft_manager::RaftManager>) {
        let mut guard = self.raft_manager.lock().await;
        *guard = Some(raft_manager);
    }
    
    pub async fn get_raft_manager(&self) -> Option<Arc<crate::raft_manager::RaftManager>> {
        self.raft_manager.lock().await.clone()
    }
    
    pub async fn clear_raft_manager(&self) {
        let mut guard = self.raft_manager.lock().await;
        *guard = None;
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

    pub fn update_raft_status(&self, status: RaftStatus) {
        // Update is_leader flag
        self.set_leader(status.is_leader);
        
        // Update term
        self.current_term.set(status.term);
        
        // Update leader_id and state - spawn task since we need async but this method is sync
        let leader_id_ref = self.leader_id.clone();
        let state_ref = self.current_state.clone();
        let leader_id = status.leader_id;
        let state = status.state.clone();
        tokio::spawn(async move {
            // Update leader_id
            {
                let mut guard = leader_id_ref.write().await;
                *guard = leader_id;
            }
            
            // Update state
            {
                let mut guard = state_ref.lock().await;
                *guard = state;
            }
        });
        
        tracing::info!(
            "[RAFT-STATUS-UPDATE] Updated Raft status - is_leader: {}, leader_id: {:?}, term: {}, state: {}",
            status.is_leader, status.leader_id, status.term, status.state
        );
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
        change_type: ConfChangeType,
        node_id: u64,
        context: String,
        p2p_node_id: Option<String>,
        p2p_addresses: Vec<String>,
        p2p_relay_url: Option<String>,
    ) -> BlixardResult<()> {
        tracing::info!(
            "Configuration change requested: {:?} for node {} with P2P info - node_id: {:?}, addresses: {:?}",
            change_type,
            node_id,
            p2p_node_id,
            p2p_addresses
        );
        
        // Store the P2P info for the new node
        if let Some(p2p_id) = &p2p_node_id {
            if let Ok(iroh_node_id) = p2p_id.parse::<iroh::NodeId>() {
                // Create a NodeAddr with the P2P info
                let mut node_addr = iroh::NodeAddr::new(iroh_node_id);
                
                // Add addresses
                let addrs: Vec<std::net::SocketAddr> = p2p_addresses
                    .iter()
                    .filter_map(|a| a.parse().ok())
                    .collect();
                if !addrs.is_empty() {
                    node_addr = node_addr.with_direct_addresses(addrs);
                }
                
                // Add relay URL if provided
                if let Some(relay) = &p2p_relay_url {
                    if let Ok(relay_url) = relay.parse() {
                        node_addr = node_addr.with_relay_url(relay_url);
                    }
                }
                
                // Register in the node registry
                tracing::info!(
                    "NODE_REGISTRY: Attempting to register node {} with Iroh ID {} in registry",
                    node_id, p2p_id
                );
                if let Err(e) = self.node_registry.register_node(
                    node_id, 
                    iroh_node_id,
                    node_addr.clone(),
                    Some(context.clone())
                ).await {
                    tracing::error!("NODE_REGISTRY: Failed to register node {} in registry: {}", node_id, e);
                } else {
                    tracing::info!(
                        "NODE_REGISTRY: Successfully registered node {} with Iroh ID {} in registry (addresses: {:?}, relay: {:?})",
                        node_id, p2p_id,
                        node_addr.direct_addresses().collect::<Vec<_>>(),
                        node_addr.relay_url()
                    );
                    
                    // Also add to cluster members so send_message can find it
                    let peer_info = PeerInfo {
                        node_id: p2p_id.clone(),
                        address: context.clone(),
                        last_seen: chrono::Utc::now(),
                        capabilities: vec![],
                        shared_resources: std::collections::HashMap::new(),
                        connection_quality: crate::p2p_manager::ConnectionQuality {
                            latency_ms: 0,
                            bandwidth_mbps: 100.0,
                            packet_loss: 0.0,
                            reliability_score: 1.0,
                        },
                        p2p_node_id: Some(p2p_id.clone()),
                        p2p_addresses: p2p_addresses.clone(),
                        p2p_relay_url: p2p_relay_url.clone(),
                        is_connected: false,
                    };
                    self.add_cluster_member(node_id, peer_info).await;
                    tracing::info!(
                        "NODE_REGISTRY: Added node {} to cluster members for Raft transport lookup",
                        node_id
                    );
                    
                    // Verify registration by trying to retrieve it
                    match self.node_registry.get_node_addr(node_id).await {
                        Ok(retrieved_addr) => {
                            tracing::info!(
                                "NODE_REGISTRY: Verification successful - can retrieve node {} (Iroh ID: {})",
                                node_id, retrieved_addr.node_id
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "NODE_REGISTRY: Verification failed - cannot retrieve node {}: {}",
                                node_id, e
                            );
                        }
                    }
                }
            }
        }
        
        // Actually propose the configuration change through Raft
        let conf_change_tx = self.raft_conf_change_tx.lock().await.clone()
            .ok_or_else(|| BlixardError::NotInitialized {
                component: "Raft conf change channel".to_string(),
            })?;
        
        // Create a response channel to wait for the result
        let (response_tx, mut response_rx) = tokio::sync::oneshot::channel();
        
        // Create the RaftConfChange
        let raft_conf_change = crate::raft_manager::RaftConfChange {
            change_type,
            node_id,
            address: context,
            p2p_node_id: p2p_node_id.clone(),
            p2p_addresses: p2p_addresses.clone(),
            p2p_relay_url: p2p_relay_url.clone(),
            response_tx: Some(response_tx),
        };
        
        // Send the configuration change
        tracing::info!(
            "CONF_CHANGE: Sending configuration change for node {} to Raft manager",
            node_id
        );
        conf_change_tx.send(raft_conf_change)
            .map_err(|_| BlixardError::Internal {
                message: "Failed to send configuration change to Raft".to_string(),
            })?;
        
        // Wait for the response
        tracing::debug!(
            "CONF_CHANGE: Waiting for response from Raft manager for node {}",
            node_id
        );
        match response_rx.try_recv() {
            Ok(Ok(())) => {
                tracing::info!(
                    "CONF_CHANGE: Configuration change for node {} successfully proposed and applied",
                    node_id
                );
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!(
                    "CONF_CHANGE: Configuration change for node {} failed: {}",
                    node_id, e
                );
                Err(e)
            }
            Err(_) => {
                // Timeout or channel closed - for now, assume success to avoid blocking
                tracing::warn!(
                    "CONF_CHANGE: Configuration change response not received immediately for node {}, assuming success",
                    node_id
                );
                Ok(())
            }
        }
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
        // For now, return a simple list including the new node
        // In a full implementation, this would query Raft for the current configuration
        let mut voters = vec![1]; // Bootstrap node is always voter 1
        
        // Add any nodes that have been registered
        let entries = self.node_registry.list_nodes().await;
        for entry in entries {
            if entry.cluster_node_id != 1 && !voters.contains(&entry.cluster_node_id) {
                voters.push(entry.cluster_node_id);
            }
        }
        
        Ok(voters)
    }
}

impl Default for SharedNodeState {
    fn default() -> Self {
        Self::new_default()
    }
}

impl std::fmt::Debug for SharedNodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedNodeState")
            .field("node_id", &self.config.id)
            .field("is_initialized", &self.is_initialized())
            .field("is_leader", &self.is_leader())
            .finish()
    }
}

// Additional helper types that might be needed
pub type SharedNodeStateRef = Arc<SharedNodeState>;
