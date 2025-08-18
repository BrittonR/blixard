use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::raft::messages::RaftProposal;
use crate::raft_manager::{RaftConfChange, RaftManager};
use crate::types::{NodeConfig, VmCommand};
use crate::vm_backend::VmBackendRegistry;
use crate::vm_health_monitor::VmHealthMonitor;

use redb::Database;

// Type aliases for complex channel types
type RaftMessageChannel = (mpsc::UnboundedSender<(u64, raft::prelude::Message)>, mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>);
type ConfChangeChannel = (mpsc::UnboundedSender<RaftConfChange>, mpsc::UnboundedReceiver<RaftConfChange>);

/// A Blixard cluster node
pub struct Node {
    /// Shared state that is Send + Sync
    pub(super) shared: Arc<SharedNodeState>,
    /// Non-sync runtime handles
    pub(super) handle: Option<JoinHandle<BlixardResult<()>>>,
    pub(super) raft_handle: Option<JoinHandle<BlixardResult<()>>>,
    /// Iroh service runner handle
    pub(super) iroh_service_handle: Option<JoinHandle<()>>,
    /// VM health monitor
    pub(super) health_monitor: Option<VmHealthMonitor>,
    /// Raft transport adapter
    pub(super) raft_transport: Option<Arc<crate::transport::raft_transport_adapter::RaftTransport>>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            shared: Arc::new(SharedNodeState::new(config)),
            handle: None,
            raft_handle: None,
            iroh_service_handle: None,
            health_monitor: None,
            raft_transport: None,
        }
    }

    /// Get a shared reference to the node state
    /// This is what should be passed to the gRPC server
    pub fn shared(&self) -> Arc<SharedNodeState> {
        Arc::clone(&self.shared)
    }

    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        let vm_name = match &command {
            VmCommand::Create { config, .. } => &config.name,
            VmCommand::Start { name } => name,
            VmCommand::Stop { name } => name,
            VmCommand::Delete { name } => name,
            VmCommand::UpdateStatus { name, .. } => name,
            VmCommand::Migrate { task } => &task.vm_name,
        };
        let command_str = format!("{:?}", command);
        self.shared
            .send_vm_command(vm_name, command_str)
            .await
            .map(|_| ())
    }

    /// Get the node ID
    pub fn get_id(&self) -> u64 {
        self.shared.get_id()
    }

    /// Get the bind address
    pub fn get_bind_addr(&self) -> String {
        self.shared.get_bind_addr()
    }

    /// List all VMs and their status
    pub async fn list_vms(
        &self,
    ) -> BlixardResult<Vec<(crate::types::VmConfig, crate::types::VmStatus)>> {
        let vm_states = self.shared.list_vms().await?;
        Ok(vm_states
            .into_iter()
            .map(|state| (state.config, state.status))
            .collect())
    }

    /// Get status of a specific VM
    pub async fn get_vm_status(
        &self,
        name: &str,
    ) -> BlixardResult<Option<(crate::types::VmConfig, crate::types::VmStatus)>> {
        match self.shared.get_vm_info(name).await {
            Ok(vm_state) => Ok(Some((vm_state.config, vm_state.status))),
            Err(BlixardError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get the IP address of a VM
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        match self.shared.get_vm_ip(name).await {
            Ok(ip) => Ok(Some(ip)),
            Err(BlixardError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        self.shared.is_running()
    }

    /// Check if this node is the Raft leader
    pub async fn is_leader(&self) -> bool {
        self.shared.is_leader()
    }

    /// Send a Raft message to the Raft manager
    pub async fn send_raft_message(
        &self,
        from: u64,
        msg: raft::prelude::Message,
    ) -> BlixardResult<()> {
        self.shared.send_raft_message(from, msg).await
    }

    /// Submit a task to the cluster
    pub async fn submit_task(
        &self,
        task_id: &str,
        task: crate::raft_manager::TaskSpec,
    ) -> BlixardResult<u64> {
        self.shared.submit_task(task_id, task).await
    }

    /// Get task status
    pub async fn get_task_status(
        &self,
        task_id: &str,
    ) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        self.shared.get_task_status(task_id).await
    }

    /// Submit VM migration
    pub async fn submit_vm_migration(
        &self,
        task: crate::types::VmMigrationTask,
    ) -> BlixardResult<()> {
        use crate::raft_manager::ProposalData;

        let proposal_data = ProposalData::MigrateVm {
            vm_name: task.vm_name.clone(),
            from_node: task.source_node_id,
            to_node: task.target_node_id,
        };

        let (response_tx, response_rx) = oneshot::channel();
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: proposal_data,
            response_tx: Some(response_tx),
        };

        self.shared.send_raft_proposal(proposal).await?;

        // Wait for response
        response_rx.await.map_err(|_| BlixardError::Internal {
            message: "Migration proposal response channel closed".to_string(),
        })??;

        Ok(())
    }

    /// Estimate available memory in MB
    /// Returns a conservative default if detection fails
    pub(super) fn estimate_available_memory() -> u64 {
        // Try to read from /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
                for line in contents.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb / 1024; // Convert KB to MB
                            }
                        }
                    }
                }
            }
        }

        // Default from configuration if detection fails
        crate::config_global::get()
            .map(|cfg| cfg.cluster.worker.default_memory_mb)
            .unwrap_or(4096) // Reasonable default of 4GB
    }

    /// Estimate available disk space in GB
    /// Returns a conservative default if detection fails
    pub(super) fn estimate_available_disk(data_dir: &str) -> u64 {
        // Try to get filesystem stats
        if let Ok(_metadata) = std::fs::metadata(data_dir) {
            // On Unix systems, we could use statvfs, but for now use a default
            // In production, use a proper system info crate
            return 100; // Default to 100GB
        }

        // Create directory if it doesn't exist and return default
        let _ = std::fs::create_dir_all(data_dir);
        crate::config_global::get()
            .map(|cfg| cfg.cluster.worker.default_disk_gb)
            .unwrap_or(100) // Reasonable default of 100GB
    }

    /// Create discovery configuration from node config
    #[allow(dead_code)] // Reserved for future dynamic discovery configuration
    pub(super) fn create_discovery_config(
        config: &NodeConfig,
    ) -> BlixardResult<crate::discovery::DiscoveryConfig> {
        let mut discovery_config = crate::discovery::DiscoveryConfig::default();

        // If we have a join address, add it as a static node
        if let Some(join_addr) = &config.join_addr {
            // Try to parse the join address to get the node ID
            // For now, we'll use "bootstrap" as the key
            discovery_config
                .static_nodes
                .insert("bootstrap".to_string(), vec![join_addr.clone()]);
        }

        // Check environment for additional static nodes
        if let Ok(static_nodes) = std::env::var("BLIXARD_STATIC_NODES") {
            // Format: node1=addr1,node2=addr2
            for entry in static_nodes.split(',') {
                if let Some((node_id, addr)) = entry.split_once('=') {
                    discovery_config
                        .static_nodes
                        .insert(node_id.trim().to_string(), vec![addr.trim().to_string()]);
                }
            }
        }

        // Enable/disable discovery methods based on environment
        if let Ok(enable_dns) = std::env::var("BLIXARD_ENABLE_DNS_DISCOVERY") {
            discovery_config.enable_dns = enable_dns.to_lowercase() == "true";
        }

        if let Ok(enable_mdns) = std::env::var("BLIXARD_ENABLE_MDNS_DISCOVERY") {
            discovery_config.enable_mdns = enable_mdns.to_lowercase() == "true";
        }

        Ok(discovery_config)
    }

    /// Create a VM backend using the factory pattern
    pub(super) fn create_vm_backend(
        &self,
        registry: &VmBackendRegistry,
        database: Arc<Database>,
    ) -> BlixardResult<Arc<dyn crate::vm_backend::VmBackend>> {
        use std::path::PathBuf;

        let data_dir = &self.shared.config.data_dir;
        let vm_config_dir = std::fs::canonicalize(PathBuf::from(data_dir).join("vm-configs"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|cwd| cwd.join(data_dir).join("vm-configs"))
                    .unwrap_or_else(|_| PathBuf::from(data_dir).join("vm-configs"))
            });
        let vm_data_dir = std::fs::canonicalize(PathBuf::from(data_dir).join("vm-data"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|cwd| cwd.join(data_dir).join("vm-data"))
                    .unwrap_or_else(|_| PathBuf::from(data_dir).join("vm-data"))
            });

        // Create directories
        std::fs::create_dir_all(&vm_config_dir).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create VM config directory: {}", e),
        })?;
        std::fs::create_dir_all(&vm_data_dir).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create VM data directory: {}", e),
        })?;

        let backend_type = &self.shared.config.vm_backend;
        tracing::info!("Creating VM backend of type: {}", backend_type);

        registry.create_backend(backend_type, vm_config_dir, vm_data_dir, database)
    }

    /// Recreate Raft manager for recovery
    pub(super) async fn recreate_raft_manager(
        node_id: u64,
        db: Option<Arc<Database>>,
        shared: Arc<SharedNodeState>,
    ) -> BlixardResult<RaftManager> {
        let db = db.ok_or_else(|| BlixardError::Internal {
            message: "Database not available for Raft recovery".to_string(),
        })?;

        // Create new channels
        let (proposal_tx, _proposal_rx) = mpsc::unbounded_channel();
        let (message_tx, _message_rx) = mpsc::unbounded_channel();
        let (_conf_change_tx, _conf_change_rx): ConfChangeChannel = mpsc::unbounded_channel();
        let (_outgoing_tx, _outgoing_rx): RaftMessageChannel = mpsc::unbounded_channel();

        // Update shared state with new channels
        // Set individual Raft channels
        shared.set_raft_proposal_tx(proposal_tx.clone());
        shared.set_raft_message_tx(message_tx.clone());

        // Create new Raft manager with correct signature
        let peers = shared.get_peers().await;
        let peer_list: Vec<(u64, String)> = peers
            .into_iter()
            .filter_map(|p| {
                // Parse node_id string to u64 - skip invalid peers
                match p.node_id.parse::<u64>() {
                    Ok(id) => Some((id, p.address)),
                    Err(e) => {
                        tracing::warn!(
                            "Skipping peer with invalid node ID '{}': {}",
                            p.node_id, e
                        );
                        None
                    }
                }
            })
            .collect();
        let shared_weak = Arc::downgrade(&shared);
        let (raft_manager, _proposal_tx, _message_tx, _conf_change_tx, _outgoing_tx) =
            RaftManager::new(node_id, db, peer_list, shared_weak)?;

        // Message handling is managed internally by RaftManager

        Ok(raft_manager)
    }
}