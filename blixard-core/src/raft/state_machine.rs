//! Raft state machine implementation
//!
//! This module contains the state machine that applies Raft log entries to the database.
//! It handles all proposal types and maintains consistency across the distributed system.

use crate::error::{BlixardError, BlixardResult};
use crate::common::error_context::{StorageContext, SerializationContext};
use crate::metrics_otel::{attributes, metrics, Timer};
use crate::raft_storage::{
    TASK_ASSIGNMENT_TABLE, TASK_RESULT_TABLE,
    TASK_TABLE, VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE,
    IP_ALLOCATION_TABLE, VM_IP_MAPPING_TABLE, RESOURCE_POLICY_TABLE,
};
use crate::resource_admission::{ResourceAdmissionController, AdmissionControlConfig};
use crate::resource_management::{ClusterResourceManager, OvercommitPolicy};
use crate::types::{VmCommand, VmConfig, VmStatus, VmState, NodeTopology, VmId};

use super::proposals::{ProposalData, TaskSpec, TaskResult, WorkerCapabilities, WorkerStatus};

use raft::prelude::Entry;
use redb::{Database, ReadableTable, WriteTransaction};
use std::sync::{Arc, Weak};
use tracing::{debug, error, info, warn};

/// State machine that applies Raft log entries to the database
pub struct RaftStateMachine {
    database: Arc<Database>,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    admission_controller: Option<Arc<ResourceAdmissionController>>,
    ip_pool_manager: Arc<crate::ip_pool_manager::IpPoolManager>,
}

impl RaftStateMachine {
    pub fn new(
        database: Arc<Database>,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
    ) -> Self {
        Self {
            database,
            shared_state,
            admission_controller: None,
            ip_pool_manager: Arc::new(crate::ip_pool_manager::IpPoolManager::new()),
        }
    }
    
    /// Configure the admission controller with custom settings
    pub fn configure_admission_controller(&mut self, config: AdmissionControlConfig) {
        let resource_manager = Arc::new(tokio::sync::RwLock::new(
            ClusterResourceManager::new(config.default_overcommit_policy.clone())
        ));
        
        self.admission_controller = Some(Arc::new(ResourceAdmissionController::new(
            self.database.clone(),
            config,
            resource_manager,
        )));
    }

    /// Apply a log entry to the state machine
    pub async fn apply_entry(&self, entry: &Entry) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        crate::fail_point!("raft::apply_entry");

        if entry.data.is_empty() {
            return Ok(());
        }

        let proposal: ProposalData =
            bincode::deserialize(&entry.data).deserialize_context("proposal", "ProposalData")?;

        let _timer = Timer::new(&metrics().raft_proposal_apply_duration, &[
            attributes::operation(proposal.proposal_type()),
        ]);

        debug!("Applying proposal: {:?}", proposal.proposal_type());

        let write_txn = self.database.begin_write().storage_context("begin write transaction")?;

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
            ProposalData::RemoveWorker { node_id } => {
                self.apply_remove_worker(write_txn, node_id)?;
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
                write_txn.commit().storage_context("commit batch transaction")?;

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
                        data: bincode::serialize(&sub_proposal).serialize_context("sub-proposal")?,
                        context: entry.context.clone(),
                        sync_log: entry.sync_log,
                    };

