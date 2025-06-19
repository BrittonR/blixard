use tokio::sync::mpsc;
use std::sync::Arc;
use redb::{Database, ReadableTable};

use crate::error::{BlixardError, BlixardResult};
use crate::types::{VmState, VmCommand, VmConfig, VmStatus};
use crate::storage::VM_STATE_TABLE;

/// Manages VM lifecycle operations
/// 
/// This is a stateless executor that receives commands from the Raft state machine
/// after consensus has been reached. It only executes VM operations and never
/// persists state directly - all state persistence happens through Raft.
/// 
/// ## Design Principles
/// 
/// 1. **No Local State**: VmManager does not maintain any VM state in memory
/// 2. **Read-Only Database Access**: Only reads from database for queries
/// 3. **Command Execution Only**: Executes VM lifecycle operations (start/stop/delete)
/// 4. **Raft Integration**: All state changes flow through Raft consensus first
/// 
/// ## Command Flow
/// 
/// 1. User request → gRPC server → SharedNodeState
/// 2. SharedNodeState creates Raft proposal → RaftManager
/// 3. RaftManager achieves consensus → writes to database
/// 4. RaftManager forwards command → VmManager (this struct)
/// 5. VmManager executes the actual VM operation
/// 
/// ## Future Work
/// 
/// When microvm.nix integration is implemented, this struct will:
/// - Interface with microvm.nix to create/start/stop/delete VMs
/// - Monitor VM health and report status changes back through Raft
/// - Handle VM migration between nodes
pub struct VmManager {
    database: Arc<Database>,
    pub(crate) command_tx: mpsc::UnboundedSender<VmCommand>,
}

impl VmManager {
    /// Create a new VM manager
    pub fn new(database: Arc<Database>) -> (Self, mpsc::UnboundedReceiver<VmCommand>) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        
        let manager = Self {
            database,
            command_tx,
        };
        
        (manager, command_rx)
    }
    
    /// Start the VM command processor
    pub fn start_processor(&self, mut command_rx: mpsc::UnboundedReceiver<VmCommand>) {
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if let Err(e) = Self::process_command(command).await {
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
        // Read from Raft-managed database for consistency across nodes
        let database = self.database.clone();
        let read_txn = database.begin_read()?;
        
        let mut result = Vec::new();
        
        if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
            for entry in table.iter()? {
                let (_key, value) = entry?;
                let vm_state: VmState = bincode::deserialize(value.value())?;
                result.push((vm_state.config, vm_state.status));
            }
        }
        
        Ok(result)
    }
    
    /// Get status of a specific VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        // Read from Raft-managed database for consistency across nodes
        let database = self.database.clone();
        let read_txn = database.begin_read()?;
        
        if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(name) {
                let vm_state: VmState = bincode::deserialize(data.value())?;
                Ok(Some((vm_state.config, vm_state.status)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
    
    async fn process_command(command: VmCommand) -> BlixardResult<()> {
        // This method only executes VM lifecycle operations.
        // All state persistence has already been handled by the RaftStateMachine
        // before this command was forwarded to us.
        match command {
            VmCommand::Create { config, node_id } => {
                // TODO: Interface with microvm.nix to create VM
                tracing::info!("VM '{}' creation command received for node {}", config.name, node_id);
                // For now, we don't actually create VMs, just log the command
                Ok(())
            }
            VmCommand::Start { name } => {
                // TODO: Interface with microvm.nix to start VM
                tracing::info!("VM '{}' start command received", name);
                Err(BlixardError::NotImplemented {
                    feature: "VM start via microvm.nix".to_string(),
                })
            }
            VmCommand::Stop { name } => {
                // TODO: Interface with microvm.nix to stop VM
                tracing::info!("VM '{}' stop command received", name);
                Err(BlixardError::NotImplemented {
                    feature: "VM stop via microvm.nix".to_string(),
                })
            }
            VmCommand::Delete { name } => {
                // TODO: Interface with microvm.nix to delete VM
                tracing::info!("VM '{}' delete command received", name);
                Err(BlixardError::NotImplemented {
                    feature: "VM deletion via microvm.nix".to_string(),
                })
            }
            VmCommand::UpdateStatus { name, status } => {
                // Status updates are already persisted by RaftStateMachine
                // This would be where we'd notify any watchers or update runtime state
                tracing::info!("VM '{}' status updated to {:?}", name, status);
                Ok(())
            }
        }
    }
}