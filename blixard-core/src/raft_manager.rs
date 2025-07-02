use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::{mpsc, RwLock, oneshot};
use tokio::time::{interval, Duration};
use raft::{Config, RawNode, StateRole, GetEntriesContext};
use raft::prelude::*;
use slog::{Logger, o, info, warn, error, Drain};
use redb::{Database, WriteTransaction, ReadableTable};
use chrono::Utc;
use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose};

use crate::error::{BlixardError, BlixardResult};
use crate::storage::{RedbRaftStorage, SnapshotData, VM_STATE_TABLE, CLUSTER_STATE_TABLE, 
    TASK_TABLE, TASK_ASSIGNMENT_TABLE, TASK_RESULT_TABLE, WORKER_TABLE, WORKER_STATUS_TABLE};
use crate::types::{VmCommand, VmStatus};
use crate::metrics_otel::{metrics, Timer, attributes};
use crate::config_global;

#[cfg(feature = "failpoints")]
use crate::fail_point;

// Raft message types for cluster communication
#[derive(Debug)]
pub enum RaftMessage {
    // Raft protocol messages
    Raft(raft::prelude::Message),
    // Application-specific messages
    Propose(RaftProposal),
    // Configuration changes
    ConfChange(RaftConfChange),
}

#[derive(Debug)]
pub struct RaftConfChange {
    pub change_type: ConfChangeType,
    pub node_id: u64,
    pub address: String,
    pub response_tx: Option<oneshot::Sender<BlixardResult<()>>>,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfChangeType {
    AddNode,
    RemoveNode,
}

#[derive(Debug)]
pub struct RaftProposal {
    pub id: Vec<u8>,
    pub data: ProposalData,
    pub response_tx: Option<oneshot::Sender<BlixardResult<()>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalData {
    // Task management
    AssignTask { task_id: String, node_id: u64, task: TaskSpec },
    CompleteTask { task_id: String, result: TaskResult },
    ReassignTask { task_id: String, from_node: u64, to_node: u64 },
    
    // Worker management
    RegisterWorker { node_id: u64, address: String, capabilities: WorkerCapabilities, topology: crate::types::NodeTopology },
    UpdateWorkerStatus { node_id: u64, status: WorkerStatus },
    RemoveWorker { node_id: u64 },
    
    // VM operations
    // Note: CreateVm is a misnomer - it handles all VM operations (create, start, stop, delete)
    // TODO: Rename to VmOperation(VmCommand) for clarity
    CreateVm(VmCommand),
    UpdateVmStatus { vm_name: String, status: VmStatus, node_id: u64 },
    MigrateVm { vm_name: String, from_node: u64, to_node: u64 },
    
    // Batch processing
    Batch(Vec<ProposalData>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub command: String,
    pub args: Vec<String>,
    pub resources: ResourceRequirements,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>, // e.g., ["gpu", "high-memory", "ssd"]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub required_features: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WorkerStatus {
    Online = 0,
    Busy = 1,
    Offline = 2,
    Failed = 3,
}

impl TryFrom<u8> for WorkerStatus {
    type Error = BlixardError;
    
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WorkerStatus::Online),
            1 => Ok(WorkerStatus::Busy),
            2 => Ok(WorkerStatus::Offline),
            3 => Ok(WorkerStatus::Failed),
            _ => Err(BlixardError::Internal {
                message: format!("Invalid WorkerStatus value: {}", value),
            }),
        }
    }
}

// State machine that applies Raft log entries to redb
pub struct RaftStateMachine {
    database: Arc<Database>,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
}

impl RaftStateMachine {
    pub fn new(database: Arc<Database>, shared_state: Weak<crate::node_shared::SharedNodeState>) -> Self {
        Self { database, shared_state }
    }

    pub async fn apply_entry(&self, entry: &Entry) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("raft::apply_entry");
        
        if entry.data.is_empty() {
            return Ok(());
        }

        let proposal: ProposalData = bincode::deserialize(&entry.data)
            .map_err(|e| BlixardError::Serialization {
                operation: "deserialize proposal".to_string(),
                source: Box::new(e),
            })?;

        let write_txn = self.database.begin_write()
            .map_err(|e| BlixardError::Storage {
                operation: "begin write transaction".to_string(),
                source: Box::new(e),
            })?;

        match proposal {
            ProposalData::AssignTask { task_id, node_id, task } => {
                self.apply_assign_task(write_txn, &task_id, node_id, &task)?;
            }
            ProposalData::CompleteTask { task_id, result } => {
                self.apply_complete_task(write_txn, &task_id, &result)?;
            }
            ProposalData::RegisterWorker { node_id, address, capabilities, topology } => {
                self.apply_register_worker(write_txn, node_id, &address, &capabilities, &topology)?;
            }
            ProposalData::UpdateWorkerStatus { node_id, status } => {
                self.apply_update_worker_status(write_txn, node_id, status)?;
            }
            ProposalData::CreateVm(vm_command) => {
                self.apply_vm_command(write_txn, &vm_command)?;
            }
            ProposalData::UpdateVmStatus { vm_name, status, node_id } => {
                self.apply_update_vm_status(write_txn, &vm_name, status, node_id)?;
            }
            ProposalData::MigrateVm { vm_name, from_node, to_node } => {
                // Create a migration task and apply it through VM command
                let task = crate::types::VmMigrationTask {
                    vm_name: vm_name.clone(),
                    source_node_id: from_node,
                    target_node_id: to_node,
                    live_migration: false,
                    force: false,
                };
                let command = VmCommand::Migrate { task };
                self.apply_vm_command(write_txn, &command)?;
            }
            ProposalData::Batch(proposals) => {
                // TODO: Optimize batch processing to use a single transaction
                // Currently each sub-proposal gets its own transaction due to
                // the way apply_entry works. This should be refactored to pass
                // the transaction through to allow true batch processing.
                
                // Commit the current transaction first
                write_txn.commit()
                    .map_err(|e| BlixardError::Storage {
                        operation: "commit batch transaction".to_string(),
                        source: Box::new(e),
                    })?;
                
                // Apply each proposal in the batch sequentially
                for sub_proposal in proposals {
                    // Prevent nested batches
                    if matches!(sub_proposal, ProposalData::Batch(_)) {
                        return Err(BlixardError::Internal {
                            message: "Nested batch proposals are not supported".to_string(),
                        });
                    }
                    
                    // Create an entry for the sub-proposal and apply it
                    let sub_entry = Entry {
                        entry_type: entry.entry_type,
                        term: entry.term,
                        index: entry.index,
                        data: bincode::serialize(&sub_proposal)
                            .map_err(|e| BlixardError::Serialization {
                                operation: "serialize sub-proposal".to_string(),
                                source: Box::new(e),
                            })?,
                        context: entry.context.clone(),
                        sync_log: entry.sync_log,
                    };
                    
                    // Recursively apply the sub-proposal
                    Box::pin(self.apply_entry(&sub_entry)).await?;
                }
                
                return Ok(());
            }
            _ => {
                // Handle other proposal types
                // Since we don't handle this proposal type, commit empty transaction
                write_txn.commit()
                    .map_err(|e| BlixardError::Storage {
                        operation: "commit transaction".to_string(),
                        source: Box::new(e),
                    })?;
            }
        }

