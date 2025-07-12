//! Raft proposal types and processing
//!
//! This module defines all the proposal types that can be submitted to the Raft
//! consensus layer, including task management, worker registration, VM operations,
//! and IP pool management.

use crate::error::BlixardError;
use crate::types::{NodeTopology, VmCommand, VmId, VmStatus};
use serde::{Deserialize, Serialize};

/// Main proposal data enum containing all possible proposals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalData {
    // Task management
    AssignTask {
        task_id: String,
        node_id: u64,
        task: TaskSpec,
    },
    CompleteTask {
        task_id: String,
        result: TaskResult,
    },
    ReassignTask {
        task_id: String,
        from_node: u64,
        to_node: u64,
    },

    // Worker management
    RegisterWorker {
        node_id: u64,
        address: String,
        capabilities: WorkerCapabilities,
        topology: NodeTopology,
    },
    UpdateWorkerStatus {
        node_id: u64,
        status: WorkerStatus,
    },
    RemoveWorker {
        node_id: u64,
    },

    // VM operations
    // Note: CreateVm is a misnomer - it handles all VM operations (create, start, stop, delete)
    // TODO: Rename to VmOperation(VmCommand) for clarity
    CreateVm(VmCommand),
    UpdateVmStatus {
        vm_name: String,
        status: VmStatus,
        node_id: u64,
    },
    MigrateVm {
        vm_name: String,
        from_node: u64,
        to_node: u64,
    },

    // IP pool operations
    IpPoolCommand(crate::ip_pool::IpPoolCommand),
    AllocateIp {
        request: crate::ip_pool::IpAllocationRequest,
    },
    ReleaseVmIps {
        vm_id: VmId,
    },

    // Batch processing
    Batch(Vec<ProposalData>),
}

/// Task specification for distributed task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub command: String,
    pub args: Vec<String>,
    pub resources: ResourceRequirements,
    pub timeout_secs: u64,
}

/// Result of task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Worker node capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>, // e.g., ["gpu", "high-memory", "ssd"]
}

/// Resource requirements for tasks and VMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub required_features: Vec<String>,
}

/// Worker node status
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

impl ProposalData {
    /// Get a human-readable name for the proposal type
    pub fn proposal_type(&self) -> &'static str {
        match self {
            ProposalData::AssignTask { .. } => "assign_task",
            ProposalData::CompleteTask { .. } => "complete_task",
            ProposalData::ReassignTask { .. } => "reassign_task",
            ProposalData::RegisterWorker { .. } => "register_worker",
            ProposalData::UpdateWorkerStatus { .. } => "update_worker_status",
            ProposalData::RemoveWorker { .. } => "remove_worker",
            ProposalData::CreateVm(_) => "vm_operation",
            ProposalData::UpdateVmStatus { .. } => "update_vm_status",
            ProposalData::MigrateVm { .. } => "migrate_vm",
            ProposalData::IpPoolCommand(_) => "ip_pool_command",
            ProposalData::AllocateIp { .. } => "allocate_ip",
            ProposalData::ReleaseVmIps { .. } => "release_vm_ips",
            ProposalData::Batch(_) => "batch",
        }
    }

    /// Check if this proposal modifies cluster resources
    pub fn modifies_resources(&self) -> bool {
        match self {
            ProposalData::AssignTask { .. }
            | ProposalData::RegisterWorker { .. }
            | ProposalData::RemoveWorker { .. }
            | ProposalData::CreateVm(_)
            | ProposalData::MigrateVm { .. }
            | ProposalData::AllocateIp { .. }
            | ProposalData::ReleaseVmIps { .. } => true,

            ProposalData::CompleteTask { .. }
            | ProposalData::ReassignTask { .. }
            | ProposalData::UpdateWorkerStatus { .. }
            | ProposalData::UpdateVmStatus { .. }
            | ProposalData::IpPoolCommand(_) => false,

            ProposalData::Batch(proposals) => proposals.iter().any(|p| p.modifies_resources()),
        }
    }

    /// Check if this proposal requires admission control
    pub fn requires_admission_control(&self) -> bool {
        match self {
            ProposalData::AssignTask { .. }
            | ProposalData::CreateVm(VmCommand::Create { .. })
            | ProposalData::MigrateVm { .. } => true,
            _ => false,
        }
    }
}

/// Batch proposal builder for efficient multi-operation processing
pub struct BatchProposalBuilder {
    proposals: Vec<ProposalData>,
}

impl BatchProposalBuilder {
    pub fn new() -> Self {
        Self {
            proposals: Vec::new(),
        }
    }

    pub fn add(mut self, proposal: ProposalData) -> Self {
        self.proposals.push(proposal);
        self
    }

    pub fn build(self) -> ProposalData {
        ProposalData::Batch(self.proposals)
    }
}
