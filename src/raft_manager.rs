use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, oneshot};
use tokio::time::{interval, Duration};
use raft::{Config, RawNode, StateRole};
use raft::prelude::*;
use slog::{Logger, o, info, Drain};
use redb::{Database, WriteTransaction, ReadableTable};
use chrono::Utc;
use serde::{Serialize, Deserialize};

use crate::error::{BlixardError, BlixardResult};
use crate::storage::RedbRaftStorage;
use crate::types::{VmCommand, VmStatus};

// Raft message types for cluster communication
#[derive(Debug)]
pub enum RaftMessage {
    // Raft protocol messages
    Raft(raft::prelude::Message),
    // Application-specific messages
    Propose(RaftProposal),
    // Configuration changes
    ConfChange(ConfChange),
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
    RegisterWorker { node_id: u64, address: String, capabilities: WorkerCapabilities },
    UpdateWorkerStatus { node_id: u64, status: WorkerStatus },
    RemoveWorker { node_id: u64 },
    
    // VM operations
    CreateVm(VmCommand),
    UpdateVmStatus { vm_name: String, status: VmStatus, node_id: u64 },
    MigrateVm { vm_name: String, from_node: u64, to_node: u64 },
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
}

impl RaftStateMachine {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub async fn apply_entry(&self, entry: &Entry) -> BlixardResult<()> {
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
            ProposalData::RegisterWorker { node_id, address, capabilities } => {
                self.apply_register_worker(write_txn, node_id, &address, &capabilities)?;
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
            _ => {
                // Handle other proposal types
            }
        }

        Ok(())
    }

    fn apply_assign_task(&self, txn: WriteTransaction, task_id: &str, node_id: u64, task: &TaskSpec) -> BlixardResult<()> {
        use crate::storage::{TASK_TABLE, TASK_ASSIGNMENT_TABLE};
        
        {
            // Store task spec
            let mut task_table = txn.open_table(TASK_TABLE)?;
            task_table.insert(task_id, bincode::serialize(task).unwrap().as_slice())?;
            
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
            result_table.insert(task_id, bincode::serialize(result).unwrap().as_slice())?;
            
            // Remove assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            assignment_table.remove(task_id)?;
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_register_worker(&self, txn: WriteTransaction, node_id: u64, address: &str, capabilities: &WorkerCapabilities) -> BlixardResult<()> {
        use crate::storage::{WORKER_TABLE, WORKER_STATUS_TABLE};
        
        let worker_data = bincode::serialize(&(address, capabilities)).unwrap();
        
        {
            // Store worker info
            let mut worker_table = txn.open_table(WORKER_TABLE)?;
            worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
            
            // Set initial status
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.insert(node_id.to_le_bytes().as_slice(), [WorkerStatus::Online as u8].as_slice())?;
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
        use crate::storage::VM_STATE_TABLE;
        
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
                    
                    table.insert(config.name.as_str(), bincode::serialize(&vm_state).unwrap().as_slice())?;
                }
                VmCommand::UpdateStatus { name, status } => {
                    let serialized = {
                        let vm_state_data = table.get(name.as_str())?;
                        if let Some(data) = vm_state_data {
                            let mut vm_state: crate::types::VmState = bincode::deserialize(data.value()).unwrap();
                            vm_state.status = *status;
                            vm_state.updated_at = Utc::now();
                            Some(bincode::serialize(&vm_state).unwrap())
                        } else {
                            None
                        }
                    };
                    
                    if let Some(data) = serialized {
                        table.insert(name.as_str(), data.as_slice())?;
                    }
                }
                _ => {
                    // Handle other VM commands
                }
            }
        }
        
        txn.commit()?;
        Ok(())
    }

    fn apply_update_vm_status(&self, txn: WriteTransaction, vm_name: &str, status: VmStatus, node_id: u64) -> BlixardResult<()> {
        use crate::storage::VM_STATE_TABLE;
        
        {
            let mut table = txn.open_table(VM_STATE_TABLE)?;
            let serialized = {
                let vm_state_data = table.get(vm_name)?;
                if let Some(data) = vm_state_data {
                    let mut vm_state: crate::types::VmState = bincode::deserialize(data.value()).unwrap();
                    vm_state.status = status;
                    vm_state.node_id = node_id;
                    vm_state.updated_at = Utc::now();
                    Some(bincode::serialize(&vm_state).unwrap())
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
}

// Main Raft manager that handles the Raft protocol
pub struct RaftManager {
    node_id: u64,
    raft_node: RwLock<RawNode<RedbRaftStorage>>,
    state_machine: Arc<RaftStateMachine>,
    peers: RwLock<HashMap<u64, String>>, // node_id -> address
    logger: Logger,
    
    // Channels for communication
    proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
    proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    message_tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
}

impl RaftManager {
    pub fn new(
        node_id: u64,
        database: Arc<Database>,
        peers: Vec<(u64, String)>,
    ) -> BlixardResult<(Self, mpsc::UnboundedSender<RaftProposal>, mpsc::UnboundedSender<(u64, raft::prelude::Message)>)> {
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

        // Create Raft node
        let raft_node = RawNode::new(&cfg, storage, &logger)
            .map_err(|e| BlixardError::Raft {
                operation: "create raft node".to_string(),
                source: Box::new(e),
            })?;

        // Create state machine
        let state_machine = Arc::new(RaftStateMachine::new(database));

        // Create channels
        let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        let manager = Self {
            node_id,
            raft_node: RwLock::new(raft_node),
            state_machine,
            peers: RwLock::new(peers.into_iter().collect()),
            logger,
            proposal_rx,
            proposal_tx: proposal_tx.clone(),
            message_rx,
            message_tx: message_tx.clone(),
        };

        Ok((manager, proposal_tx, message_tx))
    }
    
    /// Bootstrap a single-node cluster
    pub async fn bootstrap_single_node(&mut self) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        
        // Create a ConfChange to add this node as the sole voter
        let mut conf_change = ConfChange::default();
        let node_id = self.node_id;
        conf_change.node_id = node_id;
        conf_change.change_type = ConfChangeType::AddNode as i32;
        
        // Apply the configuration change
        node.apply_conf_change(&conf_change)?;
        
        // Campaign to become leader
        let _ = node.campaign();
        
        Ok(())
    }

    pub async fn run(mut self) -> BlixardResult<()> {
        let mut tick_timer = interval(Duration::from_millis(100));
        
        loop {
            tokio::select! {
                _ = tick_timer.tick() => {
                    self.tick().await?;
                }
                Some(proposal) = self.proposal_rx.recv() => {
                    self.handle_proposal(proposal).await?;
                }
                Some((from, msg)) = self.message_rx.recv() => {
                    self.handle_raft_message(from, msg).await?;
                }
            }
            
            // Process ready state
            self.on_ready().await?;
        }
    }

    async fn tick(&self) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        node.tick();
        Ok(())
    }

    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        let data = bincode::serialize(&proposal.data)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize proposal".to_string(),
                source: Box::new(e),
            })?;
        