        Ok(())
    }

    fn apply_assign_task(&self, txn: WriteTransaction, task_id: &str, node_id: u64, task: &TaskSpec) -> BlixardResult<()> {
        use crate::storage::{TASK_TABLE, TASK_ASSIGNMENT_TABLE};
        
        {
            // Store task spec
            let mut task_table = txn.open_table(TASK_TABLE)?;
            let task_data = bincode::serialize(task)
                .map_err(|e| BlixardError::Serialization {
                    operation: "serialize task spec".to_string(),
                    source: Box::new(e),
                })?;
            task_table.insert(task_id, task_data.as_slice())?;
            
            // Store assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            assignment_table.insert(task_id, node_id.to_le_bytes().as_slice())?;
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_complete_task(&self, txn: WriteTransaction, task_id: &str, result: &TaskResult) -> BlixardResult<()> {
        use crate::storage::{TASK_RESULT_TABLE, TASK_ASSIGNMENT_TABLE};
        
        {
            // Store result
            let mut result_table = txn.open_table(TASK_RESULT_TABLE)?;
            let result_data = bincode::serialize(result)
                .map_err(|e| BlixardError::Serialization {
                    operation: "serialize task result".to_string(),
                    source: Box::new(e),
                })?;
            result_table.insert(task_id, result_data.as_slice())?;
            
            // Remove assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            assignment_table.remove(task_id)?;
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_register_worker(&self, txn: WriteTransaction, node_id: u64, address: &str, capabilities: &WorkerCapabilities, topology: &crate::types::NodeTopology) -> BlixardResult<()> {
        use crate::storage::{WORKER_TABLE, WORKER_STATUS_TABLE, NODE_TOPOLOGY_TABLE};
        
        let worker_data = bincode::serialize(&(address, capabilities))
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize worker info".to_string(),
                source: Box::new(e),
            })?;
        
        let topology_data = bincode::serialize(topology)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize node topology".to_string(),
                source: Box::new(e),
            })?;
        
        {
            // Store worker info
            let mut worker_table = txn.open_table(WORKER_TABLE)?;
            worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
            
            // Set initial status
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.insert(node_id.to_le_bytes().as_slice(), [WorkerStatus::Online as u8].as_slice())?;
            
            // Store topology info
            let mut topology_table = txn.open_table(NODE_TOPOLOGY_TABLE)?;
            topology_table.insert(node_id.to_le_bytes().as_slice(), topology_data.as_slice())?;
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_update_worker_status(&self, txn: WriteTransaction, node_id: u64, status: WorkerStatus) -> BlixardResult<()> {
        use crate::storage::WORKER_STATUS_TABLE;
        
        {
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.insert(node_id.to_le_bytes().as_slice(), [status as u8].as_slice())?;
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_vm_command(&self, txn: WriteTransaction, command: &VmCommand) -> BlixardResult<()> {
        tracing::info!("🔥 apply_vm_command called with command: {:?}", command);
        use crate::storage::VM_STATE_TABLE;
        
        // For delete commands, we need to check ownership BEFORE database removal
        let delete_node_id: Option<u64> = if let VmCommand::Delete { name } = command {
            // Read the VM state before removal to determine which node owns this VM
            let table = txn.open_table(VM_STATE_TABLE)?;
            let node_id = match table.get(name.as_str()) {
                Ok(Some(data)) => {
                    // Clone the data to avoid borrow checker issues
                    let vm_data: Vec<u8> = data.value().to_vec();
                    match bincode::deserialize::<crate::types::VmState>(&vm_data) {
                        Ok(vm_state) => Some(vm_state.node_id),
                        Err(_) => None,
                    }
                }
                _ => None,
            };
            node_id
        } else {
            None
        };
        
        {
            let mut table = txn.open_table(VM_STATE_TABLE)?;
            
            match command {
                VmCommand::Create { config, node_id } => {
                    let vm_state = crate::types::VmState {
                        name: config.name.clone(),
                        config: config.clone(),
                        status: VmStatus::Creating,
                        node_id: *node_id,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };
                    
                    let vm_data = bincode::serialize(&vm_state)
                        .map_err(|e| BlixardError::Serialization {
                            operation: "serialize vm state".to_string(),
                            source: Box::new(e),
                        })?;
                    table.insert(config.name.as_str(), vm_data.as_slice())?;
                }
                VmCommand::UpdateStatus { name, status } => {
                    let serialized = {
                        let vm_state_data = table.get(name.as_str())?;
                        if let Some(data) = vm_state_data {
                            let mut vm_state: crate::types::VmState = bincode::deserialize(data.value())
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "deserialize vm state".to_string(),
                                    source: Box::new(e),
                                })?;
                            vm_state.status = *status;
                            vm_state.updated_at = Utc::now();
                            Some(bincode::serialize(&vm_state)
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "serialize vm state".to_string(),
                                    source: Box::new(e),
                                })?)
                        } else {
                            None
                        }
                    };
                    
                    if let Some(data) = serialized {
                        table.insert(name.as_str(), data.as_slice())?;
                    }
                }
                VmCommand::Start { name } => {
                    // Update status to Starting
                    let serialized = {
                        let vm_state_data = table.get(name.as_str())?;
                        if let Some(data) = vm_state_data {
                            let mut vm_state: crate::types::VmState = bincode::deserialize(data.value())
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "deserialize vm state".to_string(),
                                    source: Box::new(e),
                                })?;
                            vm_state.status = VmStatus::Starting;
                            vm_state.updated_at = Utc::now();
                            Some(bincode::serialize(&vm_state)
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "serialize vm state".to_string(),
                                    source: Box::new(e),
                                })?)
                        } else {
                            None
                        }
                    };
                    
                    if let Some(data) = serialized {
                        table.insert(name.as_str(), data.as_slice())?;
                    }
                }
                VmCommand::Stop { name } => {
                    // Update status to Stopping
                    let serialized = {
                        let vm_state_data = table.get(name.as_str())?;
                        if let Some(data) = vm_state_data {
                            let mut vm_state: crate::types::VmState = bincode::deserialize(data.value())
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "deserialize vm state".to_string(),
                                    source: Box::new(e),
                                })?;
                            vm_state.status = VmStatus::Stopping;
                            vm_state.updated_at = Utc::now();
                            Some(bincode::serialize(&vm_state)
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "serialize vm state".to_string(),
                                    source: Box::new(e),
                                })?)
                        } else {
                            None
                        }
                    };
                    
                    if let Some(data) = serialized {
                        table.insert(name.as_str(), data.as_slice())?;
                    }
                }
                VmCommand::Delete { name } => {
                    // Remove VM from database
                    table.remove(name.as_str())?;
                }
                VmCommand::Migrate { task } => {
                    // Update VM's node assignment
                    let serialized = {
                        let vm_state_data = table.get(task.vm_name.as_str())?;
                        if let Some(data) = vm_state_data {
                            let mut vm_state: crate::types::VmState = bincode::deserialize(data.value())
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "deserialize vm state".to_string(),
                                    source: Box::new(e),
                                })?;
                            // Update node assignment
                            vm_state.node_id = task.target_node_id;
                            vm_state.updated_at = Utc::now();
                            Some(bincode::serialize(&vm_state)
                                .map_err(|e| BlixardError::Serialization {
                                    operation: "serialize vm state".to_string(),
                                    source: Box::new(e),
                                })?)
                        } else {
                            None
                        }
                    };
                    
                    if let Some(data) = serialized {
                        table.insert(task.vm_name.as_str(), data.as_slice())?;
                    }
                }
            }
        }
        
        txn.commit()?;
        
        // Forward command to VM manager for actual execution (only on the target node)
        if let Some(shared) = self.shared_state.upgrade() {
            // Check if this command is for this node
            let should_execute = match command {
                VmCommand::Create { node_id, .. } => *node_id == shared.get_id(),
                VmCommand::Delete { .. } => {
                    // For delete commands, use the pre-stored ownership information
                    if let Some(owner_node_id) = delete_node_id {
                        owner_node_id == shared.get_id()
                    } else {
                        // If we couldn't determine ownership, let any node try to clean up
                        // This is safe because only the node that actually has the VM can delete it
                        true
                    }
                }
                _ => {
                    // For other commands, check if VM is assigned to this node
                    if let Ok(read_txn) = self.database.begin_read() {
                        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
                            match command {
                                VmCommand::Start { name } | VmCommand::Stop { name } => {
                                    if let Ok(Some(data)) = table.get(name.as_str()) {
                                        if let Ok(vm_state) = bincode::deserialize::<crate::types::VmState>(data.value()) {
                                            vm_state.node_id == shared.get_id()
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                }
                                VmCommand::Migrate { task } => {
                                    // Migration needs to be executed on both source and target nodes
                                    task.source_node_id == shared.get_id() || task.target_node_id == shared.get_id()
                                }
                                _ => false,
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };
            
            if should_execute {
                // Use tokio::spawn to send the command asynchronously
                let command_clone = command.clone();
                let shared_clone = shared.clone();
                tokio::spawn(async move {
                    if let Err(e) = shared_clone.send_vm_command(command_clone).await {
                        tracing::error!("Failed to forward VM command to VM manager: {}", e);
                    }
                });
            }
        }
        
        Ok(())
    }

    fn apply_update_vm_status(&self, txn: WriteTransaction, vm_name: &str, status: VmStatus, node_id: u64) -> BlixardResult<()> {
        use crate::storage::VM_STATE_TABLE;
        
        {
            let mut table = txn.open_table(VM_STATE_TABLE)?;
            let serialized = {
                let vm_state_data = table.get(vm_name)?;
                if let Some(data) = vm_state_data {
                    let mut vm_state: crate::types::VmState = bincode::deserialize(data.value())
                        .map_err(|e| BlixardError::Serialization {
                            operation: "deserialize vm state".to_string(),
                            source: Box::new(e),
                        })?;
                    vm_state.status = status;
                    vm_state.node_id = node_id;
                    vm_state.updated_at = Utc::now();
                    Some(bincode::serialize(&vm_state)
                        .map_err(|e| BlixardError::Serialization {
                            operation: "serialize vm state".to_string(),
                            source: Box::new(e),
                        })?)
                } else {
                    None
                }
            };
            
            if let Some(data) = serialized {
                table.insert(vm_name, data.as_slice())?;
            }
        }
        
        txn.commit()?;
        Ok(())
    }

    /// Apply a snapshot to the state machine
    /// This is called when the Raft layer receives a snapshot from the leader
    pub fn apply_snapshot(&self, snapshot_data: &[u8]) -> BlixardResult<()> {
        // Deserialize the snapshot data
        let snapshot: SnapshotData = bincode::deserialize(snapshot_data)
            .map_err(|e| BlixardError::Serialization {
                operation: "deserialize snapshot".to_string(),
                source: Box::new(e),
            })?;

        // Apply the snapshot within a single transaction for atomicity
        let write_txn = self.database.begin_write()
            .map_err(|e| BlixardError::Storage {
                operation: "begin snapshot transaction".to_string(),
                source: Box::new(e),
            })?;

        // Clear and restore VM states
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE)?;
            // Clear existing entries
            let keys_to_remove: Vec<String> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_str())?;
            }
            // Apply new entries
            for (key, value) in &snapshot.vm_states {
                table.insert(key.as_str(), value.as_slice())?;
            }
        }

        // Clear and restore cluster state
        {
            let mut table = write_txn.open_table(CLUSTER_STATE_TABLE)?;
            let keys_to_remove: Vec<String> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_str())?;
            }
            for (key, value) in &snapshot.cluster_state {
                table.insert(key.as_str(), value.as_slice())?;
            }
        }

        // Clear and restore tasks
        {
            let mut table = write_txn.open_table(TASK_TABLE)?;
            let keys_to_remove: Vec<String> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_str())?;
            }
            for (key, value) in &snapshot.tasks {
                table.insert(key.as_str(), value.as_slice())?;
            }
        }

        // Clear and restore task assignments
        {
            let mut table = write_txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            let keys_to_remove: Vec<String> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_str())?;
            }
            for (key, value) in &snapshot.task_assignments {
                table.insert(key.as_str(), value.as_slice())?;
            }
        }

        // Clear and restore task results
        {
            let mut table = write_txn.open_table(TASK_RESULT_TABLE)?;
            let keys_to_remove: Vec<String> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_str())?;
            }
            for (key, value) in &snapshot.task_results {
                table.insert(key.as_str(), value.as_slice())?;
            }
        }

        // Clear and restore workers
        {
            let mut table = write_txn.open_table(WORKER_TABLE)?;
            let keys_to_remove: Vec<Vec<u8>> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_vec()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_slice())?;
            }
            for (key, value) in &snapshot.workers {
                table.insert(key.as_slice(), value.as_slice())?;
            }
        }

        // Clear and restore worker status
        {
            let mut table = write_txn.open_table(WORKER_STATUS_TABLE)?;
            let keys_to_remove: Vec<Vec<u8>> = table.iter()?
                .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_vec()))
                .collect();
            for key in keys_to_remove {
                table.remove(key.as_slice())?;
            }
            for (key, value) in &snapshot.worker_status {
                table.insert(key.as_slice(), value.as_slice())?;
            }
        }

        // Commit all changes atomically
        write_txn.commit()
            .map_err(|e| BlixardError::Storage {
                operation: "commit snapshot transaction".to_string(),
                source: Box::new(e),
            })?;

        tracing::info!("Applied snapshot to state machine");
        Ok(())
    }
}

// Main Raft manager that handles the Raft protocol
pub struct RaftManager {
    node_id: u64,
    raft_node: RwLock<RawNode<RedbRaftStorage>>,
    state_machine: Arc<RaftStateMachine>,
    storage: RedbRaftStorage,  // Keep a reference to storage for conf state updates
    peers: RwLock<HashMap<u64, String>>, // node_id -> address
    logger: Logger,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    
    // Channels for communication
    proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
    proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    _message_tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
    conf_change_tx: mpsc::UnboundedSender<RaftConfChange>,
    
    // Track pending proposals
    pending_proposals: Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>,
    
    // Track last log compaction index
    last_compaction_index: Arc<RwLock<Option<u64>>>,
    
    // Message sender callback - using channel instead of async closure
    outgoing_messages: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    
    // Flag to trigger replication after config changes
    needs_replication_trigger: Arc<RwLock<bool>>,
}

