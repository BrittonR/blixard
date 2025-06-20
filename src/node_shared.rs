use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, Mutex, mpsc, oneshot};
use redb::Database;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand, VmConfig, VmStatus};
use crate::vm_manager::VmManager;
use crate::raft_manager::{RaftProposal, TaskSpec, TaskResult, RaftConfChange, ConfChangeType};

/// # State Management in Blixard
/// 
/// ## Distributed State (Authoritative - MUST go through Raft)
/// 
/// All state that needs to be consistent across the cluster MUST be persisted
/// through Raft consensus. This includes:
/// 
/// - **VM State**: All VM configurations, status, and assignments
/// - **Worker Registrations**: Node capabilities and availability
/// - **Task Assignments**: Which tasks are assigned to which workers
/// - **Cluster Configuration**: Membership changes and node roles
/// 
/// These operations use the Raft proposal mechanism via methods like:
/// - `create_vm_through_raft()`
/// - `register_worker_through_raft()`
/// - `submit_task()`
/// - `propose_conf_change()`
/// 
/// ## Local State (Non-authoritative - managed locally)
/// 
/// Some state is maintained locally for operational purposes and does NOT
/// go through Raft consensus:
/// 
/// - **Peer Addresses**: Used for routing messages between nodes
/// - **Active Connections**: gRPC connection pools and health status
/// - **Runtime Handles**: Tokio task handles and channels
/// - **Raft Status Cache**: Leader ID, term, and role information
/// 
/// This local state is used for:
/// - Message routing and delivery
/// - Connection management
/// - Health monitoring
/// - Performance optimization
/// 
/// ## Important Guidelines
/// 
/// 1. **Never write directly to the database** except during bootstrap
/// 2. **Always use Raft proposals** for any state that must be consistent
/// 3. **Local caches are for routing only**, not authoritative data
/// 4. **When in doubt, use Raft** - consistency is more important than speed

/// Raft status information
#[derive(Debug, Clone)]
pub struct RaftStatus {
    pub is_leader: bool,
    pub node_id: u64,
    pub leader_id: Option<u64>,
    pub term: u64,
    pub state: String,
}

/// Peer information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: u64,
    pub address: String,
    pub is_connected: bool,
}

/// Shared node state that is Send + Sync
/// 
/// This struct contains all the state that needs to be accessed from multiple
/// async tasks and must be Send + Sync. It serves as the central coordination
/// point for all node operations.
/// 
/// ## State Categories
/// 
/// ### Distributed State (via Raft)
/// - VM configurations and status (accessed through database after Raft consensus)
/// - Worker registrations and capabilities (stored in WORKER_TABLE)
/// - Task assignments (stored in TASK_TABLE)
/// 
/// ### Local State
/// - `peers`: HashMap of peer addresses for message routing
/// - `raft_status`: Cached information about Raft state
/// - `is_running` / `is_initialized`: Local node lifecycle state
/// - Various mpsc channels for internal communication
/// 
/// ## Thread Safety
/// All fields use appropriate synchronization primitives (RwLock, Mutex) to
/// ensure thread-safe access from concurrent tasks.
pub struct SharedNodeState {
    pub config: NodeConfig,
    database: RwLock<Option<Arc<Database>>>,
    
    // Wrap non-Sync senders in Mutex for thread-safe access
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    raft_proposal_tx: Mutex<Option<mpsc::UnboundedSender<RaftProposal>>>,
    raft_message_tx: Mutex<Option<mpsc::UnboundedSender<(u64, raft::prelude::Message)>>>,
    raft_conf_change_tx: Mutex<Option<mpsc::UnboundedSender<RaftConfChange>>>,
    
    // VM manager wrapped for thread safety
    vm_manager: RwLock<Option<Arc<VmManagerShared>>>,
    
    // Track running state
    is_running: RwLock<bool>,
    
    // Track initialization state
    is_initialized: RwLock<bool>,
    
    // Raft status
    raft_status: RwLock<RaftStatus>,
    
    // Peer management
    peers: RwLock<HashMap<u64, PeerInfo>>,
    
    // Peer connector for managing connections
    peer_connector: RwLock<Option<Arc<crate::peer_connector::PeerConnector>>>,
}

/// Thread-safe wrapper for VM manager operations
pub struct VmManagerShared {
    inner: VmManager,
    command_tx: Mutex<mpsc::UnboundedSender<VmCommand>>,
}

