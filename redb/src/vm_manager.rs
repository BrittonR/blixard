use tokio::sync::{mpsc, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use redb::Database;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{VmState, VmCommand, VmConfig, VmStatus};
use crate::storage::VM_STATE_TABLE;

/// Manages VM state and lifecycle operations
pub struct VmManager {
    vm_states: Arc<RwLock<HashMap<String, VmState>>>,
    database: Arc<Database>,
    command_tx: mpsc::UnboundedSender<VmCommand>,
}

impl VmManager {
    /// Create a new VM manager
    pub fn new(database: Arc<Database>) -> (Self, mpsc::UnboundedReceiver<VmCommand>) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let vm_states = Arc::new(RwLock::new(HashMap::new()));
        
        let manager = Self {
            vm_states,
            database,
            command_tx,
        };
        
        (manager, command_rx)
    }
    
    /// Start the VM command processor
    pub fn start_processor(&self, mut command_rx: mpsc::UnboundedReceiver<VmCommand>) {
        let vm_states = Arc::clone(&self.vm_states);
        let database = Arc::clone(&self.database);
        
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if let Err(e) = Self::process_command(command, &vm_states, &database).await {
                    tracing::error!("Error processing VM command: {}", e);
                }
            }
        });
    }
    
    /// Send a VM command for processing
    pub async fn send_command(&self, command: VmCommand) -> BlixardResult<()> {
        self.command_tx.send(command).map_err(|_| BlixardError::Internal {
            message: "Failed to send VM command".to_string(),
        })
    }
    
    /// List all VMs and their status
    pub async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        let states = self.vm_states.read().await;
        let mut result = Vec::new();
        
        for vm_state in states.values() {
            result.push((vm_state.config.clone(), vm_state.status));
        }
        
        Ok(result)
    }
    
    /// Get status of a specific VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        let states = self.vm_states.read().await;
        
        if let Some(vm_state) = states.get(name) {
            Ok(Some((vm_state.config.clone(), vm_state.status)))
        } else {
            Ok(None)
        }
    }
    
    async fn process_command(
        command: VmCommand,
        vm_states: &Arc<RwLock<HashMap<String, VmState>>>,
        database: &Arc<Database>,
    ) -> BlixardResult<()> {
        match command {
            VmCommand::Create { config: _, node_id: _ } => {
                // TODO: Interface with microvm.nix to create VM
                Err(BlixardError::NotImplemented {
                    feature: "VM creation via microvm.nix".to_string(),
                })
            }
            VmCommand::Start { name: _ } => {
                // TODO: Interface with microvm.nix to start VM
                Err(BlixardError::NotImplemented {
                    feature: "VM start via microvm.nix".to_string(),
                })
            }
            VmCommand::Stop { name: _ } => {
                // TODO: Interface with microvm.nix to stop VM
                Err(BlixardError::NotImplemented {
                    feature: "VM stop via microvm.nix".to_string(),
                })
            }
            VmCommand::Delete { name: _ } => {
                // TODO: Interface with microvm.nix to delete VM
                Err(BlixardError::NotImplemented {
                    feature: "VM deletion via microvm.nix".to_string(),
                })
            }
            VmCommand::UpdateStatus { name, status } => {
                let mut states = vm_states.write().await;
                if let Some(vm_state) = states.get_mut(&name) {
                    vm_state.status = status;
                    vm_state.updated_at = chrono::Utc::now();
                    
                    // Persist to database
                    Self::persist_vm_state(database, vm_state).await?;
                }
                Ok(())
            }
        }
    }
    
    async fn persist_vm_state(database: &Database, vm_state: &VmState) -> BlixardResult<()> {
        let write_txn = database.begin_write().map_err(|e| BlixardError::Storage {
            operation: "begin write transaction".to_string(),
            source: Box::new(e),
        })?;

        {
            let mut table = write_txn.open_table(VM_STATE_TABLE).map_err(|e| BlixardError::Storage {
                operation: "open vm_states table".to_string(),
                source: Box::new(e),
            })?;

            let serialized = bincode::serialize(vm_state).map_err(|e| BlixardError::Serialization {
                operation: "serialize vm state".to_string(),
                source: Box::new(e),
            })?;

            table.insert(vm_state.name.as_str(), serialized.as_slice()).map_err(|e| BlixardError::Storage {
                operation: "insert vm state".to_string(),
                source: Box::new(e),
            })?;
        }

        write_txn.commit().map_err(|e| BlixardError::Storage {
            operation: "commit transaction".to_string(),
            source: Box::new(e),
        })?;

        Ok(())
    }
}