impl RaftManager {
    pub fn new(
        node_id: u64,
        database: Arc<Database>,
        peers: Vec<(u64, String)>,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
    ) -> BlixardResult<(
        Self,
        mpsc::UnboundedSender<RaftProposal>,
        mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
        mpsc::UnboundedSender<RaftConfChange>,
        mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>
    )> {
        // Create Raft configuration
        let cfg = Config {
            id: node_id,
            election_tick: 10,
            heartbeat_tick: 3,
            applied: 0,
            max_size_per_msg: 1024 * 1024,
            max_inflight_msgs: 256,
            ..Default::default()
        };

        // Initialize storage
        let storage = RedbRaftStorage { database: database.clone() };
        
        // Create logger
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let logger = slog::Logger::root(drain, o!("node_id" => node_id));

        // Create Raft node with a clone of storage
        let raft_node = RawNode::new(&cfg, storage.clone(), &logger)
            .map_err(|e| BlixardError::Raft {
                operation: "create raft node".to_string(),
                source: Box::new(e),
            })?;

        // Create state machine
        let state_machine = Arc::new(RaftStateMachine::new(database, shared_state.clone()));

        // Create channels
        let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (conf_change_tx, conf_change_rx) = mpsc::unbounded_channel();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        
        // Set up batch processing if enabled
        let final_proposal_tx = if let Some(_shared) = shared_state.upgrade() {
            let config = config_global::get();
            let batch_config = crate::raft_batch_processor::BatchConfig {
                enabled: config.cluster.raft.batch_processing.enabled,
                max_batch_size: config.cluster.raft.batch_processing.max_batch_size,
                batch_timeout_ms: config.cluster.raft.batch_processing.batch_timeout_ms,
                max_batch_bytes: config.cluster.raft.batch_processing.max_batch_bytes,
            };
            
            if batch_config.enabled {
                // Create batch processor
                let (batch_tx, batch_processor) = crate::raft_batch_processor::create_batch_processor(
                    batch_config,
                    proposal_tx.clone(),
                    node_id,
                );
                
                // Spawn batch processor
                tokio::spawn(batch_processor.run());
                
                // Use batch processor's channel for sending proposals
                batch_tx
            } else {
                // Use direct channel
                proposal_tx.clone()
            }
        } else {
            proposal_tx.clone()
        };

        let manager = Self {
            node_id,
            raft_node: RwLock::new(raft_node),
            state_machine,
            storage,
            peers: RwLock::new(peers.into_iter().collect()),
            logger,
            shared_state,
            proposal_rx,
            proposal_tx: final_proposal_tx.clone(),
            message_rx,
            _message_tx: message_tx.clone(),
            conf_change_rx,
            conf_change_tx: conf_change_tx.clone(),
            pending_proposals: Arc::new(RwLock::new(HashMap::new())),
            last_compaction_index: Arc::new(RwLock::new(None)),
            outgoing_messages: outgoing_tx,
            needs_replication_trigger: Arc::new(RwLock::new(false)),
        };

        Ok((manager, final_proposal_tx, message_tx, conf_change_tx, outgoing_rx))
    }
    
    /// Bootstrap a single-node cluster
    pub async fn bootstrap_single_node(&self) -> BlixardResult<()> {
        {
            let mut node = self.raft_node.write().await;
            
            // Campaign to become leader
            let _ = node.campaign();
            
            info!(self.logger, "[RAFT-BOOTSTRAP] After bootstrap campaign"; 
                "state" => ?node.raft.state,
                "commit" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
        }
        
        // Process ready immediately to handle the bootstrap entry
        self.on_ready().await?;
        
        // Propose an empty entry to establish leadership
        {
            let mut node = self.raft_node.write().await;
            if node.raft.state == StateRole::Leader {
                info!(self.logger, "[RAFT-BOOTSTRAP] Proposing empty entry to establish leadership");
                node.propose(vec![], vec![])?;
            }
        }
        
        // Process ready again to commit the empty entry
        self.on_ready().await?;
        
        Ok(())
    }

    pub async fn run(mut self) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-RUN] Starting RaftManager::run() method");
        
        // Check if we should bootstrap as a single-node cluster
        let should_bootstrap = {
            let peers = self.peers.read().await;
            let has_join_addr = if let Some(shared) = self.shared_state.upgrade() {
                shared.config.join_addr.is_some()
            } else {
                false
            };
            
            // Only bootstrap if we have no peers AND no join address
            peers.is_empty() && !has_join_addr
        };
        
        if should_bootstrap {
            // No peers and no join address, bootstrap as single node
            info!(self.logger, "No peers configured and no join address, bootstrapping as single-node cluster");
            match self.bootstrap_single_node().await {
                Ok(_) => info!(self.logger, "[RAFT-RUN] Bootstrap succeeded"),
                Err(e) => {
                    error!(self.logger, "[RAFT-RUN] Bootstrap failed, exiting run()"; "error" => %e);
                    return Err(e);
                }
            }
        } else {
            // Either we have peers or a join address - don't bootstrap
            let peer_ids: Vec<u64> = self.peers.read().await.keys().cloned().collect();
            let has_join_addr = if let Some(shared) = self.shared_state.upgrade() {
                shared.config.join_addr.is_some()
            } else {
                false
            };
            info!(self.logger, "Not bootstrapping"; "peers" => ?peer_ids, "has_join_addr" => has_join_addr);
        }
        
        let mut tick_timer = interval(Duration::from_millis(100));
        