impl SharedNodeState {
    pub fn new(config: NodeConfig) -> Self {
        let node_id = config.id;
        Self {
            config,
            database: RwLock::new(None),
            shutdown_tx: Mutex::new(None),
            raft_proposal_tx: Mutex::new(None),
            raft_message_tx: Mutex::new(None),
            raft_conf_change_tx: Mutex::new(None),
            vm_manager: RwLock::new(None),
            is_running: RwLock::new(false),
            is_initialized: RwLock::new(false),
            raft_status: RwLock::new(RaftStatus {
                is_leader: false,
                node_id,
                leader_id: None,
                term: 0,
                state: "follower".to_string(),
            }),
            peers: RwLock::new(HashMap::new()),
            peer_connector: RwLock::new(None),
        }
    }
    
    /// Set the shutdown sender
    pub async fn set_shutdown_tx(&self, tx: oneshot::Sender<()>) {
        *self.shutdown_tx.lock().await = Some(tx);
    }
    
    /// Take the shutdown sender (consuming it)
    pub async fn take_shutdown_tx(&self) -> Option<oneshot::Sender<()>> {
        self.shutdown_tx.lock().await.take()
    }
    
    /// Set the raft proposal sender
    pub async fn set_raft_proposal_tx(&self, tx: mpsc::UnboundedSender<RaftProposal>) {
        *self.raft_proposal_tx.lock().await = Some(tx);
    }
    
    /// Set the raft message sender
    pub async fn set_raft_message_tx(&self, tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>) {
        *self.raft_message_tx.lock().await = Some(tx);
    }
    
    /// Set the raft conf change sender
    pub async fn set_raft_conf_change_tx(&self, tx: mpsc::UnboundedSender<RaftConfChange>) {
        *self.raft_conf_change_tx.lock().await = Some(tx);
    }
    
    /// Set the VM manager
    pub async fn set_vm_manager(&self, vm_manager: VmManager, command_tx: mpsc::UnboundedSender<VmCommand>) {
        let shared = Arc::new(VmManagerShared {
            inner: vm_manager,
            command_tx: Mutex::new(command_tx),
        });
        *self.vm_manager.write().await = Some(shared);
    }
    
    /// Get node ID
    pub fn get_id(&self) -> u64 {
        self.config.id
    }
    
    /// Get bind address
    pub fn get_bind_addr(&self) -> &std::net::SocketAddr {
        &self.config.bind_addr
    }
    
    /// Set the database
    pub async fn set_database(&self, db: Arc<Database>) {
        *self.database.write().await = Some(db);
    }
    
    /// Clear the database (for shutdown)
    pub async fn clear_database(&self) {
        *self.database.write().await = None;
    }
    
    /// Shutdown all components that hold database references
    pub async fn shutdown_components(&self) {
        // Clear VM manager to release its database reference
        *self.vm_manager.write().await = None;
        
        // Shutdown peer connector and its background tasks
        if let Some(pc) = self.peer_connector.read().await.as_ref() {
            pc.shutdown().await;
        }
        // Clear peer connector
        *self.peer_connector.write().await = None;
        
        // Clear all channel senders
        *self.raft_proposal_tx.lock().await = None;
        *self.raft_message_tx.lock().await = None;
        *self.raft_conf_change_tx.lock().await = None;
        *self.shutdown_tx.lock().await = None;
        
        // Finally clear the database
        *self.database.write().await = None;
    }
    
    /// Get database
    pub async fn get_database(&self) -> Option<Arc<Database>> {
        self.database.read().await.clone()
    }
    
    /// Set the peer connector
    pub async fn set_peer_connector(&self, connector: Arc<crate::peer_connector::PeerConnector>) {
        *self.peer_connector.write().await = Some(connector);
    }
    
    /// Get the peer connector
    pub async fn get_peer_connector(&self) -> Option<Arc<crate::peer_connector::PeerConnector>> {
        self.peer_connector.read().await.clone()
    }
    
