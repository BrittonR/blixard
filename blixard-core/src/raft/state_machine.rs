//! Raft state machine implementation
//!
//! This module contains the state machine that applies Raft log entries to the database.
//! It handles all proposal types and maintains consistency across the distributed system.

use crate::common::error_context::{SerializationContext, StorageContext};
use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, safe_metrics, Timer};
use crate::raft_storage::{
    IP_ALLOCATION_TABLE, NODE_TOPOLOGY_TABLE, RESOURCE_POLICY_TABLE, TASK_ASSIGNMENT_TABLE,
    TASK_RESULT_TABLE, TASK_TABLE, TENANT_QUOTA_TABLE, TENANT_USAGE_TABLE, VM_IP_MAPPING_TABLE, VM_STATE_TABLE, WORKER_STATUS_TABLE,
    WORKER_TABLE,
};
use crate::resource_admission::{AdmissionControlConfig, ResourceAdmissionController};
use crate::resource_management::{ClusterResourceManager, OvercommitPolicy};
use crate::types::{NodeTopology, VmCommand, VmConfig, VmId, VmState, VmStatus};

use super::proposals::{ProposalData, TaskResult, TaskSpec, WorkerCapabilities, WorkerStatus};

use raft::prelude::Entry;
use redb::{Database, ReadableTable, WriteTransaction};
use std::sync::{Arc, Weak};
use tracing::{debug, error, info, warn};

/// State machine that applies Raft log entries to the database
pub struct RaftStateMachine {
    database: Arc<Database>,
    #[allow(dead_code)] // Shared state reserved for future state machine coordination
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
        let resource_manager = Arc::new(tokio::sync::RwLock::new(ClusterResourceManager::new(
            config.default_overcommit_policy.clone(),
        )));

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

        #[cfg(feature = "observability")]
        let _timer = safe_metrics().ok().map(|m| Timer::with_attributes(
            m.raft_proposal_duration.clone(),
            vec![attributes::operation(proposal.proposal_type())],
        ));

        debug!("Applying proposal: {:?}", proposal.proposal_type());

        let write_txn = self
            .database
            .begin_write()
            .storage_context("begin write transaction")?;