        let mut tick_count = 0u64;
        info!(self.logger, "[RAFT-RUN] Entering main run loop");
        loop {
            tokio::select! {
                _ = tick_timer.tick() => {
                    tick_count += 1;
                    if tick_count % 50 == 0 {  // Log every 5 seconds
                        info!(self.logger, "[RAFT-TICK] Tick #{}", tick_count);
                    }
                    if let Err(e) = self.tick().await {
                        error!(self.logger, "[RAFT-RUN] tick() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some(proposal) = self.proposal_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received proposal");
                    if let Err(e) = self.handle_proposal(proposal).await {
                        error!(self.logger, "[RAFT-RUN] handle_proposal() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some((from, msg)) = self.message_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received message from {}", from);
                    if let Err(e) = self.handle_raft_message(from, msg).await {
                        error!(self.logger, "[RAFT-RUN] handle_raft_message() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some(conf_change) = self.conf_change_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received conf change");
                    if let Err(e) = self.handle_conf_change(conf_change).await {
                        error!(self.logger, "[RAFT-RUN] handle_conf_change() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                else => {
                    // None of the channels have data, all closed
                    error!(self.logger, "[RAFT-RUN] All channels closed, exiting run()");
                    return Err(BlixardError::Internal {
                        message: "All RaftManager channels closed".to_string(),
                    });
                }
            }
            
            // Process ready state after any event
            // Keep processing until there are no more ready states
            loop {
                match self.on_ready().await {
                    Ok(false) => break,
                    Ok(true) => continue,
                    Err(e) => {
                        error!(self.logger, "[RAFT-RUN] on_ready() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
            }
            
            // After processing all ready states, check if we have unapplied commits
            // This can happen in single-node clusters where commits happen in light ready
            {
                let node = self.raft_node.read().await;
                if node.raft.raft_log.committed > node.raft.raft_log.applied {
                    info!(self.logger, "[RAFT-LOOP] Unapplied commits after ready processing";
                        "committed" => node.raft.raft_log.committed,
                        "applied" => node.raft.raft_log.applied
                    );
                    drop(node);
                    
                    // Manually trigger a tick to force Raft to generate a new ready state
                    info!(self.logger, "[RAFT-LOOP] Triggering tick to process unapplied commits");
                    self.tick().await?;
                    
                    // Process any new ready states
                    let mut ready_count = 0;
                    loop {
                        match self.on_ready().await {
                            Ok(false) => break,
                            Ok(true) => ready_count += 1,
                            Err(e) => {
                                error!(self.logger, "[RAFT-RUN] on_ready() failed during unapplied commits processing, exiting run()"; "error" => %e);
                                return Err(e);
                            }
                        }
                    }
                    
                    if ready_count == 0 {
                        warn!(self.logger, "[RAFT-LOOP] No ready states after tick for unapplied commits");
                    } else {
                        info!(self.logger, "[RAFT-LOOP] Processed {} ready states after tick", ready_count);
                    }
                }
            }
            
            // Check if we need to trigger replication after conf change
            let needs_trigger = {
                let mut flag = self.needs_replication_trigger.write().await;
                let needs = *flag;
                *flag = false; // Reset the flag
                needs
            };
            
            if needs_trigger {
                let mut node = self.raft_node.write().await;
                if node.raft.state == StateRole::Leader {
                    info!(self.logger, "[RAFT-CONF] Triggering replication by proposing empty entry");
                    node.propose(vec![], vec![])?;
                    // The next ready cycle will send AppendEntries to all nodes
                }
            }
            
            // Check if any nodes need snapshots (only leaders should do this)
            // This is done AFTER all ready processing is complete to avoid generating
            // messages during the advance phase
            {
                let mut node = self.raft_node.write().await;
                if node.raft.state == raft::StateRole::Leader {
                    if let Err(e) = self.check_and_send_snapshots(&mut node).await {
                        error!(self.logger, "[RAFT-RUN] check_and_send_snapshots() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn tick(&self) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        node.tick();
        
        // Update shared state with current Raft status
        if let Some(shared) = self.shared_state.upgrade() {
            let is_leader = node.raft.state == StateRole::Leader;
            let leader_id = if is_leader {
                Some(self.node_id)
            } else {
                // Raft uses 0 to indicate "no leader known"
                let raft_leader = node.raft.leader_id;
                if raft_leader == 0 {
                    None
                } else {
                    Some(raft_leader)
                }
            };
            
            let status = crate::node_shared::RaftStatus {
                is_leader,
                node_id: self.node_id,
                leader_id,
                term: node.raft.term,
                state: format!("{:?}", node.raft.state).to_lowercase(),
            };
            
            shared.update_raft_status(status).await;
        }
        
        // After tick, check if there are any messages to send (e.g., heartbeats)
        let msgs = node.raft.msgs.drain(..).collect::<Vec<_>>();
        drop(node); // Release the lock before sending messages
        
        if !msgs.is_empty() {
            for msg in msgs {
                self.send_raft_message(msg).await?;
            }
        }
        
        Ok(())
    }

    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.raft_proposal_duration.clone(),
            vec![attributes::node_id(self.node_id)]
        );
        metrics.raft_proposals_total.add(1, &[attributes::node_id(self.node_id)]);
        
        let proposal_id = proposal.id.clone();
        let has_response = proposal.response_tx.is_some();
        
        info!(self.logger, "[RAFT-PROPOSAL] Handling proposal"; 
            "id_len" => proposal_id.len(),
            "has_response_tx" => has_response,
            "data_type" => format!("{:?}", proposal.data)
        );
        
        // Store the response channel if provided
        if let Some(response_tx) = proposal.response_tx {
            let mut pending = self.pending_proposals.write().await;
            pending.insert(proposal_id.clone(), response_tx);
            info!(self.logger, "[RAFT-PROPOSAL] Stored response channel"; 
                "pending_count" => pending.len()
            );
        }
        
        let data = bincode::serialize(&proposal.data)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize proposal".to_string(),
                source: Box::new(e),
            })?;
        
        info!(self.logger, "[RAFT-PROPOSAL] Proposing to Raft"; 
            "id_len" => proposal_id.len(),
            "data_len" => data.len()
        );
        
        let mut node = self.raft_node.write().await;
        
        // Ensure we're passing the proposal_id as context correctly
        info!(self.logger, "[RAFT-PROPOSAL] About to call node.propose"; 
            "context_len" => proposal_id.len(),
            "data_len" => data.len()
        );
        
        match node.propose(proposal_id.clone(), data) {
            Ok(_) => {
                info!(self.logger, "[RAFT-PROPOSAL] Proposal submitted to Raft successfully");
            }
            Err(e) => {
                warn!(self.logger, "[RAFT-PROPOSAL] Failed to propose"; "error" => %e);
                
                // If proposal fails, remove from pending and send error
                let error_msg = format!("Failed to propose: {}", e);
                
                if has_response {
                    let mut pending = self.pending_proposals.write().await;
                    if let Some(response_tx) = pending.remove(&proposal_id) {
                        let _ = response_tx.send(Err(BlixardError::Internal {
                            message: error_msg.clone(),
                        }));
                    }
                }
                
                return Err(BlixardError::Raft {
                    operation: "propose".to_string(),
                    source: Box::new(e),
                });
            }
        }
        
        Ok(())
    }

    async fn handle_raft_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-MSG] Received message"; "from" => from, "to" => msg.to, "type" => ?msg.msg_type());
        
        // Log AppendEntries responses in detail
        if msg.msg_type() == raft::prelude::MessageType::MsgAppendResponse {
            info!(self.logger, "[RAFT-MSG] Received MsgAppendResponse";
                "from" => msg.from,
                "to" => msg.to,
                "index" => msg.index,
                "reject" => msg.reject,
                "reject_hint" => msg.reject_hint,
                "log_term" => msg.log_term
            );
        }
        
        // Log snapshot messages
        if msg.msg_type() == raft::prelude::MessageType::MsgSnapshot {
            let snapshot = msg.get_snapshot();
            let metadata = snapshot.get_metadata();
            info!(self.logger, "[RAFT-MSG] Received MsgSnapshot";
                "from" => msg.from,
                "to" => msg.to,
                "snapshot_index" => metadata.index,
                "snapshot_term" => metadata.term,
                "snapshot_voters" => ?metadata.get_conf_state().voters,
                "data_size" => snapshot.data.len()
            );
        }
        
        // Ensure we know about the sender for all messages, not just MsgAppend
        // This is critical for bidirectional communication
        if let Some(shared) = self.shared_state.upgrade() {
            // For any message from a peer we don't know about, try to get their info
            let mut peers = self.peers.write().await;
            if !peers.contains_key(&from) {
                if let Some(peer_info) = shared.get_peer(from).await {
                    peers.insert(from, peer_info.address.clone());
                    info!(self.logger, "[RAFT-MSG] Added sender {} at {} to peer list from incoming {:?} message", 
                        from, peer_info.address, msg.msg_type());
                    
                    // Also ensure we have a connection to this peer
                    if let Some(peer_connector) = shared.get_peer_connector().await {
                        self.connect_to_peer_with_p2p(peer_connector, &peer_info).await;
                    }
                }
            }
        }
        
        let mut node = self.raft_node.write().await;
        
        // Check if the sender is still in the configuration before processing the message
        // This prevents crashes when messages arrive from recently removed nodes
        let current_voters = node.raft.prs().conf().voters().clone();
        
        // Special handling for nodes that might not have their configuration yet
        // If our configuration is empty (just joined), we should accept messages from nodes
        // we've added as peers during the join process
        if !current_voters.contains(from) {
            // Check if this is a joining node that hasn't processed its configuration yet
            // In this case, check if sender is in our peer list (was added during join)
            let peers = self.peers.read().await;
            info!(self.logger, "[RAFT-MSG] Checking peers for sender";
                "from" => from,
                "peers" => ?peers.keys().cloned().collect::<Vec<_>>(),
                "contains_sender" => peers.contains_key(&from)
            );
            if peers.contains_key(&from) {
                // Check if we have an empty configuration (common for joining nodes)
                let conf_state = self.storage.load_conf_state().unwrap_or_default();
                info!(self.logger, "[RAFT-MSG] Loaded conf state from storage";
                    "voters" => ?conf_state.voters,
                    "is_empty" => conf_state.voters.is_empty()
                );
                if conf_state.voters.is_empty() {
                    info!(self.logger, "[RAFT-MSG] Accepting message from peer - joining node with empty configuration";
                        "from" => from,
                        "msg_type" => ?msg.msg_type()
                    );
                } else if conf_state.voters.contains(&from) {
                    // The sender is in our stored configuration but not in Raft's current view
                    // This happens when a node joins and saves configuration before Raft processes it
                    info!(self.logger, "[RAFT-MSG] Accepting message - sender in stored configuration but not in Raft state";
                        "from" => from,
                        "stored_voters" => ?conf_state.voters,
                        "raft_voters" => ?current_voters,
                        "msg_type" => ?msg.msg_type()
                    );
                } else {
                    warn!(self.logger, "[RAFT-MSG] Discarding message - sender not in stored configuration";
                        "from" => from,
                        "current_voters" => ?current_voters,
                        "conf_state_voters" => ?conf_state.voters,
                        "msg_type" => ?msg.msg_type()
                    );
                    return Ok(());
                }
            } else {
                warn!(self.logger, "[RAFT-MSG] Discarding message from node not in configuration";
                    "from" => from,
                    "current_voters" => ?current_voters,
                    "msg_type" => ?msg.msg_type()
                );
                return Ok(());
            }
        }
        
        node.step(msg)
            .map_err(|e| BlixardError::Raft {
                operation: "step message".to_string(),
                source: Box::new(e),
            })?;
        
        // After stepping a message, check if there are any response messages to send
        // Some messages (like MsgAppendResponse) may be generated immediately but don't
        // trigger has_ready(), so we need to check and send them right away
        let msgs = node.raft.msgs.drain(..).collect::<Vec<_>>();
        drop(node); // Release the lock before sending messages
        
        if !msgs.is_empty() {
            info!(self.logger, "[RAFT-MSG] Found {} messages after step", msgs.len());
            for msg in msgs {
                info!(self.logger, "[RAFT-MSG] Sending response after step"; 
                    "from" => msg.from, "to" => msg.to, "type" => ?msg.msg_type());
                self.send_raft_message(msg).await?;
            }
        }
        
        Ok(())
    }
    
    async fn handle_conf_change(&self, conf_change: RaftConfChange) -> BlixardResult<()> {
        use raft::prelude::ConfChangeType as RaftConfChangeType;
        
        info!(self.logger, "[RAFT-CONF] Handling configuration change"; "type" => ?conf_change.change_type, "node_id" => conf_change.node_id, "address" => ?conf_change.address);
        
        // Log current Raft state before handling
        {
            let node = self.raft_node.read().await;
            info!(self.logger, "[RAFT-CONF] Current Raft state before conf change";
                "state" => ?node.raft.state,
                "term" => node.raft.term,
                "commit" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied,
                "last_index" => node.raft.raft_log.last_index()
            );
            
            // Log current voters from stored conf state
            let voters: Vec<u64> = match self.storage.load_conf_state() {
                Ok(conf_state) => conf_state.voters,
                Err(e) => {
                    warn!(self.logger, "[RAFT-CONF] Failed to load conf state"; "error" => ?e);
                    Vec::new()
                }
            };
            info!(self.logger, "[RAFT-CONF] Current voters from conf state"; "voters" => ?voters);
        }
        
        let change_type = match conf_change.change_type {
            ConfChangeType::AddNode => RaftConfChangeType::AddNode,
            ConfChangeType::RemoveNode => RaftConfChangeType::RemoveNode,
        };
        
        // Create Raft ConfChange
        let mut cc = ConfChange::default();
        cc.set_change_type(change_type);
        cc.node_id = conf_change.node_id;
        
        // For add node, include the address in context
        let context = if matches!(conf_change.change_type, ConfChangeType::AddNode) {
            bincode::serialize(&conf_change.address)
                .map_err(|e| BlixardError::Serialization {
                    operation: "serialize node address".to_string(),
                    source: Box::new(e),
                })?
        } else {
            vec![]
        };
        cc.context = context.into();
        
        // Store the response channel if provided
        let cc_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        if let Some(response_tx) = conf_change.response_tx {
            info!(self.logger, "[RAFT-CONF] Storing response channel for conf change"; 
                "cc_id_len" => cc_id.len(),
                "cc_id_bytes" => ?&cc_id[..std::cmp::min(16, cc_id.len())]
            );
            let mut pending = self.pending_proposals.write().await;
            pending.insert(cc_id.clone(), response_tx);
            info!(self.logger, "[RAFT-CONF] Response channel stored successfully");
        }
        
        // Propose the configuration change
        let mut node = self.raft_node.write().await;
        
        // Check if we're leader
        let is_leader = node.raft.state == StateRole::Leader;
        if !is_leader {
            error!(self.logger, "[RAFT-CONF] Not the leader, cannot propose conf change"; "state" => ?node.raft.state);
            // Send error response if we have a channel
            if let Some(response_tx) = self.pending_proposals.write().await.remove(&cc_id) {
                let _ = response_tx.send(Err(BlixardError::ClusterError(
                    "Not the leader - cannot propose configuration changes".to_string()
                )));
            }
            return Err(BlixardError::ClusterError(
                "Not the leader - cannot propose configuration changes".to_string()
            ));
        }
        
        let is_single_node = self.peers.read().await.is_empty();
        info!(self.logger, "[RAFT-CONF] Node state"; "is_leader" => is_leader, "is_single_node" => is_single_node);
        
        info!(self.logger, "[RAFT-CONF] Proposing conf change to Raft"; "cc_id" => ?cc_id, "node_id" => conf_change.node_id, "change_type" => ?change_type);
        
        match node.propose_conf_change(cc_id.clone(), cc.clone()) {
            Ok(_) => {
                info!(self.logger, "[RAFT-CONF] Conf change proposed successfully"; "node_id" => conf_change.node_id);
            }
            Err(e) => {
                error!(self.logger, "[RAFT-CONF] Failed to propose conf change"; "error" => %e);
                let error_msg = format!("{}", e);
                // Send error response if we have a channel
                if let Some(response_tx) = self.pending_proposals.write().await.remove(&cc_id) {
                    let _ = response_tx.send(Err(BlixardError::Internal {
                        message: format!("Failed to propose conf change: {}", error_msg),
                    }));
                }
                return Err(BlixardError::Raft {
                    operation: "propose conf change".to_string(),
                    source: Box::new(e),
                });
            }
        }
        
        // For single-node clusters, check if we can apply the conf change immediately
        if is_leader && is_single_node {
            info!(self.logger, "[RAFT-CONF] Single-node cluster, checking if conf change can be applied");
            
            // Get the latest index
            let last_index = node.raft.raft_log.last_index();
            let committed = node.raft.raft_log.committed;
            info!(self.logger, "[RAFT-CONF] Raft log state"; "last_index" => last_index, "committed" => committed);
            
            drop(node); // Release lock for processing
            
            // Process ready states until the conf change is committed
            let mut retries = 0;
            loop {
                self.on_ready().await?;
                
                let node = self.raft_node.read().await;
                let new_committed = node.raft.raft_log.committed;
                info!(self.logger, "[RAFT-CONF] Checking commit progress"; "committed" => new_committed, "target" => last_index);
                
                if new_committed >= last_index {
                    info!(self.logger, "[RAFT-CONF] Conf change committed!");
                    break;
                }
                
                retries += 1;
                if retries > 10 {
                    warn!(self.logger, "[RAFT-CONF] Conf change not committed after 10 retries");
                    break;
                }
                
                drop(node);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            // Now manually apply the conf change and update state
            let mut node = self.raft_node.write().await;
            let cs = node.apply_conf_change(&cc)?;
            info!(self.logger, "[RAFT-CONF] Applied conf change"; "voters" => ?cs.voters);
            
            // Save the new configuration state to storage
            self.storage.save_conf_state(&cs)?;
            info!(self.logger, "[RAFT-CONF] Saved new configuration state to storage");
            
            // Update peers list
            if matches!(conf_change.change_type, ConfChangeType::AddNode) {
                if let Ok(address) = bincode::deserialize::<String>(&cc.context) {
                    let mut peers = self.peers.write().await;
                    peers.insert(conf_change.node_id, address.clone());
                    info!(self.logger, "[RAFT-CONF] Added peer to local list"; "node_id" => conf_change.node_id, "address" => &address);
                    
                    // Also update SharedNodeState peers
                    if let Some(shared) = self.shared_state.upgrade() {
                        let _ = shared.add_peer(conf_change.node_id, address.clone()).await;
                        info!(self.logger, "[RAFT-CONF] Added peer to SharedNodeState"; "node_id" => conf_change.node_id);
                        
                        // Proactively connect to the new peer
                        if let Some(peer_connector) = shared.get_peer_connector().await {
                            if let Some(peer_info) = shared.get_peer(conf_change.node_id).await {
                                tracing::info!("[RAFT-CONF] Proactively connecting to new peer {} at {}", 
                                    peer_info.id, peer_info.address);
                                self.connect_to_peer_with_p2p(peer_connector, &peer_info).await;
                            }
                        }
                    }
                    
                    // Set flag to trigger replication after adding node
                    *self.needs_replication_trigger.write().await = true;
                    info!(self.logger, "[RAFT-CONF] Set replication trigger flag for new node");
                    
                    // Give a small delay to allow peer connection to establish
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    info!(self.logger, "[RAFT-CONF] Waited for peer connection establishment");
                }
            }
            
            // Send immediate success response
            let mut pending = self.pending_proposals.write().await;
            if let Some(response_tx) = pending.remove(&cc_id) {
                let _ = response_tx.send(Ok(()));
                info!(self.logger, "[RAFT-CONF] Sent success response for conf change");
            }
            
            // IMPORTANT: Set flag to trigger replication after ready processing
            // This ensures the new node receives the configuration change log entry
            if matches!(conf_change.change_type, ConfChangeType::AddNode) {
                *self.needs_replication_trigger.write().await = true;
                info!(self.logger, "[RAFT-CONF] Set flag to trigger replication to new node");
            }
        }
        
        Ok(())
    }

    async fn on_ready(&self) -> BlixardResult<bool> {
        let mut node = self.raft_node.write().await;
        
        // Check if there's a ready state to process
        if !node.has_ready() {
            // No ready state - messages are now handled immediately after step()
            return Ok(false);
        }
        
        info!(self.logger, "[RAFT-READY] Processing ready state");
        
        let mut ready = node.ready();
        
        // Log ready state details
        // Debug messages in ready state
        for msg in ready.messages() {
            info!(self.logger, "[RAFT-READY] Has message in ready"; 
                "from" => msg.from, "to" => msg.to, "type" => ?msg.msg_type());
        }
        
        // Log the current state of the raft log BEFORE processing
        info!(self.logger, "[RAFT-READY] Raft log state BEFORE processing";
            "applied" => node.raft.raft_log.applied,
            "committed" => node.raft.raft_log.committed,
            "last_index" => node.raft.raft_log.last_index()
        );
        
        info!(self.logger, "[RAFT-READY] Ready state details";
            "messages" => ready.messages().len(),
            "committed_entries" => ready.committed_entries().len(),
            "entries" => ready.entries().len(),
            "has_snapshot" => !ready.snapshot().is_empty()
        );
        
        // Debug: Check if there are committed entries before processing
        if !ready.committed_entries().is_empty() {
            info!(self.logger, "[RAFT-READY] Found {} committed entries in ready state", ready.committed_entries().len());
            for (i, entry) in ready.committed_entries().iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Committed entry {}: index={}, type={:?}, context_len={}, data_len={}", 
                    i, entry.index, entry.entry_type(), entry.context.len(), entry.data.len());
            }
        } else {
            // If no committed entries but we're behind, something is wrong
            if node.raft.raft_log.committed > node.raft.raft_log.applied {
                warn!(self.logger, "[RAFT-READY] No committed entries but behind on applied!";
                    "committed" => node.raft.raft_log.committed,
                    "applied" => node.raft.raft_log.applied
                );
            }
        }
        
        // Apply snapshot if present
        if !ready.snapshot().is_empty() {
            info!(self.logger, "[RAFT-READY] Applying snapshot");
            let snapshot = ready.snapshot();
            
            // Apply the snapshot to storage
            self.storage.apply_snapshot(snapshot)?;
            
            // Apply the snapshot to the state machine
            let snapshot_data = snapshot.get_data();
            if !snapshot_data.is_empty() {
                self.state_machine.apply_snapshot(snapshot_data)?;
            }
            
            // The Raft library will handle updating its internal state
            // We just need to ensure our storage reflects the snapshot
            
            let metadata = snapshot.get_metadata();
            info!(self.logger, "[RAFT-READY] Snapshot applied"; 
                "index" => metadata.index, 
                "term" => metadata.term,
                "voters" => ?metadata.get_conf_state().voters
            );
        }
        
        // Save entries to storage
        if !ready.entries().is_empty() {
            info!(self.logger, "[RAFT-READY] Saving {} entries to storage", ready.entries().len());
            
            // Log detailed entry information
            for (i, entry) in ready.entries().iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Entry details";
                    "entry_num" => i,
                    "index" => entry.index,
                    "term" => entry.term,
                    "entry_type" => ?entry.entry_type(),
                    "context_len" => entry.context.len(),
                    "data_len" => entry.data.len()
                );
            }
            
            node.mut_store().append(&ready.entries())?;
            
            // In a single-node cluster, we might need to update the commit index immediately
            if self.peers.read().await.is_empty() && node.raft.state == StateRole::Leader {
                // Single node cluster - can commit immediately
                let last_index = ready.entries().last().map(|e| e.index).unwrap_or(0);
                info!(self.logger, "[RAFT-READY] Single node cluster, checking commit"; "last_index" => last_index);
            }
        }
        
        // Save hard state if it changed
        if let Some(hs) = ready.hs() {
            info!(self.logger, "[RAFT-READY] Saving hard state"; "term" => hs.term, "vote" => hs.vote, "commit" => hs.commit);
            node.mut_store().save_hard_state(&hs)?;
        }
        
        // Send messages to peers
        if !ready.messages().is_empty() {
            info!(self.logger, "[RAFT-READY] Sending {} messages", ready.messages().len());
            for msg in ready.take_messages() {
                // Add more detailed logging for AppendEntries
                if msg.msg_type() == raft::prelude::MessageType::MsgAppend {
                    info!(self.logger, "[RAFT-MSG] Sending MsgAppend";
                        "to" => msg.to,
                        "from" => msg.from,
                        "index" => msg.index,
                        "log_term" => msg.log_term,
                        "entries_count" => msg.entries.len(),
                        "commit" => msg.commit
                    );
                }
                self.send_raft_message(msg).await?;
            }
        }
        
        // Apply committed entries to state machine
        let committed_entries = ready.take_committed_entries();
        info!(self.logger, "[RAFT-READY] Committed entries count: {}", committed_entries.len());
        
        // IMPORTANT: Check if we're behind on applying entries
        // This can happen when entries were committed in previous ready cycles
        // but we haven't processed them yet (e.g., during bootstrap)
        if committed_entries.is_empty() && node.raft.raft_log.committed > node.raft.raft_log.applied {
            warn!(self.logger, "[RAFT-READY] No committed entries but we're behind on applying";
                "committed" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
            // This is a bug - we should have gotten these entries in committed_entries!
        }
        
        let mut last_applied_index = 0u64;
        if !committed_entries.is_empty() {
            info!(self.logger, "[RAFT-READY] Processing {} committed entries", committed_entries.len());
            let pending_proposals = Arc::clone(&self.pending_proposals);
            info!(self.logger, "[RAFT-READY] Processing committed entries");
            for (idx, entry) in committed_entries.iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Processing committed entry";
                    "entry_num" => idx,
                    "index" => entry.index,
                    "term" => entry.term,
                    "type" => ?entry.entry_type(),
                    "context_len" => entry.context.len(),
                    "data_len" => entry.data.len(),
                    "has_context" => !entry.context.is_empty(),
                    "has_data" => !entry.data.is_empty()
                );
                
                last_applied_index = entry.index;
                use raft::prelude::EntryType;
                
                match entry.entry_type() {
                    EntryType::EntryNormal => {
                        info!(self.logger, "[RAFT-READY] Processing normal entry"; 
                            "index" => entry.index, 
                            "context_len" => entry.context.len(),
                            "has_data" => !entry.data.is_empty(),
                            "is_empty" => entry.data.is_empty() && entry.context.is_empty()
                        );
                        
                        // Apply normal entry to state machine
                        let result = self.state_machine.apply_entry(&entry).await;
                        
                        // Log the result
                        match &result {
                            Ok(_) => info!(self.logger, "[RAFT-READY] Entry applied successfully"; "index" => entry.index),
                            Err(e) => warn!(self.logger, "[RAFT-READY] Failed to apply entry"; "index" => entry.index, "error" => %e),
                        }
                        
                        // Send response to waiting proposal if any
                        if !entry.context.is_empty() {
                            let pending_count = pending_proposals.read().await.len();
                            info!(self.logger, "[RAFT-READY] Looking for pending proposal"; 
                                "context_len" => entry.context.len(),
                                "pending_count" => pending_count
                            );
                            
                            // Log all pending proposal IDs for debugging
                            {
                                let pending_read = pending_proposals.read().await;
                                info!(self.logger, "[RAFT-READY] Checking pending proposals");
                                for (id, _) in pending_read.iter() {
                                    info!(self.logger, "[RAFT-READY] Pending proposal"; 
                                        "id_len" => id.len(),
                                        "matches_context" => (id == &entry.context)
                                    );
                                }
                            }
                            
                            let mut pending = pending_proposals.write().await;
                            if let Some(response_tx) = pending.remove(&entry.context) {
                                info!(self.logger, "[RAFT-READY] Found and sending proposal response"; "success" => result.is_ok());
                                if let Err(_) = response_tx.send(result) {
                                    warn!(self.logger, "[RAFT-READY] Failed to send proposal response - receiver dropped");
                                }
                            } else {
                                warn!(self.logger, "[RAFT-READY] No pending proposal found for context"; 
                                    "context_len" => entry.context.len()
                                );
                            }
                        } else {
                            info!(self.logger, "[RAFT-READY] Entry has no context, no response to send");
                        }
                    }
                    EntryType::EntryConfChange => {
                        info!(self.logger, "[RAFT-CONF] Processing EntryConfChange entry"; 
                            "entry_index" => entry.index,
                            "entry_context_len" => entry.context.len(),
                            "entry_data_len" => entry.data.len()
                        );
                        
                        // Handle configuration change
                        // The entry data contains the serialized ConfChange
                        use protobuf::Message as ProtobufMessage;
                        let mut cc = ConfChange::default();
                        if let Err(e) = cc.merge_from_bytes(&entry.data) {
                            error!(self.logger, "[RAFT-CONF] Failed to parse ConfChange from entry data"; 
                                "error" => %e,
                                "data_len" => entry.data.len()
                            );
                            // Send error response if we have a pending proposal
                            if !entry.context.is_empty() {
                                let mut pending = self.pending_proposals.write().await;
                                if let Some(response_tx) = pending.remove(&entry.context) {
                                    let _ = response_tx.send(Err(BlixardError::Internal {
                                        message: format!("Failed to parse ConfChange: {:?}", e),
                                    }));
                                }
                            }
                        } else {
                            // Always apply configuration changes from committed entries
                            // The Raft library handles deduplication internally
                            
                            info!(self.logger, "[RAFT-CONF] Successfully parsed ConfChange";
                                "node_id" => cc.node_id,
                                "change_type" => cc.change_type,
                                "context_len" => cc.context.len()
                            );
                            
                            // Log the current voters before applying the change
                            let current_voters: Vec<u64> = match self.storage.load_conf_state() {
                                Ok(conf_state) => conf_state.voters,
                                Err(e) => {
                                    warn!(self.logger, "[RAFT-CONF] Failed to load conf state"; "error" => ?e);
                                    Vec::new()
                                }
                            };
                            info!(self.logger, "[RAFT-CONF] Current voters before conf change from conf state"; "voters" => ?current_voters);
                            
                            // Additional logging before applying conf change
                            info!(self.logger, "[RAFT-CONF] About to apply configuration change";
                                "self_node_id" => self.node_id,
                                "change_type" => ?cc.change_type(),
                                "target_node_id" => cc.node_id,
                                "current_voters_from_storage" => ?current_voters,
                                "entry_index" => entry.index
                            );
                            
                            // Get Raft's view of voters before applying
                            let raft_voters_before = node.raft.prs().conf().voters().clone();
                            info!(self.logger, "[RAFT-CONF] Raft's view of voters before apply_conf_change";
                                "voters" => ?raft_voters_before
                            );
                            
                            // CRITICAL: If Raft's view of voters is empty or incomplete, we need to handle it carefully
                            // This can happen when a node is still catching up with configuration changes
                            if cc.change_type() == raft::prelude::ConfChangeType::RemoveNode {
                                // Check if Raft's configuration is empty by looking at the debug output
                                let raft_conf_debug = format!("{:?}", node.raft.prs().conf().voters());
                                let raft_has_empty_voters = raft_conf_debug.contains("voters: {}");
                                
                                // For non-leaders with empty Raft configuration, we should trust the log entry
                                // and update our configuration state without going through apply_conf_change
                                if raft_has_empty_voters && !current_voters.is_empty() && node.raft.state != raft::StateRole::Leader {
                                    warn!(self.logger, "[RAFT-CONF] Non-leader with empty Raft voter set - updating conf state from log entry";
                                        "storage_voters" => ?current_voters,
                                        "attempting_to_remove" => cc.node_id,
                                        "self_node_id" => self.node_id
                                    );
                                    
                                    // Calculate the new configuration based on the log entry
                                    let mut new_voters = current_voters.clone();
                                    new_voters.retain(|&id| id != cc.node_id);
                                    
                                    if !new_voters.is_empty() {
                                        let mut new_cs = ConfState::default();
                                        new_cs.voters = new_voters.clone();
                                        
                                        // Save this configuration state
                                        self.storage.save_conf_state(&new_cs)?;
                                        info!(self.logger, "[RAFT-CONF] Non-leader updated conf state from log entry";
                                            "voters" => ?new_cs.voters,
                                            "removed_node" => cc.node_id
                                        );
                                        
                                        // Skip the apply_conf_change call since we've already updated the state
                                        continue;
                                    }
                                }
                            }
                            
                            info!(self.logger, "[RAFT-CONF] Applying committed configuration change"; "node_id" => cc.node_id, "type" => cc.change_type, "entry_index" => entry.index);
                            let cs = match node.apply_conf_change(&cc) {
                                Ok(cs) => {
                                    info!(self.logger, "[RAFT-CONF] Successfully applied conf change");
                                    cs
                                }
                                Err(e) => {
                                    let error_msg = format!("{}", e);
                                    
                                    // Special handling for "removed all voters" error on non-leaders
                                    if error_msg.contains("removed all voters") && node.raft.state != raft::StateRole::Leader {
                                        warn!(self.logger, "[RAFT-CONF] Non-leader got 'removed all voters' error - this is expected when node hasn't caught up";
                                            "error" => %e,
                                            "self_node_id" => self.node_id,
                                            "removing_node_id" => cc.node_id
                                        );
                                        
                                        // We should have already handled this case before apply_conf_change
                                        // If we still got here, it means something unexpected happened
                                        // Don't send error response for non-leaders - this is expected
                                        continue;
                                    }
                                    
                                    error!(self.logger, "[RAFT-CONF] Failed to apply conf change"; "error" => %e);
                                    
                                    // Log more details about the failure
                                    let raft_voters_after_error = node.raft.prs().conf().voters().clone();
                                    error!(self.logger, "[RAFT-CONF] Configuration state at error";
                                        "raft_voters" => ?raft_voters_after_error,
                                        "storage_voters" => ?current_voters,
                                        "self_node_id" => self.node_id,
                                        "removing_node_id" => cc.node_id
                                    );
                                    
                                    // Send error response if we have a pending proposal
                                    if !entry.context.is_empty() {
                                        let mut pending = self.pending_proposals.write().await;
                                        if let Some(response_tx) = pending.remove(&entry.context) {
                                            let _ = response_tx.send(Err(BlixardError::Raft {
                                                operation: "apply conf change".to_string(),
                                                source: Box::new(e),
                                            }));
                                        }
                                    }
                                    continue;
                                }
                            };
                            
                            // Log what the returned ConfState contains
                            info!(self.logger, "[RAFT-CONF] apply_conf_change returned ConfState"; 
                                "cs.voters" => ?cs.voters, 
                                "cs.learners" => ?cs.learners,
                                "cs.voters_outgoing" => ?cs.voters_outgoing,
                                "cs.learners_next" => ?cs.learners_next,
                                "cs.auto_leave" => cs.auto_leave
                            );
                            
                            // Handle the configuration state based on the change type
                            let final_cs = match cc.change_type() {
                                raft::prelude::ConfChangeType::AddNode => {
                                    info!(self.logger, "[RAFT-CONF] Processing AddNode configuration";
                                        "node_id" => self.node_id,
                                        "returned_voters" => ?cs.voters,
                                        "added_node" => cc.node_id
                                    );
                                    
                                    // For joining nodes that already have a saved configuration from the join response,
                                    // trust that configuration rather than trying to reconstruct
                                    if self.node_id == cc.node_id {
                                        // This is the joining node processing its own AddNode
                                        if let Ok(saved_conf) = self.storage.load_conf_state() {
                                            if !saved_conf.voters.is_empty() {
                                                info!(self.logger, "[RAFT-CONF] Joining node using saved configuration from join response";
                                                    "saved_voters" => ?saved_conf.voters,
                                                    "raft_returned_voters" => ?cs.voters
                                                );
                                                // Use the saved configuration which should be complete
                                                let mut final_conf = saved_conf.clone();
                                                // Ensure it includes self
                                                if !final_conf.voters.contains(&self.node_id) {
                                                    final_conf.voters.push(self.node_id);
                                                    final_conf.voters.sort();
                                                }
                                                final_conf
                                            } else {
                                                // No saved configuration, use what Raft returned
                                                cs
                                            }
                                        } else {
                                            // Failed to load saved configuration, use what Raft returned
                                            cs
                                        }
                                    } else {
                                        // For other nodes processing AddNode of a different node,
                                        // we need to be careful about trusting Raft's returned configuration
                                        // because it may not have the complete history
                                        
                                        // For non-leaders, always merge with existing configuration
                                        if node.raft.state != raft::StateRole::Leader {
                                            info!(self.logger, "[RAFT-CONF] Non-leader processing AddNode, merging with stored configuration";
                                                "new_node" => cc.node_id,
                                                "raft_returned" => ?cs.voters,
                                                "is_leader" => false
                                            );
                                            
                                            if let Ok(existing_conf) = self.storage.load_conf_state() {
                                                let mut merged_conf = cs.clone();
                                                merged_conf.voters = existing_conf.voters.clone();
                                                if !merged_conf.voters.contains(&cc.node_id) {
                                                    merged_conf.voters.push(cc.node_id);
                                                    merged_conf.voters.sort();
                                                }
                                                info!(self.logger, "[RAFT-CONF] Merged configuration for non-leader";
                                                    "merged_voters" => ?merged_conf.voters
                                                );
                                                merged_conf
                                            } else {
                                                // Can't load existing configuration, use what Raft returned
                                                error!(self.logger, "[RAFT-CONF] Failed to load existing configuration for merge");
                                                cs
                                            }
                                        } else {
                                            // Leader should have complete configuration
                                            info!(self.logger, "[RAFT-CONF] Leader using Raft's returned configuration";
                                                "voters" => ?cs.voters
                                            );
                                            cs
                                        }
                                    }
                                }
                                raft::prelude::ConfChangeType::RemoveNode => {
                                    // For RemoveNode, validate that we're not removing all voters
                                    if cs.voters.is_empty() {
                                        error!(self.logger, "[RAFT-CONF] RemoveNode resulted in empty voters list - this should not happen";
                                            "removed_node" => cc.node_id,
                                            "previous_voters" => ?current_voters
                                        );
                                        // Try to recover by using the previous configuration without the removed node
                                        if let Ok(prev_conf) = self.storage.load_conf_state() {
                                            let mut recovered_voters = prev_conf.voters;
                                            recovered_voters.retain(|&id| id != cc.node_id);
                                            if !recovered_voters.is_empty() {
                                                let mut recovered_cs = cs.clone();
                                                recovered_cs.voters = recovered_voters;
                                                warn!(self.logger, "[RAFT-CONF] Recovered voters list from previous state";
                                                    "recovered_voters" => ?recovered_cs.voters
                                                );
                                                recovered_cs
                                            } else {
                                                // This is a critical error - we would have no voters
                                                error!(self.logger, "[RAFT-CONF] Cannot recover - would result in no voters");
                                                cs
                                            }
                                        } else {
                                            cs
                                        }
                                    } else {
                                        // Normal case - use the ConfState returned by Raft
                                        cs
                                    }
                                }
                                _ => cs // For other types, use as-is
                            };
                            
                            // Save the final configuration state to storage
                            self.storage.save_conf_state(&final_cs)?;
                            info!(self.logger, "[RAFT-CONF] Saved new configuration state to storage"; "voters" => ?final_cs.voters);
                            
                            // Verify the actual voters after applying the change from conf state
                            let new_voters: Vec<u64> = final_cs.voters.clone();
                            info!(self.logger, "[RAFT-CONF] Actual voters after conf change from conf state"; "voters" => ?new_voters);
                            
                            // Update peer list based on the change type
                            use raft::prelude::ConfChangeType as RaftConfChangeType;
                            match cc.change_type() {
                                RaftConfChangeType::AddNode => {
                                    // Deserialize address from context
                                    if !cc.context.is_empty() {
                                        if let Ok(address) = bincode::deserialize::<String>(&cc.context) {
                                            let mut peers = self.peers.write().await;
                                            peers.insert(cc.node_id, address.clone());
                                            info!(self.logger, "[RAFT-CONF] Added peer to local list"; "node_id" => cc.node_id);
                                            
                                            // Also update SharedNodeState peers
                                            if let Some(shared) = self.shared_state.upgrade() {
                                                // Check if peer already exists to avoid duplicate add
                                                if shared.get_peer(cc.node_id).await.is_none() {
                                                    let _ = shared.add_peer(cc.node_id, address.clone()).await;
                                                    info!(self.logger, "[RAFT-CONF] Added peer to SharedNodeState"; "node_id" => cc.node_id);
                                                } else {
                                                    info!(self.logger, "[RAFT-CONF] Peer already exists in SharedNodeState"; "node_id" => cc.node_id);
                                                }
                                                
                                                // CRITICAL FIX: Ensure all existing peers know about each other
                                                // When a new node joins, we need to share peer information
                                                if self.node_id == 1 || node.raft.state == raft::StateRole::Leader {
                                                    // If we're the leader or bootstrap node, we should have all peer info
                                                    let all_peers = shared.get_peers().await;
                                                    info!(self.logger, "[RAFT-CONF] Ensuring peer information consistency";
                                                        "total_peers" => all_peers.len(),
                                                        "new_node" => cc.node_id
                                                    );
                                                    
                                                    // Make sure all peers are in our local list and SharedNodeState
                                                    for peer in all_peers {
                                                        if peer.id != self.node_id && peer.id != cc.node_id {
                                                            // This peer should also know about the new node
                                                            // The peer will learn about it when processing this same log entry
                                                            info!(self.logger, "[RAFT-CONF] Peer {} will learn about new node {} from log",
                                                                peer.id, cc.node_id);
                                                        }
                                                    }
                                                }
                                                
                                                // Proactively connect to the new peer
                                                if let Some(peer_connector) = shared.get_peer_connector().await {
                                                    if let Some(peer_info) = shared.get_peer(cc.node_id).await {
                                                        tracing::info!("[RAFT-CONF] Proactively connecting to new peer {} at {}", 
                                                            peer_info.id, peer_info.address);
                                                        self.connect_to_peer_with_p2p(peer_connector, &peer_info).await;
                                                    }
                                                }
                                            }
                                        } else {
                                            error!(self.logger, "[RAFT-CONF] Failed to deserialize peer address from context";
                                                "node_id" => cc.node_id,
                                                "context_len" => cc.context.len()
                                            );
                                        }
                                    } else {
                                        error!(self.logger, "[RAFT-CONF] No address in conf change context for AddNode";
                                            "node_id" => cc.node_id
                                        );
                                    }
                                    
                                    // Set flag to trigger replication after adding node
                                    *self.needs_replication_trigger.write().await = true;
                                    info!(self.logger, "[RAFT-CONF] Set replication trigger flag for new node");
                                    
                                    // Give a small delay to allow peer connection to establish
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                    info!(self.logger, "[RAFT-CONF] Waited for peer connection establishment");
                                    
                                    // Check if the new node needs a snapshot
                                    // New nodes often need snapshots to get the current configuration
                                    if let Some(progress) = node.raft.prs().get(cc.node_id) {
                                        let last_index = node.raft.raft_log.last_index();
                                        info!(self.logger, "[RAFT-CONF] New node progress check";
                                            "node_id" => cc.node_id,
                                            "matched" => progress.matched,
                                            "last_index" => last_index
                                        );
                                        
                                        // If the node is behind, log it but don't send append here
                                        // The snapshot will be sent in the next ready cycle
                                        if progress.matched == 0 && last_index > 0 {
                                            info!(self.logger, "[RAFT-CONF] New node needs snapshot, will be sent in next cycle";
                                                "node_id" => cc.node_id
                                            );
                                            // Don't call send_append here - it generates messages during ready processing
                                        }
                                    }
                                }
                                RaftConfChangeType::RemoveNode => {
                                    let mut peers = self.peers.write().await;
                                    peers.remove(&cc.node_id);
                                    info!(self.logger, "[RAFT-CONF] Removed peer from local list"; "node_id" => cc.node_id);
                                    
                                    // Remove worker from the worker registry
                                    match self.storage.database.begin_write() {
                                        Ok(write_txn) => {
                                            let node_id_bytes = cc.node_id.to_le_bytes();
                                            {
                                                // Scope for table access
                                                if let Ok(mut worker_table) = write_txn.open_table(crate::storage::WORKER_TABLE) {
                                                    let _ = worker_table.remove(node_id_bytes.as_slice());
                                                }
                                                if let Ok(mut status_table) = write_txn.open_table(crate::storage::WORKER_STATUS_TABLE) {
                                                    let _ = status_table.remove(node_id_bytes.as_slice());
                                                }
                                            } // Tables are dropped here
                                            
                                            if let Ok(_) = write_txn.commit() {
                                                info!(self.logger, "[RAFT-CONF] Removed worker from registry"; "node_id" => cc.node_id);
                                            } else {
                                                warn!(self.logger, "[RAFT-CONF] Failed to commit worker removal"; "node_id" => cc.node_id);
                                            }
                                        }
                                        Err(e) => {
                                            warn!(self.logger, "[RAFT-CONF] Failed to begin write transaction for worker removal"; "error" => %e);
                                        }
                                    }
                                    
                                    // Also update SharedNodeState peers
                                    if let Some(shared) = self.shared_state.upgrade() {
                                        let _ = shared.remove_peer(cc.node_id).await;
                                        info!(self.logger, "[RAFT-CONF] Removed peer from SharedNodeState"; "node_id" => cc.node_id);
                                    }
                                }
                                _ => {}
                            }
                            
                            // Send response to waiting conf change if any
                            if !entry.context.is_empty() {
                                info!(self.logger, "[RAFT-CONF] Looking for pending conf change response channel";
                                    "context_len" => entry.context.len(),
                                    "context_bytes" => ?&entry.context[..std::cmp::min(16, entry.context.len())]
                                );
                                
                                // Log all pending proposals for debugging
                                {
                                    let pending_read = pending_proposals.read().await;
                                    info!(self.logger, "[RAFT-CONF] Current pending proposals";
                                        "count" => pending_read.len()
                                    );
                                    for (id, _) in pending_read.iter() {
                                        info!(self.logger, "[RAFT-CONF] Pending proposal";
                                            "id_bytes" => ?&id[..std::cmp::min(16, id.len())],
                                            "matches" => (id == &entry.context)
                                        );
                                    }
                                }
                                
                                let mut pending = pending_proposals.write().await;
                                if let Some(response_tx) = pending.remove(&entry.context) {
                                    info!(self.logger, "[RAFT-CONF] Found pending conf change, sending success response");
                                    if let Err(_) = response_tx.send(Ok(())) {
                                        warn!(self.logger, "[RAFT-CONF] Failed to send conf change response - receiver dropped");
                                    } else {
                                        info!(self.logger, "[RAFT-CONF] Successfully sent conf change response");
                                    }
                                } else {
                                    warn!(self.logger, "[RAFT-CONF] No pending conf change found for context";
                                        "context_len" => entry.context.len(),
                                        "pending_count" => pending.len()
                                    );
                                    
                                    // For RemoveNode, we might not have a pending response channel
                                    // This is OK if the remove was initiated from a different source
                                    if cc.change_type() == RaftConfChangeType::RemoveNode {
                                        info!(self.logger, "[RAFT-CONF] RemoveNode completed without pending response (likely from different source)");
                                    }
                                }
                            } else {
                                info!(self.logger, "[RAFT-CONF] Entry has no context, no response to send");
                            }
                            
                            // IMPORTANT: After adding a new node, we need to trigger replication
                            // Set flag to trigger after ready state is processed
                            if cc.change_type() == RaftConfChangeType::AddNode {
                                *self.needs_replication_trigger.write().await = true;
                                info!(self.logger, "[RAFT-CONF] Set flag to trigger replication after ready state");
                            }
                        }
                    }
                    EntryType::EntryConfChangeV2 => {
                        // We don't use V2 conf changes yet
                        warn!(self.logger, "Received EntryConfChangeV2, not implemented");
                    }
                }
            }
        }
        
        // Advance the Raft node
        info!(self.logger, "[RAFT-READY] About to call advance on ready state";
            "applied_before" => node.raft.raft_log.applied,
            "committed_before" => node.raft.raft_log.committed
        );
        let mut light_rd = node.advance(ready);
        info!(self.logger, "[RAFT-READY] Advanced ready state";
            "applied_after" => node.raft.raft_log.applied,
            "committed_after" => node.raft.raft_log.committed
        );
        
        // Handle messages from LightReady
        if !light_rd.messages().is_empty() {
            info!(self.logger, "[RAFT-READY] Sending {} messages from LightReady", light_rd.messages().len());
            for msg in light_rd.take_messages() {
                self.send_raft_message(msg).await?;
            }
        }
        
        // Update commit index if needed
        let had_commit_update = if let Some(commit) = light_rd.commit_index() {
            info!(self.logger, "[RAFT-READY] Light ready has commit index"; "commit" => commit);
            // Store commit index update
            let mut hs = node.raft.hard_state();
            hs.set_commit(commit);
            node.mut_store().save_hard_state(&hs)?;
            true
        } else {
            false
        };
        
        // CRITICAL: Check if new entries were committed during this ready cycle
        // This happens in single-node clusters where commits happen immediately
        let current_applied = if last_applied_index > 0 { last_applied_index } else { node.raft.raft_log.applied };
        if had_commit_update && node.raft.raft_log.committed > current_applied {
            // We have newly committed entries that weren't in ready.committed_entries()
            // Fetch and process them now
            let start = current_applied + 1;
            let end = node.raft.raft_log.committed + 1;
            
            info!(self.logger, "[RAFT-READY] Fetching newly committed entries after light ready";
                "start" => start,
                "end" => end,
                "last_applied" => last_applied_index,
                "committed" => node.raft.raft_log.committed
            );
            
            if let Ok(entries) = node.mut_store().entries(start, end, None, GetEntriesContext::empty(false)) {
                info!(self.logger, "[RAFT-READY] Processing {} newly committed entries", entries.len());
                
                // Process these entries
                for (idx, entry) in entries.iter().enumerate() {
                    info!(self.logger, "[RAFT-READY] Processing newly committed entry";
                        "entry_num" => idx,
                        "index" => entry.index,
                        "term" => entry.term,
                        "type" => ?entry.entry_type(),
                        "context_len" => entry.context.len(),
                        "data_len" => entry.data.len()
                    );
                    
                    last_applied_index = entry.index;
                    
                    match entry.entry_type() {
                        EntryType::EntryNormal => {
                            info!(self.logger, "[RAFT-READY] Processing normal entry"; 
                                "index" => entry.index, 
                                "context_len" => entry.context.len(),
                                "has_data" => !entry.data.is_empty()
                            );
                            
                            // Apply normal entry to state machine
                            let result = self.state_machine.apply_entry(&entry).await;
                            
                            // Log the result
                            match &result {
                                Ok(_) => info!(self.logger, "[RAFT-READY] Entry applied successfully"; "index" => entry.index),
                                Err(e) => warn!(self.logger, "[RAFT-READY] Failed to apply entry"; "index" => entry.index, "error" => %e),
                            }
                            
                            // Send response to waiting proposal if any
                            if !entry.context.is_empty() {
                                let pending_count = self.pending_proposals.read().await.len();
                                info!(self.logger, "[RAFT-READY] Looking for pending proposal"; 
                                    "context_len" => entry.context.len(),
                                    "pending_count" => pending_count
                                );
                                
                                let mut pending = self.pending_proposals.write().await;
                                if let Some(response_tx) = pending.remove(&entry.context) {
                                    info!(self.logger, "[RAFT-READY] Found and sending proposal response"; "success" => result.is_ok());
                                    if let Err(_) = response_tx.send(result) {
                                        warn!(self.logger, "[RAFT-READY] Failed to send proposal response - receiver dropped");
                                    }
                                } else {
                                    warn!(self.logger, "[RAFT-READY] No pending proposal found for context"; 
                                        "context_len" => entry.context.len()
                                    );
                                }
                            }
                        }
                        _ => {
                            // Handle other entry types if needed
                            warn!(self.logger, "[RAFT-READY] Skipping non-normal entry type"; "type" => ?entry.entry_type());
                        }
                    }
                }
            }
        }
        
        // CRITICAL: Tell Raft we've applied the committed entries
        // This is required for Raft to return new committed entries in future ready states
        if last_applied_index > 0 {
            node.advance_apply_to(last_applied_index);
            info!(self.logger, "[RAFT-READY] Advanced applied index to {}", last_applied_index);
            
            // Check if we should trigger log compaction
            // Compact when we have more than 1000 applied entries
            const COMPACTION_THRESHOLD: u64 = 1000;
            if last_applied_index > COMPACTION_THRESHOLD {
                // Check if it's time to compact (don't compact too frequently)
                let last_compact = *self.last_compaction_index.read().await;
                let should_compact = match last_compact {
                    Some(last_compact_idx) => last_applied_index - last_compact_idx > COMPACTION_THRESHOLD / 2,
                    None => true,
                };
                
                if should_compact {
                    info!(self.logger, "[RAFT-COMPACT] Triggering log compaction";
                        "applied_index" => last_applied_index,
                        "last_compaction" => ?last_compact
                    );
                    
                    // Create a snapshot
                    match node.mut_store().snapshot(last_applied_index, 0) {
                        Ok(snapshot) => {
                            let compact_index = snapshot.get_metadata().index;
                            
                            // Compact the log
                            if let Err(e) = self.storage.compact_log_before(compact_index) {
                                warn!(self.logger, "[RAFT-COMPACT] Failed to compact log"; "error" => %e);
                            } else {
                                info!(self.logger, "[RAFT-COMPACT] Successfully compacted log before index {}", compact_index);
                                *self.last_compaction_index.write().await = Some(compact_index);
                            }
                        }
                        Err(e) => {
                            warn!(self.logger, "[RAFT-COMPACT] Failed to create snapshot for compaction"; "error" => %e);
                        }
                    }
                }
            }
        }
        
        // Check the current state after processing
        info!(self.logger, "[RAFT-READY] After processing - raft state"; 
            "term" => node.raft.term,
            "commit" => node.raft.raft_log.committed,
            "applied" => node.raft.raft_log.applied,
            "last_index" => node.raft.raft_log.last_index()
        );
        
        // Update SharedNodeState with current Raft status
        if let Some(shared) = self.shared_state.upgrade() {
            let is_leader = node.raft.state == StateRole::Leader;
            let leader_id = if node.raft.leader_id == 0 {
                None
            } else {
                Some(node.raft.leader_id)
            };
            
            let status = crate::node_shared::RaftStatus {
                is_leader,
                node_id: self.node_id,
                leader_id,
                term: node.raft.term,
                state: format!("{:?}", node.raft.state).to_lowercase(),
            };
            
            shared.update_raft_status(status).await;
        }
        
        // IMPORTANT: Do NOT call any methods that might generate messages after advance
        // but before the next ready cycle. This includes check_and_send_snapshots which
        // calls send_append and might generate messages on non-leader nodes.
        
        // Check if there's another ready state immediately available
        let has_more_ready = node.has_ready();
        if has_more_ready {
            info!(self.logger, "[RAFT-READY] Another ready state is immediately available");
        } else if had_commit_update {
            // If we had a commit update but no ready state, we might need to
            // trigger another cycle to process the newly committed entries
            info!(self.logger, "[RAFT-READY] Had commit update but no immediate ready state";
                "committed" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
        }
        
        // Return true to indicate we processed a ready state
        Ok(true)
    }

    async fn send_raft_message(&self, msg: raft::prelude::Message) -> BlixardResult<()> {
        let to = msg.to;
        let from = msg.from;
        let msg_type = msg.msg_type();
        info!(self.logger, "[RAFT-MSG] Sending message"; "from" => from, "to" => to, "type" => ?msg_type);
        
        // Log snapshot messages being sent
        if msg_type == raft::prelude::MessageType::MsgSnapshot {
            let snapshot = msg.get_snapshot();
            let metadata = snapshot.get_metadata();
            info!(self.logger, "[RAFT-MSG] Sending MsgSnapshot";
                "to" => to,
                "snapshot_index" => metadata.index,
                "snapshot_term" => metadata.term,
                "snapshot_voters" => ?metadata.get_conf_state().voters,
                "data_size" => snapshot.data.len()
            );
        }
        
        self.outgoing_messages.send((to, msg))
            .map_err(|_| BlixardError::Internal {
                message: "Failed to send outgoing Raft message".to_string(),
            })?;
        Ok(())
    }
    
    /// Helper function to connect to a peer using P2P info
    async fn connect_to_peer_with_p2p(&self, peer_connector: Arc<crate::transport::iroh_peer_connector::IrohPeerConnector>, peer_info: &crate::node_shared::PeerInfo) {
        // Try to create NodeAddr from PeerInfo
        if let Some(p2p_node_id) = &peer_info.p2p_node_id {
            // Parse the base64 encoded node ID
            if let Ok(node_id_bytes) = general_purpose::STANDARD.decode(p2p_node_id) {
                if node_id_bytes.len() == 32 {
                    let mut bytes = [0u8; 32];
                    bytes.copy_from_slice(&node_id_bytes);
                    if let Ok(node_id) = iroh::NodeId::from_bytes(&bytes) {
                        let mut node_addr = iroh::NodeAddr::new(node_id);
                    
                        // Add direct addresses
                        for addr_str in &peer_info.p2p_addresses {
                            if let Ok(addr) = addr_str.parse() {
                                node_addr = node_addr.with_direct_addresses([addr]);
                            }
                        }
                    
                        // Add relay URL if available
                        if let Some(relay_url) = &peer_info.p2p_relay_url {
                            if let Ok(url) = relay_url.parse() {
                                node_addr = node_addr.with_relay_url(url);
                            }
                        }
                    
                        let peer_id = peer_info.id;
                        let peer_address = peer_info.address.clone();
                        tokio::spawn(async move {
                            if let Err(e) = peer_connector.connect_to_peer(peer_id, node_addr).await {
                                tracing::warn!("[RAFT-MSG] Failed to connect to peer {} at {}: {}", peer_id, peer_address, e);
                            }
                        });
                    }
                }
            }
        }
    }
    
    /// Check if any followers need snapshots and trigger sending
    async fn check_and_send_snapshots(&self, node: &mut RawNode<RedbRaftStorage>) -> BlixardResult<()> {
        let last_index = node.raft.raft_log.last_index();
        
        // Check each follower's progress
        let followers_needing_snapshot: Vec<u64> = node.raft.prs().iter()
            .filter_map(|(id, pr)| {
                if *id == self.node_id {
                    return None; // Skip self
                }
                
                // For new nodes (matched = 0) or nodes that are far behind
                // Send snapshot if:
                // 1. Node has never received any entries (matched = 0)
                // 2. Node is more than 5 entries behind and has committed configuration changes
                let needs_snapshot = if pr.matched == 0 && last_index > 0 {
                    // New node that has never synced
                    true
                } else {
                    // Check if node is too far behind
                    let lag = last_index.saturating_sub(pr.matched);
                    lag > 5 && self.storage.last_index().unwrap_or(0) > 3
                };
                
                if needs_snapshot {
                    info!(self.logger, "[RAFT-SNAPSHOT] Follower needs snapshot";
                        "follower_id" => id,
                        "matched" => pr.matched,
                        "last_index" => last_index,
                        "lag" => last_index.saturating_sub(pr.matched)
                    );
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        
        // Send snapshots to lagging followers
        for follower_id in followers_needing_snapshot {
            info!(self.logger, "[RAFT-SNAPSHOT] Triggering snapshot send to follower"; "follower_id" => follower_id);
            // The Raft library will handle creating and sending the snapshot message
            // when we try to replicate to this follower
            node.raft.send_append(follower_id);
        }
        
        Ok(())
    }

    pub async fn is_leader(&self) -> bool {
        let node = self.raft_node.read().await;
        node.raft.state == StateRole::Leader
    }
    
    pub fn get_node_id(&self) -> u64 {
        self.node_id
    }

    pub async fn get_leader_id(&self) -> Option<u64> {
        let node = self.raft_node.read().await;
        Some(node.raft.leader_id)
    }
    
    /// Report snapshot status after sending or receiving
    pub async fn report_snapshot(&self, to: u64, status: raft::SnapshotStatus) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        node.report_snapshot(to, status);
        
        info!(self.logger, "[RAFT-SNAPSHOT] Reported snapshot status";
            "to" => to,
            "status" => ?status
        );
        
        Ok(())
    }

    pub async fn propose(&self, data: ProposalData) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("raft::propose");
        
        let (tx, rx) = oneshot::channel();
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data,
            response_tx: Some(tx),
        };
        
        self.proposal_tx.send(proposal)
            .map_err(|_| BlixardError::Internal {
                message: "Failed to send proposal".to_string(),
            })?;
        
        // Wait for response
        rx.await
            .map_err(|_| BlixardError::Internal {
                message: "Proposal response channel closed".to_string(),
            })?
    }
    
    pub async fn propose_conf_change(&self, change_type: ConfChangeType, node_id: u64, address: String) -> BlixardResult<()> {
        let (tx, rx) = oneshot::channel();
        let conf_change = RaftConfChange {
            change_type,
            node_id,
            address,
            response_tx: Some(tx),
        };
        
        self.conf_change_tx.send(conf_change)
            .map_err(|_| BlixardError::Internal {
                message: "Failed to send conf change".to_string(),
            })?;
        
        // Wait for response
        rx.await
            .map_err(|_| BlixardError::Internal {
                message: "Conf change response channel closed".to_string(),
            })?
    }
}

// Helper function to schedule tasks to workers based on resource requirements
pub async fn schedule_task(
    database: Arc<Database>,
    _task_id: &str,
    task: &TaskSpec,
) -> BlixardResult<Option<u64>> {
    use crate::storage::{WORKER_TABLE, WORKER_STATUS_TABLE};
    
    tracing::info!("Scheduling task with requirements: {:?}", task.resources);
    
    let read_txn = database.begin_read()?;
    let worker_table = read_txn.open_table(WORKER_TABLE)?;
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE)?;
    
    // Find available workers
    let mut best_worker: Option<(u64, i32)> = None; // (node_id, score)
    
    let mut worker_count = 0;
    for entry in worker_table.iter()? {
        let (node_id_bytes, worker_data) = entry?;
        let node_id = u64::from_le_bytes(node_id_bytes.value().try_into()
            .map_err(|_| BlixardError::Storage {
                operation: "read node id".to_string(),
                source: "invalid node id bytes length".into(),
            })?);
        worker_count += 1;
        
        tracing::info!("Found worker {}", node_id);
        
        // Check if worker is online
        if let Some(status_data) = status_table.get(node_id_bytes.value())? {
            let status = WorkerStatus::try_from(status_data.value()[0]).ok();
            tracing::info!("Worker {} status: {:?}", node_id, status);
            if status != Some(WorkerStatus::Online) {
                continue;
            }
        } else {
            tracing::warn!("Worker {} has no status entry", node_id);
            continue;
        }
        
        // Deserialize worker capabilities
        let (_address, capabilities): (String, WorkerCapabilities) = 
            bincode::deserialize(worker_data.value())
                .map_err(|e| BlixardError::Serialization {
                    operation: "deserialize worker capabilities".to_string(),
                    source: Box::new(e),
                })?;
        
        tracing::info!("Worker {} capabilities: {:?}", node_id, capabilities);
        
        // Check if worker meets requirements
        if capabilities.cpu_cores >= task.resources.cpu_cores &&
           capabilities.memory_mb >= task.resources.memory_mb &&
           capabilities.disk_gb >= task.resources.disk_gb &&
           task.resources.required_features.iter().all(|f| capabilities.features.contains(f)) {
            
            // Calculate score (simple scoring based on available resources)
            let score = (capabilities.cpu_cores - task.resources.cpu_cores) as i32 +
                       ((capabilities.memory_mb - task.resources.memory_mb) / 1024) as i32;
            
            if best_worker.map_or(true, |(_id, best_score)| score < best_score) {
                best_worker = Some((node_id, score));
            }
        }
    }
    
    tracing::info!("Total workers found: {}, best worker: {:?}", worker_count, best_worker);
    Ok(best_worker.map(|(node_id, _)| node_id))
}