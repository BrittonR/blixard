use std::sync::Arc;
use tokio::sync::{RwLock, Mutex, mpsc, oneshot};
use redb::Database;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand, VmConfig, VmStatus};
use crate::vm_manager::VmManager;
use crate::raft_manager::{RaftProposal, TaskSpec, TaskResult};

/// Shared node state that is Send + Sync
/// This can be safely wrapped in Arc and shared across threads
pub struct SharedNodeState {
    pub config: NodeConfig,
    database: RwLock<Option<Arc<Database>>>,
    
    // Wrap non-Sync senders in Mutex for thread-safe access
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    raft_proposal_tx: Mutex<Option<mpsc::UnboundedSender<RaftProposal>>>,
    raft_message_tx: Mutex<Option<mpsc::UnboundedSender<(u64, raft::prelude::Message)>>>,
    
    // VM manager wrapped for thread safety
    vm_manager: RwLock<Option<Arc<VmManagerShared>>>,
    
    // Track running state
    is_running: RwLock<bool>,
}

/// Thread-safe wrapper for VM manager operations
pub struct VmManagerShared {
    inner: VmManager,
    command_tx: Mutex<mpsc::UnboundedSender<VmCommand>>,
}

impl SharedNodeState {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            database: RwLock::new(None),
            shutdown_tx: Mutex::new(None),
            raft_proposal_tx: Mutex::new(None),
            raft_message_tx: Mutex::new(None),
            vm_manager: RwLock::new(None),
            is_running: RwLock::new(false),
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
    
    /// Get database
    pub async fn get_database(&self) -> Option<Arc<Database>> {
        self.database.read().await.clone()
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
        let proposal_tx = self.raft_proposal_tx.lock().await;
        
        let database = self.database.read().await;
        if let (Some(tx), Some(db)) = (proposal_tx.as_ref(), database.as_ref()) {
            // Check if we can schedule the task
            let assigned_node = crate::raft_manager::schedule_task(db.clone(), task_id, &task).await?
                .ok_or_else(|| BlixardError::ClusterError("No suitable worker available".to_string()))?;
            
            // Submit the task assignment through Raft
            let proposal_data = crate::raft_manager::ProposalData::AssignTask {
                task_id: task_id.to_string(),
                node_id: assigned_node,
                task,
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal = RaftProposal {
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            tx.send(proposal).map_err(|_| BlixardError::Internal {
                message: "Failed to send task proposal".to_string(),
            })?;
            
            // Wait for response
            response_rx.await.map_err(|_| BlixardError::Internal {
                message: "Task proposal response channel closed".to_string(),
            })??;
            
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
        use redb::ReadableTable;
        
        let database = self.database.read().await;
        if let Some(db) = database.as_ref() {
            let read_txn = db.begin_read()?;
            
            // Get worker nodes
            let mut node_ids = Vec::new();
            if let Ok(worker_table) = read_txn.open_table(crate::storage::WORKER_TABLE) {
                for entry in worker_table.iter()? {
                    let (node_id_bytes, _) = entry?;
                    let node_id = u64::from_le_bytes(node_id_bytes.value().try_into().unwrap());
                    node_ids.push(node_id);
                }
            }
            
            // TODO: Get leader_id and term from Raft state
            let leader_id = self.config.id; // Placeholder
            let term = 0; // Placeholder
            
            Ok((leader_id, node_ids, term))
        } else {
            Err(BlixardError::Internal {
                message: "Database not initialized".to_string(),
            })
        }
    }
    
    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
    
    /// Set running state
    pub async fn set_running(&self, running: bool) {
        *self.is_running.write().await = running;
    }
}

// Ensure SharedNodeState is Send + Sync
static_assertions::assert_impl_all!(SharedNodeState: Send, Sync);