                    // Recursively apply the sub-proposal
                    Box::pin(self.apply_entry(&sub_entry)).await?;
                }

                return Ok(());
            }
            ProposalData::IpPoolCommand(command) => {
                self.apply_ip_pool_command(write_txn, command).await?;
            }
            ProposalData::AllocateIp { request } => {
                self.apply_allocate_ip(write_txn, request).await?;
            }
            ProposalData::ReleaseVmIps { vm_id } => {
                self.apply_release_vm_ips(write_txn, vm_id).await?;
            }
        }

        Ok(())
    }

    // Task management methods
    
    fn apply_assign_task(
        &self,
        txn: WriteTransaction,
        task_id: &str,
        node_id: u64,
        task: &TaskSpec,
    ) -> BlixardResult<()> {
        {
            // Store task spec
            let mut task_table = txn.open_table(TASK_TABLE)?;
            let task_data = bincode::serialize(task).serialize_context("task spec")?;
            task_table.insert(task_id, task_data.as_slice())?;

            // Store assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            assignment_table.insert(task_id, node_id.to_le_bytes().as_slice())?;
        }

        txn.commit().storage_context("commit assign task")?;
        info!("Assigned task {} to node {}", task_id, node_id);
        Ok(())
    }

    fn apply_complete_task(
        &self,
        txn: WriteTransaction,
        task_id: &str,
        result: &TaskResult,
    ) -> BlixardResult<()> {
        {
            // Store result
            let mut result_table = txn.open_table(TASK_RESULT_TABLE)?;
            let result_data = bincode::serialize(result).serialize_context("task result")?;
            result_table.insert(task_id, result_data.as_slice())?;

            // Remove assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            assignment_table.remove(task_id)?;
        }

        txn.commit().storage_context("commit complete task")?;
        info!("Completed task {} with result: {}", task_id, result.success);
        Ok(())
    }

    // Worker management methods
    
    fn apply_register_worker(
        &self,
        txn: WriteTransaction,
        node_id: u64,
        address: &str,
        capabilities: &WorkerCapabilities,
        topology: &NodeTopology,
    ) -> BlixardResult<()> {
        let worker_data = bincode::serialize(&(address, capabilities))
            .serialize_context("worker info")?;
        let topology_data = bincode::serialize(topology).serialize_context("node topology")?;

        {
            // Store worker info
            let mut worker_table = txn.open_table(WORKER_TABLE)?;
            worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;

            // Set initial status
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.insert(
                node_id.to_le_bytes().as_slice(),
                [WorkerStatus::Online as u8].as_slice(),
            )?;

            // Store topology info
            let mut topology_table = txn.open_table(NODE_TOPOLOGY_TABLE)?;
            topology_table.insert(node_id.to_le_bytes().as_slice(), topology_data.as_slice())?;
        }

        txn.commit().storage_context("commit register worker")?;
        info!("Registered worker {} at {} with {} cores, {} MB memory", 
              node_id, address, capabilities.cpu_cores, capabilities.memory_mb);
        Ok(())
    }

    fn apply_update_worker_status(
        &self,
        txn: WriteTransaction,
        node_id: u64,
        status: WorkerStatus,
    ) -> BlixardResult<()> {
        {
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.insert(node_id.to_le_bytes().as_slice(), [status as u8].as_slice())?;
        }

        txn.commit().storage_context("commit update worker status")?;
        info!("Updated worker {} status to {:?}", node_id, status);
        Ok(())
    }
    
    fn apply_remove_worker(
        &self,
        txn: WriteTransaction,
        node_id: u64,
    ) -> BlixardResult<()> {
        {
            let mut worker_table = txn.open_table(WORKER_TABLE)?;
            worker_table.remove(node_id.to_le_bytes().as_slice())?;
            
            let mut status_table = txn.open_table(WORKER_STATUS_TABLE)?;
            status_table.remove(node_id.to_le_bytes().as_slice())?;
            
            let mut topology_table = txn.open_table(NODE_TOPOLOGY_TABLE)?;
            topology_table.remove(node_id.to_le_bytes().as_slice())?;
        }

        txn.commit().storage_context("commit remove worker")?;
        info!("Removed worker {}", node_id);
        Ok(())
    }

    // VM management methods
    
    fn apply_vm_command(
        &self,
        txn: WriteTransaction,
        command: &VmCommand,
    ) -> BlixardResult<()> {
        match command {
            VmCommand::Create { config, node_id } => {
                // Validate admission if controller is configured
                if let Some(ref controller) = self.admission_controller {
                    self.validate_vm_admission(config, *node_id)?;
                }
                
                let vm_state = VmState {
                    name: config.name.clone(),
                    status: VmStatus::Created,
                    node_id: Some(*node_id),
                    config: config.clone(),
                    health: crate::vm_health_types::HealthState::default(),
                };
                
                {
                    let mut vm_table = txn.open_table(VM_STATE_TABLE)?;
                    let vm_data = bincode::serialize(&vm_state).serialize_context("vm state")?;
                    vm_table.insert(config.name.as_str(), vm_data.as_slice())?;
                }
                
                txn.commit().storage_context("commit create vm")?;
                info!("Created VM {} on node {}", config.name, node_id);
            }
            
            VmCommand::Start { name } => {
                self.update_vm_status_internal(txn, name, VmStatus::Starting)?;
                info!("Starting VM {}", name);
            }
            
            VmCommand::Stop { name, force: _ } => {
                self.update_vm_status_internal(txn, name, VmStatus::Stopping)?;
                info!("Stopping VM {}", name);
            }
            
            VmCommand::Delete { name } => {
                {
                    let mut vm_table = txn.open_table(VM_STATE_TABLE)?;
                    vm_table.remove(name.as_str())?;
                }
                txn.commit().storage_context("commit delete vm")?;
                info!("Deleted VM {}", name);
            }
            
            VmCommand::Migrate { task } => {
                // Update VM's node assignment
                {
                    let mut vm_table = txn.open_table(VM_STATE_TABLE)?;
                    if let Some(data) = vm_table.get(task.vm_name.as_str())? {
                        let mut vm_state: VmState = bincode::deserialize(data.value())
                            .deserialize_context("vm state", "VmState")?;
                        vm_state.node_id = Some(task.target_node_id);
                        vm_state.status = VmStatus::Migrating;
                        
                        let updated_data = bincode::serialize(&vm_state).serialize_context("vm state")?;
                        vm_table.insert(task.vm_name.as_str(), updated_data.as_slice())?;
                    }
                }
                txn.commit().storage_context("commit migrate vm")?;
                info!("Migrating VM {} from node {} to node {}", 
                      task.vm_name, task.source_node_id, task.target_node_id);
            }
            
            _ => {
                warn!("Unhandled VM command: {:?}", command);
                txn.commit().storage_context("commit unhandled vm command")?;
            }
        }
        Ok(())
    }
    
    fn apply_update_vm_status(
        &self,
        txn: WriteTransaction,
        vm_name: &str,
        status: VmStatus,
        node_id: u64,
    ) -> BlixardResult<()> {
        {
            let mut vm_table = txn.open_table(VM_STATE_TABLE)?;
            if let Some(data) = vm_table.get(vm_name)? {
                let mut vm_state: VmState = bincode::deserialize(data.value())
                    .deserialize_context("vm state", "VmState")?;
                vm_state.status = status;
                vm_state.node_id = Some(node_id);
                
                let updated_data = bincode::serialize(&vm_state).serialize_context("vm state")?;
                vm_table.insert(vm_name, updated_data.as_slice())?;
            }
        }
        
        txn.commit().storage_context("commit update vm status")?;
        info!("Updated VM {} status to {:?} on node {}", vm_name, status, node_id);
        Ok(())
    }
    
    fn update_vm_status_internal(
        &self,
        txn: WriteTransaction,
        vm_name: &str,
        status: VmStatus,
    ) -> BlixardResult<()> {
        {
            let mut vm_table = txn.open_table(VM_STATE_TABLE)?;
            if let Some(data) = vm_table.get(vm_name)? {
                let mut vm_state: VmState = bincode::deserialize(data.value())
                    .deserialize_context("vm state", "VmState")?;
                vm_state.status = status;
                
                let updated_data = bincode::serialize(&vm_state).serialize_context("vm state")?;
                vm_table.insert(vm_name, updated_data.as_slice())?;
            }
        }
        
        txn.commit().storage_context("commit update vm status")?;
        Ok(())
    }

    // IP pool management methods
    
    async fn apply_ip_pool_command(
        &self,
        write_txn: WriteTransaction,
        command: crate::ip_pool::IpPoolCommand,
    ) -> BlixardResult<()> {
        // Process IP pool command
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;
        
        runtime.block_on(self.ip_pool_manager.process_command(command))?;
        write_txn.commit().storage_context("commit ip pool command")?;
        Ok(())
    }
    
    async fn apply_allocate_ip(
        &self,
        write_txn: WriteTransaction,
        request: crate::ip_pool::IpAllocationRequest,
    ) -> BlixardResult<()> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;
        
        let result = runtime.block_on(self.ip_pool_manager.allocate_ip(request));
        
        match result {
            Ok(allocation_result) => {
                // Store allocation in database
                let mut ip_alloc_table = write_txn.open_table(IP_ALLOCATION_TABLE)?;
                let alloc_data = bincode::serialize(&allocation_result.allocation)
                    .serialize_context("IP allocation")?;
                
                let key = format!("{}:{}", allocation_result.allocation.pool_id, allocation_result.allocation.ip);
                ip_alloc_table.insert(key.as_str(), alloc_data.as_slice())?;
                
                // Update VM->IP mapping
                let mut vm_ip_table = write_txn.open_table(VM_IP_MAPPING_TABLE)?;
                let mut vm_ips = if let Some(data) = vm_ip_table.get(allocation_result.allocation.vm_id.to_string().as_str())? {
                    bincode::deserialize::<Vec<std::net::IpAddr>>(data.value())
                        .unwrap_or_else(|_| Vec::new())
                } else {
                    Vec::new()
                };
                
                if !vm_ips.contains(&allocation_result.allocation.ip) {
                    vm_ips.push(allocation_result.allocation.ip);
                }
                
                let ip_data = bincode::serialize(&vm_ips).serialize_context("VM IPs")?;
                vm_ip_table.insert(allocation_result.allocation.vm_id.to_string().as_str(), ip_data.as_slice())?;
                
                info!(
                    "Allocated IP {} from pool {} to VM {}",
                    allocation_result.allocation.ip,
                    allocation_result.allocation.pool_id,
                    allocation_result.allocation.vm_id
                );
            }
            Err(e) => {
                error!("Failed to allocate IP: {}", e);
                // Don't fail the transaction, just log the error
            }
        }
        
        write_txn.commit().storage_context("commit allocate ip")?;
        Ok(())
    }
    
    async fn apply_release_vm_ips(
        &self,
        write_txn: WriteTransaction,
        vm_id: VmId,
    ) -> BlixardResult<()> {
        // Get VM's allocated IPs
        let mapping_table = write_txn.open_table(VM_IP_MAPPING_TABLE)?;
        let vm_ips = if let Some(data) = mapping_table.get(vm_id.to_string().as_str())? {
            let value = data.value().to_vec();
            bincode::deserialize::<Vec<std::net::IpAddr>>(&value)
                .unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };
        drop(mapping_table);
        
        // Release each IP back to its pool
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;
        
        for ip in vm_ips {
            if let Err(e) = runtime.block_on(self.ip_pool_manager.release_vm_ips(vm_id.clone())) {
                error!("Failed to release IP {} for VM {}: {}", ip, vm_id, e);
            } else {
                info!("Released IP {} for VM {}", ip, vm_id);
            }
        }
        
        // Remove VM->IP mapping
        {
            let mut mapping_table = write_txn.open_table(VM_IP_MAPPING_TABLE)?;
            mapping_table.remove(vm_id.to_string().as_str())?;
        }
        
        write_txn.commit().storage_context("commit release vm ips")?;
        Ok(())
    }

    // Admission control helpers
    
    /// Validates that a VM can be admitted to the specified node
    /// This is critical admission control that prevents resource overcommit
    fn validate_vm_admission(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        // Try to load overcommit policy for the node
        let read_txn = self.database.begin_read()?;
        let overcommit_policy = if let Ok(policy_table) = read_txn.open_table(RESOURCE_POLICY_TABLE) {
            if let Ok(Some(policy_data)) = policy_table.get(node_id.to_le_bytes().as_ref()) {
                bincode::deserialize::<OvercommitPolicy>(policy_data.value()).ok()
            } else {
                None
            }
        } else {
            None
        };
        
        // Use the configured admission controller if available
        if let Some(ref controller) = self.admission_controller {
            // Check admission with the overcommit policy
            let resources = crate::resource_quotas::ResourceRequest {
                vcpus: config.vcpus,
                memory_mb: config.memory,
                disk_gb: 0, // TODO: Add disk requirements to VmConfig
                features: vec![], // TODO: Add feature requirements
            };
            
            let runtime = tokio::runtime::Handle::try_current()
                .map_err(|_| BlixardError::Internal {
                    message: "No tokio runtime available".to_string(),
                })?;
            
            runtime.block_on(controller.check_admission(
                node_id,
                &resources,
                overcommit_policy.as_ref(),
            ))?;
        }
        
        Ok(())
    }
}