        let mut node = self.raft_node.write().await;
        node.propose(vec![], data)
            .map_err(|e| BlixardError::Raft {
                operation: "propose".to_string(),
                source: Box::new(e),
            })?;
        
        Ok(())
    }

    async fn handle_raft_message(&self, _from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        node.step(msg)
            .map_err(|e| BlixardError::Raft {
                operation: "step message".to_string(),
                source: Box::new(e),
            })?;
        Ok(())
    }

    async fn on_ready(&self) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        if !node.has_ready() {
            return Ok(());
        }
        
        let mut ready = node.ready();
        
        // Send messages to peers
        if !ready.messages().is_empty() {
            for msg in ready.take_messages() {
                self.send_raft_message(msg).await?;
            }
        }
        
        // Apply committed entries to state machine
        if !ready.committed_entries().is_empty() {
            for entry in ready.take_committed_entries() {
                self.state_machine.apply_entry(&entry).await?;
            }
        }
        
        // Advance the Raft node
        let light_rd = node.advance(ready);
        
        // Update commit index if needed
        if let Some(commit) = light_rd.commit_index() {
            // Store commit index update
            let mut hs = node.raft.hard_state();
            hs.set_commit(commit);
            node.mut_store().save_hard_state(&hs)?;
        }
        
        Ok(())
    }

    async fn send_raft_message(&self, msg: raft::prelude::Message) -> BlixardResult<()> {
        let peers = self.peers.read().await;
        if let Some(_addr) = peers.get(&msg.to) {
            // TODO: Send message via gRPC to peer
            info!(self.logger, "Sending Raft message"; "to" => msg.to, "type" => ?msg.msg_type);
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

    pub async fn propose(&self, data: ProposalData) -> BlixardResult<()> {
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
}

// Helper function to schedule tasks to workers based on resource requirements
pub async fn schedule_task(
    database: Arc<Database>,
    _task_id: &str,
    task: &TaskSpec,
) -> BlixardResult<Option<u64>> {
    use crate::storage::{WORKER_TABLE, WORKER_STATUS_TABLE};
    
    let read_txn = database.begin_read()?;
    let worker_table = read_txn.open_table(WORKER_TABLE)?;
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE)?;
    
    // Find available workers
    let mut best_worker: Option<(u64, i32)> = None; // (node_id, score)
    
    for entry in worker_table.iter()? {
        let (node_id_bytes, worker_data) = entry?;
        let node_id = u64::from_le_bytes(node_id_bytes.value().try_into().unwrap());
        
        // Check if worker is online
        if let Some(status_data) = status_table.get(node_id_bytes.value())? {
            let status = WorkerStatus::try_from(status_data.value()[0]).ok();
            if status != Some(WorkerStatus::Online) {
                continue;
            }
        }
        
        // Deserialize worker capabilities
        let (_address, capabilities): (String, WorkerCapabilities) = 
            bincode::deserialize(worker_data.value()).unwrap();
        
        // Check if worker meets requirements
        if capabilities.cpu_cores >= task.resources.cpu_cores &&
           capabilities.memory_mb >= task.resources.memory_mb &&
           capabilities.disk_gb >= task.resources.disk_gb &&
           task.resources.required_features.iter().all(|f| capabilities.features.contains(f)) {
            
            // Calculate score (simple scoring based on available resources)
            let score = (capabilities.cpu_cores - task.resources.cpu_cores) as i32 +
                       ((capabilities.memory_mb - task.resources.memory_mb) / 1024) as i32;
            
            if best_worker.is_none() || score < best_worker.unwrap().1 {
                best_worker = Some((node_id, score));
            }
        }
    }
    
    Ok(best_worker.map(|(node_id, _)| node_id))
}