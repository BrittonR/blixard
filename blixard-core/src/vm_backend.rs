use async_trait::async_trait;
use std::sync::Arc;
use redb::{Database, ReadableTable};

use crate::error::BlixardResult;
use crate::types::{VmConfig, VmStatus, VmCommand};

/// Abstract interface for VM backend implementations
/// 
/// This trait defines the contract that all VM backends must implement,
/// allowing the core distributed systems logic to be decoupled from
/// specific VM implementations (microvm.nix, Docker, Firecracker, etc.)
#[async_trait]
pub trait VmBackend: Send + Sync {
    /// Create a new VM with the given configuration
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()>;
    
    /// Start an existing VM
    async fn start_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Stop a running VM
    async fn stop_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Delete a VM and its resources
    async fn delete_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Update VM status (for health monitoring)
    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()>;
    
    /// Get the current status of a VM
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>>;
    
    /// List all VMs managed by this backend
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>>;
}

/// VM Manager that coordinates between Raft consensus and VM backend
/// 
/// This structure bridges the distributed systems layer (Raft consensus)
/// with the actual VM implementation. It maintains the same command-based
/// architecture but delegates actual VM operations to the backend.
pub struct VmManager {
    database: Arc<Database>,
    backend: Arc<dyn VmBackend>,
}

impl VmManager {
    /// Create a new VM manager with the given backend
    pub fn new(database: Arc<Database>, backend: Arc<dyn VmBackend>) -> Self {
        Self {
            database,
            backend,
        }
    }
    
    /// Process a VM command after Raft consensus
    pub async fn process_command(&self, command: VmCommand) -> BlixardResult<()> {
        // All state persistence has already been handled by the RaftStateMachine
        // before this command was forwarded to us. We only execute the actual VM operation.
        match command {
            VmCommand::Create { config, node_id } => {
                self.backend.create_vm(&config, node_id).await
            }
            VmCommand::Start { name } => {
                self.backend.start_vm(&name).await
            }
            VmCommand::Stop { name } => {
                self.backend.stop_vm(&name).await
            }
            VmCommand::Delete { name } => {
                self.backend.delete_vm(&name).await
            }
            VmCommand::UpdateStatus { name, status } => {
                self.backend.update_vm_status(&name, status).await
            }
        }
    }
    
    /// List all VMs (reads from Raft-managed database for consistency)
    pub async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        // Read from database instead of backend to ensure Raft consistency
        self.backend.list_vms().await
    }
    
    /// Get status of a specific VM (reads from Raft-managed database)
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        // Read from database instead of backend to ensure Raft consistency
        // We need to get both config and status, so we use list_vms and filter
        let all_vms = self.backend.list_vms().await?;
        Ok(all_vms.into_iter().find(|(config, _)| config.name == name))
    }
}

/// Mock VM backend for testing
/// 
/// This implementation logs operations without actually managing VMs,
/// useful for testing the distributed systems logic without VM dependencies.
pub struct MockVmBackend {
    database: Arc<Database>,
}

impl MockVmBackend {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl VmBackend for MockVmBackend {
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        tracing::info!("Mock: Creating VM '{}' on node {}", config.name, node_id);
        Ok(())
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Starting VM '{}'", name);
        Ok(())
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Stopping VM '{}'", name);
        Ok(())
    }
    
    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Deleting VM '{}'", name);
        Ok(())
    }
    
    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        tracing::info!("Mock: Updating VM '{}' status to {:?}", name, status);
        Ok(())
    }
    
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        // Read from Raft-managed database for consistency
        let read_txn = self.database.begin_read()?;
        
        if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(name) {
                let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
                Ok(Some(vm_state.status))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
    
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        // Read from Raft-managed database for consistency
        let read_txn = self.database.begin_read()?;
        let mut result = Vec::new();
        
        if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
            for entry in table.iter()? {
                let (_key, value) = entry?;
                let vm_state: crate::types::VmState = bincode::deserialize(value.value())?;
                result.push((vm_state.config, vm_state.status));
            }
        }
        
        Ok(result)
    }
}