        match proposal {
            ProposalData::AssignTask {
                task_id,
                node_id,
                task,
            } => {
                self.apply_assign_task(write_txn, &task_id, node_id, &task)?;
            }
            ProposalData::CompleteTask { task_id, result } => {
                self.apply_complete_task(write_txn, &task_id, &result)?;
            }
            ProposalData::ReassignTask {
                task_id,
                from_node,
                to_node,
            } => {
                self.apply_reassign_task(write_txn, &task_id, from_node, to_node)?;
            }
            ProposalData::RegisterWorker {
                node_id,
                address,
                capabilities,
                topology,
            } => {
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
            ProposalData::UpdateVmStatus {
                vm_name,
                status,
                node_id,
            } => {
                self.apply_update_vm_status(write_txn, &vm_name, status, node_id)?;
            }
            ProposalData::MigrateVm {
                vm_name,
                from_node,
                to_node,
            } => {
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
            ProposalData::SetTenantQuota { quota } => {
                self.apply_set_tenant_quota(write_txn, &quota)?;
            }
            ProposalData::UpdateTenantUsage {
                tenant_id,
                delta,
                operation_id,
            } => {
                self.apply_update_tenant_usage(write_txn, &tenant_id, &delta, &operation_id)?;
            }
            ProposalData::RemoveTenantQuota { tenant_id } => {
                self.apply_remove_tenant_quota(write_txn, &tenant_id)?;
            }
            ProposalData::Batch(proposals) => {
                // TODO: Optimize batch processing to use a single transaction
                // Currently each sub-proposal gets its own transaction due to
                // the way apply_entry works. This should be refactored to pass
                // the transaction through to allow true batch processing.

                // Commit the current transaction first
                write_txn
                    .commit()
                    .storage_context("commit batch transaction")?;

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
                            .serialize_context("sub-proposal")?,
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

    fn apply_reassign_task(
        &self,
        txn: WriteTransaction,
        task_id: &str,
        from_node: u64,
        to_node: u64,
    ) -> BlixardResult<()> {
        {
            // Update task assignment
            let mut assignment_table = txn.open_table(TASK_ASSIGNMENT_TABLE)?;
            let assignment_data = to_node.to_le_bytes();
            assignment_table.insert(task_id, assignment_data.as_slice())?;
        }

        txn.commit().storage_context("commit reassign task")?;
        info!(
            "Reassigned task {} from node {} to node {}",
            task_id, from_node, to_node
        );
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
        let worker_data =
            bincode::serialize(&(address, capabilities)).serialize_context("worker info")?;
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
        info!(
            "Registered worker {} at {} with {} cores, {} MB memory",
            node_id, address, capabilities.cpu_cores, capabilities.memory_mb
        );
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

        txn.commit()
            .storage_context("commit update worker status")?;
        info!("Updated worker {} status to {:?}", node_id, status);
        Ok(())
    }

    fn apply_remove_worker(&self, txn: WriteTransaction, node_id: u64) -> BlixardResult<()> {
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

    fn apply_vm_command(&self, txn: WriteTransaction, command: &VmCommand) -> BlixardResult<()> {
        match command {
            VmCommand::Create { config, node_id } => {
                // Validate admission if controller is configured
                if let Some(ref _controller) = self.admission_controller {
                    self.validate_vm_admission(config, *node_id)?;
                }

                let now = chrono::Utc::now();
                let vm_state = VmState {
                    name: config.name.clone(),
                    status: VmStatus::Creating,
                    node_id: *node_id,
                    config: config.clone(),
                    created_at: now,
                    updated_at: now,
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

            VmCommand::Stop { name } => {
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
                    let vm_data = vm_table
                        .get(task.vm_name.as_str())?
                        .map(|data| data.value().to_vec());

                    if let Some(data) = vm_data {
                        let mut vm_state: VmState = bincode::deserialize(&data)
                            .deserialize_context("vm state", "VmState")?;
                        vm_state.node_id = task.target_node_id;
                        vm_state.status = VmStatus::Stopping;

                        let updated_data =
                            bincode::serialize(&vm_state).serialize_context("vm state")?;
                        vm_table.insert(task.vm_name.as_str(), updated_data.as_slice())?;
                    }
                }
                txn.commit().storage_context("commit migrate vm")?;
                info!(
                    "Migrating VM {} from node {} to node {}",
                    task.vm_name, task.source_node_id, task.target_node_id
                );
            }

            _ => {
                warn!("Unhandled VM command: {:?}", command);
                txn.commit()
                    .storage_context("commit unhandled vm command")?;
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
            let vm_data = vm_table.get(vm_name)?.map(|data| data.value().to_vec());

            if let Some(data) = vm_data {
                let mut vm_state: VmState =
                    bincode::deserialize(&data).deserialize_context("vm state", "VmState")?;
                vm_state.status = status;
                vm_state.node_id = node_id;

                let updated_data = bincode::serialize(&vm_state).serialize_context("vm state")?;
                vm_table.insert(vm_name, updated_data.as_slice())?;
            }
        }

        txn.commit().storage_context("commit update vm status")?;
        info!(
            "Updated VM {} status to {:?} on node {}",
            vm_name, status, node_id
        );
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

            // Get the data and make a copy to avoid borrow conflicts
            let vm_data_bytes = match vm_table.get(vm_name)? {
                Some(data) => Some(data.value().to_vec()),
                None => None,
            };

            if let Some(data) = vm_data_bytes {
                let mut vm_state: VmState =
                    bincode::deserialize(&data).deserialize_context("vm state", "VmState")?;
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
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;

        runtime.block_on(self.ip_pool_manager.process_command(command))?;
        write_txn
            .commit()
            .storage_context("commit ip pool command")?;
        Ok(())
    }

    async fn apply_allocate_ip(
        &self,
        write_txn: WriteTransaction,
        request: crate::ip_pool::IpAllocationRequest,
    ) -> BlixardResult<()> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;

        let result = runtime.block_on(self.ip_pool_manager.allocate_ip(request));

        match result {
            Ok(allocation_result) => {
                // Store allocation in database
                let mut ip_alloc_table = write_txn.open_table(IP_ALLOCATION_TABLE)?;
                let alloc_data = bincode::serialize(&allocation_result.allocation)
                    .serialize_context("IP allocation")?;

                let key = format!(
                    "{}:{}",
                    allocation_result.allocation.pool_id, allocation_result.allocation.ip
                );
                ip_alloc_table.insert(key.as_str(), alloc_data.as_slice())?;

                // Update VM->IP mapping
                let mut vm_ip_table = write_txn.open_table(VM_IP_MAPPING_TABLE)?;
                let mut vm_ips = if let Some(data) =
                    vm_ip_table.get(allocation_result.allocation.vm_id.to_string().as_str())?
                {
                    bincode::deserialize::<Vec<std::net::IpAddr>>(data.value())
                        .unwrap_or_else(|e| {
                            warn!("Failed to deserialize VM IP addresses, using empty list: {}", e);
                            Vec::new()
                        })
                } else {
                    Vec::new()
                };

                if !vm_ips.contains(&allocation_result.allocation.ip) {
                    vm_ips.push(allocation_result.allocation.ip);
                }

                let ip_data = bincode::serialize(&vm_ips).serialize_context("VM IPs")?;
                vm_ip_table.insert(
                    allocation_result.allocation.vm_id.to_string().as_str(),
                    ip_data.as_slice(),
                )?;

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
            bincode::deserialize::<Vec<std::net::IpAddr>>(&value).unwrap_or_else(|e| {
                warn!("Failed to deserialize VM IP addresses for release, using empty list: {}", e);
                Vec::new()
            })
        } else {
            Vec::new()
        };
        drop(mapping_table);

        // Release each IP back to its pool
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BlixardError::Internal {
                message: "No tokio runtime available".to_string(),
            })?;

        for ip in vm_ips {
            if let Err(e) = runtime.block_on(self.ip_pool_manager.release_vm_ips(vm_id)) {
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

        write_txn
            .commit()
            .storage_context("commit release vm ips")?;
        Ok(())
    }

    // Admission control helpers

    /// Validates that a VM can be admitted to the specified node
    /// This is critical admission control that prevents resource overcommit
    fn validate_vm_admission(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        // Try to load overcommit policy for the node
        let read_txn = self.database.begin_read()?;
        let _overcommit_policy =
            if let Ok(policy_table) = read_txn.open_table(RESOURCE_POLICY_TABLE) {
                if let Ok(Some(policy_data)) = policy_table.get(node_id.to_le_bytes().as_ref()) {
                    bincode::deserialize::<OvercommitPolicy>(policy_data.value()).ok()
                } else {
                    None
                }
            } else {
                None
            };

        // Use the configured admission controller if available
        if let Some(ref _controller) = self.admission_controller {
            // Check admission with the overcommit policy
            let resources = crate::resource_quotas::ResourceRequest {
                tenant_id: config.tenant_id.clone(),
                node_id: Some(node_id),
                vcpus: config.vcpus,
                memory_mb: config.memory as u64,
                disk_gb: 0, // TODO: Add disk requirements to VmConfig
                timestamp: chrono::Utc::now().into(),
            };

            let runtime =
                tokio::runtime::Handle::try_current().map_err(|_| BlixardError::Internal {
                    message: "No tokio runtime available".to_string(),
                })?;

            // Check with admission controller
            // controller.validate_vm_admission(config, node_id)?;
            
            // Also check quota through shared state
            if let Some(shared_state) = self.shared_state.upgrade() {
                runtime.block_on(async {
                    if let Some(quota_manager) = shared_state.get_quota_manager().await {
                        quota_manager.check_resource_quota(&resources).await
                            .map_err(|violation| BlixardError::InsufficientResources {
                                requested: format!("VM with {} vCPUs, {} MB memory", config.vcpus, config.memory),
                                available: format!("Quota violation: {:?}", violation),
                            })
                    } else {
                        Ok(())
                    }
                })?;
            }
        }

        Ok(())
    }
    
    // Quota management methods
    
    fn apply_set_tenant_quota(
        &self,
        txn: WriteTransaction,
        quota: &crate::resource_quotas::TenantQuota,
    ) -> BlixardResult<()> {
        {
            let mut quota_table = txn.open_table(TENANT_QUOTA_TABLE)?;
            let quota_data = bincode::serialize(quota).serialize_context("tenant quota")?;
            quota_table.insert(quota.tenant_id.as_str(), quota_data.as_slice())?;
        }
        txn.commit().storage_context("commit set tenant quota")?;
        
        // Update in-memory quota manager if available
        if let Some(shared_state) = self.shared_state.upgrade() {
            let runtime = tokio::runtime::Handle::try_current().ok();
            if let Some(runtime) = runtime {
                runtime.block_on(async {
                    if let Some(quota_manager) = shared_state.get_quota_manager().await {
                        let _ = quota_manager.set_tenant_quota(quota.clone()).await;
                    }
                });
            }
        }
        
        Ok(())
    }
    
    fn apply_update_tenant_usage(
        &self,
        txn: WriteTransaction,
        tenant_id: &str,
        delta: &crate::raft::proposals::ResourceUsageDelta,
        _operation_id: &uuid::Uuid,
    ) -> BlixardResult<()> {
        // Read current usage and update it
        let updated_usage = {
            let mut usage_table = txn.open_table(TENANT_USAGE_TABLE)?;
            
            // Read current usage
            let current_usage = if let Ok(Some(usage_data)) = usage_table.get(tenant_id) {
            if let Ok(usage) = bincode::deserialize::<crate::resource_quotas::TenantUsage>(usage_data.value()) {
                usage
            } else {
                // Create a default TenantUsage
                crate::resource_quotas::TenantUsage {
                    tenant_id: tenant_id.to_string(),
                    vm_usage: crate::resource_quotas::VmResourceUsage {
                        active_vms: 0,
                        used_vcpus: 0,
                        used_memory_mb: 0,
                        used_disk_gb: 0,
                        vms_per_node: std::collections::HashMap::new(),
                    },
                    api_usage: crate::resource_quotas::ApiUsage {
                        requests_current_second: 0,
                        requests_current_minute: 0,
                        requests_current_hour: 0,
                        concurrent_requests: 0,
                        operation_usage: crate::resource_quotas::OperationUsage {
                            vm_create_current_minute: 0,
                            vm_delete_current_minute: 0,
                            cluster_join_current_hour: 0,
                            status_query_current_second: 0,
                            config_change_current_hour: 0,
                        },
                        request_timestamps: vec![],
                    },
                    storage_usage: crate::resource_quotas::StorageUsage {
                        used_storage_gb: 0,
                        disk_image_count: 0,
                        backup_storage_gb: 0,
                        current_iops: 0,
                    },
                    updated_at: std::time::SystemTime::now(),
                }
            }
        } else {
            // Create a default TenantUsage
            crate::resource_quotas::TenantUsage {
                tenant_id: tenant_id.to_string(),
                vm_usage: crate::resource_quotas::VmResourceUsage {
                    active_vms: 0,
                    used_vcpus: 0,
                    used_memory_mb: 0,
                    used_disk_gb: 0,
                    vms_per_node: std::collections::HashMap::new(),
                },
                api_usage: crate::resource_quotas::ApiUsage {
                    requests_current_second: 0,
                    requests_current_minute: 0,
                    requests_current_hour: 0,
                    concurrent_requests: 0,
                    operation_usage: crate::resource_quotas::OperationUsage {
                        vm_create_current_minute: 0,
                        vm_delete_current_minute: 0,
                        cluster_join_current_hour: 0,
                        status_query_current_second: 0,
                        config_change_current_hour: 0,
                    },
                    request_timestamps: vec![],
                },
                storage_usage: crate::resource_quotas::StorageUsage {
                    used_storage_gb: 0,
                    disk_image_count: 0,
                    backup_storage_gb: 0,
                    current_iops: 0,
                },
                updated_at: std::time::SystemTime::now(),
            }
        };
        
            // Apply delta
            let mut updated_usage = current_usage;
            updated_usage.vm_usage.active_vms = (updated_usage.vm_usage.active_vms as i32 + delta.vm_count_delta).max(0) as u32;
            updated_usage.vm_usage.used_vcpus = (updated_usage.vm_usage.used_vcpus as i32 + delta.vcpu_delta).max(0) as u32;
            updated_usage.vm_usage.used_memory_mb = (updated_usage.vm_usage.used_memory_mb as i64 + delta.memory_mb_delta).max(0) as u64;
            updated_usage.vm_usage.used_disk_gb = (updated_usage.vm_usage.used_disk_gb as i64 + delta.disk_gb_delta).max(0) as u64;
            updated_usage.updated_at = std::time::SystemTime::now();
            
            // Write back
            let usage_data = bincode::serialize(&updated_usage).serialize_context("tenant usage")?;
            usage_table.insert(tenant_id, usage_data.as_slice())?;
            
            updated_usage
        };
        
        txn.commit().storage_context("commit update tenant usage")?;
        
        // Update in-memory quota manager if available
        if let Some(shared_state) = self.shared_state.upgrade() {
            let runtime = tokio::runtime::Handle::try_current().ok();
            if let Some(runtime) = runtime {
                runtime.block_on(async {
                    if let Some(quota_manager) = shared_state.get_quota_manager().await {
                        let _ = quota_manager.update_resource_usage(
                            tenant_id,
                            delta.vcpu_delta,
                            delta.memory_mb_delta,
                            delta.disk_gb_delta,
                            1  // Node ID - using 1 as placeholder since we don't have node ID in delta
                        ).await;
                    }
                });
            }
        }
        
        Ok(())
    }
    
    fn apply_remove_tenant_quota(
        &self,
        txn: WriteTransaction,
        tenant_id: &str,
    ) -> BlixardResult<()> {
        {
            let mut quota_table = txn.open_table(TENANT_QUOTA_TABLE)?;
            quota_table.remove(tenant_id)?;
            
            let mut usage_table = txn.open_table(TENANT_USAGE_TABLE)?;
            usage_table.remove(tenant_id)?;
        }
        
        txn.commit().storage_context("commit remove tenant quota")?;
        
        // Update in-memory quota manager if available
        if let Some(shared_state) = self.shared_state.upgrade() {
            let runtime = tokio::runtime::Handle::try_current().ok();
            if let Some(runtime) = runtime {
                runtime.block_on(async {
                    if let Some(quota_manager) = shared_state.get_quota_manager().await {
                        let _ = quota_manager.remove_tenant_quota(tenant_id).await;
                    }
                });
            }
        }
        
        Ok(())
    }
}