    /// Send a raft proposal
    pub async fn send_raft_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        let tx = self.raft_proposal_tx.lock().await;
        if let Some(sender) = tx.as_ref() {
            sender.send(proposal).map_err(|_| BlixardError::Internal {
                message: "Failed to send Raft proposal".to_string(),
            })
        } else {
            Err(BlixardError::Internal {
                message: "Raft proposal sender not initialized".to_string(),
            })
        }
    }
    
    /// Send a raft message
    pub async fn send_raft_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        let tx = self.raft_message_tx.lock().await;
        if let Some(sender) = tx.as_ref() {
            sender.send((from, msg)).map_err(|_| BlixardError::Internal {
                message: "Failed to send Raft message".to_string(),
            })
        } else {
            Err(BlixardError::Internal {
                message: "Raft message sender not initialized".to_string(),
            })
        }
    }
    
    /// Send a VM command
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        let vm_manager = self.vm_manager.read().await;
        if let Some(manager) = vm_manager.as_ref() {
            let tx = manager.command_tx.lock().await;
            tx.send(command).map_err(|_| BlixardError::Internal {
                message: "Failed to send VM command".to_string(),
            })
        } else {
            Err(BlixardError::Internal {
                message: "VM manager not initialized".to_string(),
            })
        }
    }
    
    /// List all VMs
    pub async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        let vm_manager = self.vm_manager.read().await;
        if let Some(manager) = vm_manager.as_ref() {
            manager.inner.list_vms().await
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Get VM status
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        let vm_manager = self.vm_manager.read().await;
        if let Some(manager) = vm_manager.as_ref() {
            manager.inner.get_vm_status(name).await
        } else {
            Ok(None)
        }
    }
    
    /// Create a VM through Raft consensus
    pub async fn create_vm_through_raft(&self, command: VmCommand) -> BlixardResult<()> {
        tracing::info!("create_vm_through_raft called");
        
        // CRITICAL FIX: Check if this node has been removed from the cluster
        // If a node has no peers, it may have been removed from the cluster
        let peers = self.get_peers().await;
        let raft_status = self.get_raft_status().await?;
        
        // If we have no peers, we might have been cleanly removed from the cluster
        // In this case, we should not process writes (prevents split-brain during testing)
        if peers.is_empty() && !raft_status.is_leader {
            tracing::warn!("Node has no peers and is not leader - rejecting write to prevent split-brain");
            return Err(BlixardError::ClusterError(
                "Node isolated from cluster and cannot process writes".to_string()
            ));
        }
        
        // Check if we're the leader
        if !raft_status.is_leader {
            // Forward to leader if we know who it is
            if let Some(leader_id) = raft_status.leader_id {
                tracing::info!("Forwarding VM creation to leader node {}", leader_id);
                
                // Get leader address
                let peers = self.get_peers().await;
                let leader_addr = peers.iter()
                    .find(|p| p.id == leader_id)
                    .map(|p| p.address.clone())
                    .ok_or_else(|| BlixardError::ClusterError(
                        format!("Leader node {} address not found", leader_id)
                    ))?;
                
                // Forward the request to the leader
                use crate::proto::cluster_service_client::ClusterServiceClient;
                use crate::proto::CreateVmRequest;
                
                let mut client = ClusterServiceClient::connect(format!("http://{}", leader_addr))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to leader: {}", e),
                    })?;
                
                // Extract VM config from command
                let (vm_config, _node_id) = match &command {
                    VmCommand::Create { config, node_id } => (config, node_id),
                    _ => return Err(BlixardError::Internal {
                        message: "Expected Create command".to_string(),
                    }),
                };
                
                let request = CreateVmRequest {
                    name: vm_config.name.clone(),
                    config_path: vm_config.config_path.clone(),
                    vcpus: vm_config.vcpus,
                    memory_mb: vm_config.memory,
                };
                
                let response = client.create_vm(request)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to forward VM creation to leader: {}", e),
                    })?;
                
                let response = response.into_inner();
                if response.success {
                    Ok(())
                } else {
                    Err(BlixardError::ClusterError(response.message))
                }
            } else {
                Err(BlixardError::ClusterError("No leader elected yet".to_string()))
            }
        } else {
            // We are the leader, but verify we can reach a quorum before processing writes
            tracing::info!("Processing VM creation locally as leader");
            
            // CRITICAL FIX: For network partition safety, verify leadership is still valid
            // by ensuring we have recent communication with a majority of the configured voters
            let raft_status = self.get_raft_status().await?;
            
            // Additional safety check: if we haven't heard from followers recently,
            // we might be in a network partition and should reject writes
            let peers = self.get_peers().await;
            if !peers.is_empty() {
                // In a multi-node cluster, we should have peers
                // Check if we can reach a majority (simple peer connectivity check)
                let total_configured_nodes = peers.len() + 1; // +1 for self
                let required_majority = (total_configured_nodes / 2) + 1;
                
                // Count reachable nodes (start with self)
                let mut reachable_count = 1;
                for peer in &peers {
                    if peer.is_connected {
                        reachable_count += 1;
                    }
                }
                
                if reachable_count < required_majority {
                    tracing::warn!(
                        "Leadership safety check failed: only {}/{} nodes reachable, need {} for majority", 
                        reachable_count, total_configured_nodes, required_majority
                    );
                    return Err(BlixardError::ClusterError(
                        format!("Cannot process writes: leader isolation detected - only {}/{} nodes reachable", 
                               reachable_count, total_configured_nodes)
                    ));
                }
                
                tracing::info!("Leadership safety check passed: {}/{} nodes reachable", reachable_count, total_configured_nodes);
            }
            
            let proposal_tx = self.raft_proposal_tx.lock().await;
            if let Some(tx) = proposal_tx.as_ref() {
                // Create the proposal
                let proposal_data = crate::raft_manager::ProposalData::CreateVm(command);
                
                let (response_tx, response_rx) = oneshot::channel();
                let proposal_id = uuid::Uuid::new_v4().as_bytes().to_vec();
                tracing::info!("Creating VM proposal with id length: {}", proposal_id.len());
                
                let proposal = RaftProposal {
                    id: proposal_id.clone(),
                    data: proposal_data,
                    response_tx: Some(response_tx),
                };
                
                tracing::info!("Sending VM creation proposal to Raft manager");
                match tx.send(proposal) {
                    Ok(_) => {
                        tracing::info!("Successfully sent VM proposal to Raft channel");
                    }
                    Err(e) => {
                        tracing::error!("Failed to send VM proposal: channel closed or full. Error: {:?}", e);
                        return Err(BlixardError::Internal {
                            message: "Failed to send VM proposal - Raft channel closed".to_string(),
                        });
                    }
                }
                
                tracing::info!("Waiting for VM proposal response...");
                // Wait for response with a timeout
                match tokio::time::timeout(tokio::time::Duration::from_secs(10), response_rx).await {
                    Ok(Ok(result)) => {
                        tracing::info!("Received VM proposal response: {:?}", result.is_ok());
                        result?;
                        Ok(())
                    }
                    Ok(Err(_)) => {
                        tracing::error!("VM proposal response channel closed");
                        Err(BlixardError::Internal {
                            message: "VM proposal response channel closed".to_string(),
                        })
                    }
                    Err(_) => {
                        tracing::error!("VM proposal timed out after 10 seconds");
                        Err(BlixardError::Internal {
                            message: "VM proposal timed out".to_string(),
                        })
                    }
                }
            } else {
                Err(BlixardError::Internal {
                    message: "Raft proposal sender not initialized".to_string(),
                })
            }
        }
    }
    
    /// Send VM operation through Raft consensus
    pub async fn send_vm_operation_through_raft(&self, command: VmCommand) -> BlixardResult<()> {
        tracing::info!("send_vm_operation_through_raft called");
        
        // Check if we're the leader
        let raft_status = self.get_raft_status().await?;
        if !raft_status.is_leader {
            // Forward to leader if we know who it is
            if let Some(leader_id) = raft_status.leader_id {
                tracing::info!("Forwarding VM operation to leader node {}", leader_id);
                
                // Get leader address
                let peers = self.get_peers().await;
                let leader_addr = peers.iter()
                    .find(|p| p.id == leader_id)
                    .map(|p| p.address.clone())
                    .ok_or_else(|| BlixardError::ClusterError(
                        format!("Leader node {} address not found", leader_id)
                    ))?;
                
                // Forward the request to the leader based on command type
                use crate::proto::cluster_service_client::ClusterServiceClient;
                use crate::proto::{StartVmRequest, StopVmRequest};
                
                let mut client = ClusterServiceClient::connect(format!("http://{}", leader_addr))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to leader: {}", e),
                    })?;
                
                match &command {
                    VmCommand::Start { name } => {
                        let request = StartVmRequest {
                            name: name.clone(),
                        };
                        
                        let response = client.start_vm(request)
                            .await
                            .map_err(|e| BlixardError::Internal {
                                message: format!("Failed to forward VM start to leader: {}", e),
                            })?;
                        
                        let response = response.into_inner();
                        if response.success {
                            Ok(())
                        } else {
                            Err(BlixardError::ClusterError(response.message))
                        }
                    }
                    VmCommand::Stop { name } => {
                        let request = StopVmRequest {
                            name: name.clone(),
                        };
                        
                        let response = client.stop_vm(request)
                            .await
                            .map_err(|e| BlixardError::Internal {
                                message: format!("Failed to forward VM stop to leader: {}", e),
                            })?;
                        
                        let response = response.into_inner();
                        if response.success {
                            Ok(())
                        } else {
                            Err(BlixardError::ClusterError(response.message))
                        }
                    }
                    _ => Err(BlixardError::Internal {
                        message: "Unsupported VM command for forwarding".to_string(),
                    })
                }
            } else {
                Err(BlixardError::ClusterError("No leader elected yet".to_string()))
            }
        } else {
            // We are the leader, process through Raft
            tracing::info!("Processing VM operation locally as leader");
            
            // For now, use UpdateVmStatus proposal for start/stop operations
            let (vm_name, new_status) = match &command {
                VmCommand::Start { name } => (name.clone(), VmStatus::Starting),
                VmCommand::Stop { name } => (name.clone(), VmStatus::Stopping),
                VmCommand::Delete { name } => (name.clone(), VmStatus::Stopped), // Delete after stopping
                _ => return Err(BlixardError::Internal {
                    message: "Unsupported VM command".to_string(),
                }),
            };
            
            let proposal_data = crate::raft_manager::ProposalData::UpdateVmStatus {
                vm_name,
                status: new_status,
                node_id: self.get_id(),
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal_id = uuid::Uuid::new_v4().as_bytes().to_vec();
            
            let proposal = RaftProposal {
                id: proposal_id,
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            // Send proposal
            match self.send_raft_proposal(proposal).await {
                Ok(_) => {
                    tracing::info!("Successfully sent VM operation proposal");
                }
                Err(e) => {
                    tracing::error!("Failed to send VM operation proposal: {}", e);
                    return Err(e);
                }
            }
            
            // Wait for response
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), response_rx).await {
                Ok(Ok(result)) => {
                    result?;
                    Ok(())
                }
                Ok(Err(_)) => {
                    Err(BlixardError::Internal {
                        message: "VM operation response channel closed".to_string(),
                    })
                }
                Err(_) => {
                    Err(BlixardError::Internal {
                        message: "VM operation timed out".to_string(),
                    })
                }
            }
        }
    }
    
    
    /// Submit a task to the cluster
    pub async fn submit_task(&self, task_id: &str, task: TaskSpec) -> BlixardResult<u64> {
        tracing::info!("submit_task called for task_id: {}", task_id);
        
        // Check if we're the leader
        let raft_status = self.get_raft_status().await?;
        if !raft_status.is_leader {
            // Forward to leader if we know who it is
            if let Some(leader_id) = raft_status.leader_id {
                tracing::info!("Forwarding task submission to leader node {}", leader_id);
                
                // Get leader address
                let peers = self.get_peers().await;
                let leader_addr = peers.iter()
                    .find(|p| p.id == leader_id)
                    .map(|p| p.address.clone())
                    .ok_or_else(|| BlixardError::ClusterError(
                        format!("Leader node {} address not found", leader_id)
                    ))?;
                
                // Forward the request to the leader
                use crate::proto::cluster_service_client::ClusterServiceClient;
                use crate::proto::TaskRequest;
                
                let mut client = ClusterServiceClient::connect(format!("http://{}", leader_addr))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to leader: {}", e),
                    })?;
                
                let request = TaskRequest {
                    task_id: task_id.to_string(),
                    command: task.command,
                    args: task.args,
                    cpu_cores: task.resources.cpu_cores as u32,
                    memory_mb: task.resources.memory_mb,
                    disk_gb: task.resources.disk_gb,
                    required_features: task.resources.required_features,
                    timeout_secs: task.timeout_secs,
                };
                
                let response = client.submit_task(request)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to forward task to leader: {}", e),
                    })?;
                
                let response = response.into_inner();
                if response.accepted {
                    Ok(response.assigned_node)
                } else {
                    Err(BlixardError::ClusterError(response.message))
                }
            } else {
                Err(BlixardError::ClusterError("No leader elected yet".to_string()))
            }
        } else {
            // We are the leader, process locally
            tracing::info!("Processing task submission locally as leader");
            
            let proposal_tx = self.raft_proposal_tx.lock().await;
            
            let database = self.database.read().await;
            if let (Some(tx), Some(db)) = (proposal_tx.as_ref(), database.as_ref()) {
                tracing::info!("Checking if we can schedule the task...");
                // Check if we can schedule the task
                let assigned_node = crate::raft_manager::schedule_task(db.clone(), task_id, &task).await?
                .ok_or_else(|| {
                    tracing::error!("No suitable worker available for task {}", task_id);
                    BlixardError::ClusterError("No suitable worker available".to_string())
                })?;
            
            tracing::info!("Task {} assigned to worker node {}", task_id, assigned_node);
            
            // Submit the task assignment through Raft
            let proposal_data = crate::raft_manager::ProposalData::AssignTask {
                task_id: task_id.to_string(),
                node_id: assigned_node,
                task,
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal_id = uuid::Uuid::new_v4().as_bytes().to_vec();
            tracing::info!("Creating task proposal with id length: {}", proposal_id.len());
            
            let proposal = RaftProposal {
                id: proposal_id.clone(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            tracing::info!("Sending proposal to Raft manager");
            match tx.send(proposal) {
                Ok(_) => {
                    tracing::info!("Successfully sent proposal to Raft channel");
                }
                Err(e) => {
                    tracing::error!("Failed to send proposal: channel closed or full. Error: {:?}", e);
                    return Err(BlixardError::Internal {
                        message: "Failed to send task proposal - Raft channel closed".to_string(),
                    });
                }
            }
            
            tracing::info!("Waiting for proposal response...");
            // Wait for response with a timeout
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), response_rx).await {
                Ok(Ok(result)) => {
                    tracing::info!("Received proposal response: {:?}", result.is_ok());
                    result?;
                }
                Ok(Err(_)) => {
                    tracing::error!("Proposal response channel closed without response");
                    return Err(BlixardError::Internal {
                        message: "Task proposal response channel closed".to_string(),
                    });
                }
                Err(_) => {
                    tracing::error!("Proposal timed out after 10 seconds");
                    return Err(BlixardError::Internal {
                        message: "Task proposal timed out".to_string(),
                    });
                }
            }
            
                Ok(assigned_node)
            } else {
                Err(BlixardError::Internal {
                    message: "Raft manager or database not initialized".to_string(),
                })
            }
        }
    }
    
    /// Get task status
    pub async fn get_task_status(&self, task_id: &str) -> BlixardResult<Option<(String, Option<TaskResult>)>> {
        let database = self.database.read().await;
        if let Some(db) = database.as_ref() {
            let read_txn = db.begin_read()?;
            
            // Check if task is assigned
            if let Ok(assignment_table) = read_txn.open_table(crate::storage::TASK_ASSIGNMENT_TABLE) {
                if let Ok(Some(_)) = assignment_table.get(task_id) {
                    // Task is assigned, check if it has results
                    if let Ok(result_table) = read_txn.open_table(crate::storage::TASK_RESULT_TABLE) {
                        if let Ok(Some(result_data)) = result_table.get(task_id) {
                            let result: TaskResult = bincode::deserialize(result_data.value())?;
                            return Ok(Some(("completed".to_string(), Some(result))));
                        } else {
                            return Ok(Some(("running".to_string(), None)));
                        }
                    }
                }
            }
            
            // Check if task exists
            if let Ok(task_table) = read_txn.open_table(crate::storage::TASK_TABLE) {
                if let Ok(Some(_)) = task_table.get(task_id) {
                    return Ok(Some(("pending".to_string(), None)));
                }
            }
            
            Ok(None)
        } else {
            Err(BlixardError::Internal {
                message: "Database not initialized".to_string(),
            })
        }
    }
    
    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        // Check if initialized
        if !self.is_initialized().await {
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }
        
        // Get the actual Raft configuration state from storage
        // This is the authoritative source of cluster membership
        let node_ids = self.get_current_voters().await?;
        
        // Get Raft status for leader and term info
        let raft_status = self.raft_status.read().await;
        let leader_id = raft_status.leader_id.unwrap_or(0);
        let term = raft_status.term;
        
        Ok((leader_id, node_ids, term))
    }
    
    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
    
    /// Set running state
    pub async fn set_running(&self, running: bool) {
        *self.is_running.write().await = running;
    }
    
    /// Check if node is initialized
    pub async fn is_initialized(&self) -> bool {
        *self.is_initialized.read().await
    }
    
    /// Set initialized state
    pub async fn set_initialized(&self, initialized: bool) {
        *self.is_initialized.write().await = initialized;
    }
    
    /// Get Raft status
    pub async fn get_raft_status(&self) -> BlixardResult<RaftStatus> {
        Ok(self.raft_status.read().await.clone())
    }
    
    /// Update Raft status
    pub async fn update_raft_status(&self, status: RaftStatus) {
        *self.raft_status.write().await = status;
    }
    
    /// Add a peer
    pub async fn add_peer(&self, id: u64, address: String) -> BlixardResult<()> {
        let mut peers = self.peers.write().await;
        if peers.contains_key(&id) {
            return Err(BlixardError::ClusterError(format!("Peer {} already exists", id)));
        }
        peers.insert(id, PeerInfo {
            id,
            address,
            is_connected: false,
        });
        Ok(())
    }
    
    /// Remove a peer
    pub async fn remove_peer(&self, id: u64) -> BlixardResult<()> {
        let mut peers = self.peers.write().await;
        if !peers.contains_key(&id) {
            return Err(BlixardError::ClusterError(format!("Peer {} not found", id)));
        }
        peers.remove(&id);
        Ok(())
    }
    
    /// Get all peers
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
    
    /// Get a specific peer
    pub async fn get_peer(&self, id: u64) -> Option<PeerInfo> {
        let peers = self.peers.read().await;
        peers.get(&id).cloned()
    }
    
    /// Update peer connection status
    pub async fn update_peer_connection(&self, id: u64, is_connected: bool) -> BlixardResult<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(&id) {
            peer.is_connected = is_connected;
            Ok(())
        } else {
            Err(BlixardError::ClusterError(format!("Peer {} not found", id)))
        }
    }
    
    /// Check if this node is the Raft leader
    pub async fn is_leader(&self) -> bool {
        self.raft_status.read().await.is_leader
    }
    
    /// Propose a configuration change through Raft
    pub async fn propose_conf_change(&self, change_type: ConfChangeType, node_id: u64, address: String) -> BlixardResult<()> {
        tracing::info!("[NODE-SHARED] Proposing conf change: {:?} for node {} at {}", change_type, node_id, address);
        
        // Check if we're initialized
        if !self.is_initialized().await {
            tracing::error!("[NODE-SHARED] Cannot propose conf change - node not initialized");
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }
        
        // Check if we're the leader (only leaders can propose conf changes)
        let is_leader = self.is_leader().await;
        if !is_leader {
            let raft_status = self.raft_status.read().await;
            let leader_id = raft_status.leader_id;
            tracing::error!("[NODE-SHARED] Cannot propose conf change - not the leader. Current leader: {:?}", leader_id);
            return Err(BlixardError::ClusterError(
                format!("Only the leader can propose configuration changes. Current leader: {:?}", leader_id)
            ));
        }
        
        let tx = self.raft_conf_change_tx.lock().await;
        if let Some(sender) = tx.as_ref() {
            let (response_tx, response_rx) = oneshot::channel();
            let conf_change = RaftConfChange {
                change_type,
                node_id,
                address: address.clone(),
                response_tx: Some(response_tx),
            };
            
            tracing::info!("[NODE-SHARED] Sending conf change to Raft manager channel");
            match sender.send(conf_change) {
                Ok(_) => {
                    tracing::info!("[NODE-SHARED] Conf change sent successfully, waiting for response");
                }
                Err(e) => {
                    tracing::error!("[NODE-SHARED] Failed to send conf change to channel: {}", e);
                    return Err(BlixardError::Internal {
                        message: format!("Failed to send configuration change: {}", e),
                    });
                }
            }
            
            tracing::info!("[NODE-SHARED] Waiting for conf change response from Raft (timeout: 5s)");
            // Wait for the configuration change to be committed
            // Use a timeout to prevent indefinite blocking
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), response_rx).await {
                Ok(Ok(result)) => {
                    match &result {
                        Ok(_) => {
                            tracing::info!("[NODE-SHARED] Conf change completed successfully");
                        }
                        Err(e) => {
                            tracing::error!("[NODE-SHARED] Conf change failed: {}", e);
                        }
                    }
                    result
                }
                Ok(Err(e)) => {
                    tracing::error!("[NODE-SHARED] Conf change response channel closed: {}", e);
                    Err(BlixardError::Internal {
                        message: format!("Configuration change response channel closed: {}", e),
                    })
                }
                Err(_) => {
                    tracing::error!("[NODE-SHARED] Conf change timed out after 5 seconds");
                    // Get current Raft status for debugging
                    let raft_status = self.raft_status.read().await;
                    tracing::error!("[NODE-SHARED] Current Raft status: is_leader={}, node_id={}, leader_id={:?}, term={}, state={}", 
                        raft_status.is_leader, raft_status.node_id, raft_status.leader_id, raft_status.term, raft_status.state);
                    
                    Err(BlixardError::Internal {
                        message: "Configuration change timed out".to_string(),
                    })
                }
            }
        } else {
            tracing::error!("[NODE-SHARED] Raft manager not initialized - no conf change channel");
            Err(BlixardError::Internal {
                message: "Raft manager not initialized".to_string(),
            })
        }
    }
    
    /// Get the current voters from the Raft configuration
    pub async fn get_current_voters(&self) -> BlixardResult<Vec<u64>> {
        let database = self.database.read().await;
        if let Some(db) = database.as_ref() {
            // Load configuration state from storage
            let storage = crate::storage::RedbRaftStorage { database: db.clone() };
            let conf_state = storage.load_conf_state()?;
            Ok(conf_state.voters)
        } else {
            Err(BlixardError::Internal {
                message: "Database not initialized".to_string(),
            })
        }
    }
    
    /// Register a worker through Raft consensus
    /// This ensures worker information is replicated across all nodes
    pub async fn register_worker_through_raft(
        &self,
        node_id: u64,
        address: String,
        capabilities: crate::raft_manager::WorkerCapabilities,
    ) -> BlixardResult<()> {
        use crate::raft_manager::{ProposalData, RaftProposal};
        use tokio::sync::oneshot;
        
        tracing::info!("[NODE-SHARED] Registering worker {} through Raft", node_id);
        
        // Create the proposal
        let proposal_data = ProposalData::RegisterWorker {
            node_id,
            address,
            capabilities,
        };
        
        let (response_tx, response_rx) = oneshot::channel();
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: proposal_data,
            response_tx: Some(response_tx),
        };
        
        // Submit the proposal
        let tx = self.raft_proposal_tx.lock().await;
        if let Some(sender) = tx.as_ref() {
            match sender.send(proposal) {
                Ok(_) => {
                    tracing::info!("[NODE-SHARED] Worker registration proposal sent");
                }
                Err(e) => {
                    tracing::error!("[NODE-SHARED] Failed to send worker registration proposal: {}", e);
                    return Err(BlixardError::Internal {
                        message: format!("Failed to send worker registration proposal: {}", e),
                    });
                }
            }
            
            // Wait for the proposal to be committed
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), response_rx).await {
                Ok(Ok(result)) => {
                    match result {
                        Ok(_) => {
                            tracing::info!("[NODE-SHARED] Worker {} registered successfully", node_id);
                            Ok(())
                        }
                        Err(e) => {
                            tracing::error!("[NODE-SHARED] Worker registration failed: {}", e);
                            Err(e)
                        }
                    }
                }
                Ok(Err(_)) => {
                    tracing::error!("[NODE-SHARED] Worker registration response channel closed");
                    Err(BlixardError::Internal {
                        message: "Worker registration response channel closed".to_string(),
                    })
                }
                Err(_) => {
                    tracing::error!("[NODE-SHARED] Worker registration timed out after 10 seconds");
                    Err(BlixardError::Internal {
                        message: "Worker registration timed out".to_string(),
                    })
                }
            }
        } else {
            Err(BlixardError::Internal {
                message: "Raft proposal sender not initialized".to_string(),
            })
        }
    }
}

// Ensure SharedNodeState is Send + Sync
static_assertions::assert_impl_all!(SharedNodeState: Send, Sync);