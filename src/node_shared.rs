use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, Mutex, mpsc, oneshot};
use redb::Database;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand, VmConfig, VmStatus};
use crate::vm_manager::VmManager;
use crate::raft_manager::{RaftProposal, TaskSpec, TaskResult, RaftConfChange, ConfChangeType};

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
/// This can be safely wrapped in Arc and shared across threads
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
    
    
    /// Submit a task to the cluster
    pub async fn submit_task(&self, task_id: &str, task: TaskSpec) -> BlixardResult<u64> {
        tracing::info!("submit_task called for task_id: {}", task_id);
        
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
            tx.send(proposal).map_err(|_| BlixardError::Internal {
                message: "Failed to send task proposal".to_string(),
            })?;
            
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
        
        // Get Raft peers (nodes in the cluster)
        let peers = self.peers.read().await;
        let mut node_ids: Vec<u64> = peers.keys().cloned().collect();
        
        // Include self in the node list
        if !node_ids.contains(&self.config.id) {
            node_ids.push(self.config.id);
        }
        node_ids.sort();
        
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
        let tx = self.raft_conf_change_tx.lock().await;
        if let Some(sender) = tx.as_ref() {
            let (response_tx, response_rx) = oneshot::channel();
            let conf_change = RaftConfChange {
                change_type,
                node_id,
                address,
                response_tx: Some(response_tx),
            };
            
            sender.send(conf_change)
                .map_err(|_| BlixardError::Internal {
                    message: "Failed to send configuration change".to_string(),
                })?;
            
            tracing::info!("[NODE-SHARED] Waiting for conf change response from Raft");
            // Wait for the configuration change to be committed
            // Use a timeout to prevent indefinite blocking
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), response_rx).await {
                Ok(Ok(result)) => {
                    tracing::info!("[NODE-SHARED] Conf change completed successfully");
                    result
                }
                Ok(Err(_)) => {
                    tracing::warn!("[NODE-SHARED] Conf change response channel closed");
                    Err(BlixardError::Internal {
                        message: "Configuration change response channel closed".to_string(),
                    })
                }
                Err(_) => {
                    tracing::warn!("[NODE-SHARED] Conf change timed out after 5 seconds");
                    Err(BlixardError::Internal {
                        message: "Configuration change timed out".to_string(),
                    })
                }
            }
        } else {
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
}

// Ensure SharedNodeState is Send + Sync
static_assertions::assert_impl_all!(SharedNodeState: Send, Sync);