use async_trait::async_trait;
use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use redb::{Database, ReadableTable};
use uuid;

use crate::error::{BlixardResult, BlixardError};
use crate::types::{VmConfig, VmStatus, VmCommand};
use crate::vm_scheduler::{VmScheduler, PlacementStrategy, PlacementDecision};

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
    
    /// Get the IP address of a VM (optional, not all backends support this)
    async fn get_vm_ip(&self, _name: &str) -> BlixardResult<Option<String>> {
        Ok(None) // Default implementation returns None
    }
}

/// VM Manager that coordinates between Raft consensus and VM backend
/// 
/// This structure bridges the distributed systems layer (Raft consensus)
/// with the actual VM implementation. It maintains the same command-based
/// architecture but delegates actual VM operations to the backend.
pub struct VmManager {
    database: Arc<Database>,
    backend: Arc<dyn VmBackend>,
    node_state: Arc<crate::node_shared::SharedNodeState>,
}

impl VmManager {
    /// Create a new VM manager with the given backend
    pub fn new(database: Arc<Database>, backend: Arc<dyn VmBackend>, node_state: Arc<crate::node_shared::SharedNodeState>) -> Self {
        Self {
            database,
            backend,
            node_state,
        }
    }
    
    /// Process a VM command after Raft consensus
    pub async fn process_command(&self, command: VmCommand) -> BlixardResult<()> {
        // All state persistence has already been handled by the RaftStateMachine
        // before this command was forwarded to us. We only execute the actual VM operation.
        match command {
            VmCommand::Create { config, node_id } => {
                let result = self.backend.create_vm(&config, node_id).await;
                
                // Monitor VM status after creation attempt
                if result.is_ok() {
                    self.monitor_vm_status_after_operation(&config.name).await;
                }
                
                result
            }
            VmCommand::Start { name } => {
                let result = self.backend.start_vm(&name).await;
                
                // Monitor VM status after start attempt
                if result.is_ok() {
                    self.monitor_vm_status_after_operation(&name).await;
                }
                
                result
            }
            VmCommand::Stop { name } => {
                let result = self.backend.stop_vm(&name).await;
                
                // Monitor VM status after stop attempt
                if result.is_ok() {
                    self.monitor_vm_status_after_operation(&name).await;
                }
                
                result
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
    
    /// Get status of a specific VM (reads from Raft-managed database)
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        // Read from database instead of backend to ensure Raft consistency
        let read_txn = self.database.begin_read()?;
        
        if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(name) {
                let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
                Ok(Some((vm_state.config, vm_state.status)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
    
    /// Get the IP address of a VM from the backend
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        self.backend.get_vm_ip(name).await
    }
    
    /// Monitor VM status after an operation and trigger status updates through Raft
    /// 
    /// This method polls the actual VM status from the backend and compares it with
    /// the status stored in the database. If there's a difference, it triggers a
    /// status update through the Raft consensus mechanism.
    async fn monitor_vm_status_after_operation(&self, vm_name: &str) {
        // Spawn a background task to avoid blocking the main operation
        let backend = Arc::clone(&self.backend);
        let database = Arc::clone(&self.database);
        let node_state = Arc::clone(&self.node_state);
        let name = vm_name.to_string();
        
        tokio::spawn(async move {
            // Wait a moment for the VM operation to take effect
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            // Poll for status changes with timeout
            let mut attempts = 0;
            const MAX_ATTEMPTS: u32 = 20; // 10 seconds total (500ms * 20)
            
            while attempts < MAX_ATTEMPTS {
                // Get current status from backend (actual VM state)
                let actual_status = match backend.get_vm_status(&name).await {
                    Ok(Some(status)) => status,
                    Ok(None) => {
                        tracing::warn!("VM '{}' not found in backend during status monitoring", name);
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get VM '{}' status from backend: {}", name, e);
                        return;
                    }
                };
                
                // Get stored status from database (distributed state)
                let stored_status = {
                    let read_txn = match database.begin_read() {
                        Ok(txn) => txn,
                        Err(e) => {
                            tracing::warn!("Failed to read database during status monitoring: {}", e);
                            return;
                        }
                    };
                    
                    if let Ok(table) = read_txn.open_table(crate::storage::VM_STATE_TABLE) {
                        if let Ok(Some(data)) = table.get(name.as_str()) {
                            match bincode::deserialize::<crate::types::VmState>(data.value()) {
                                Ok(vm_state) => Some(vm_state.status),
                                Err(e) => {
                                    tracing::warn!("Failed to deserialize VM state during monitoring: {}", e);
                                    return;
                                }
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                
                // Check if status has changed and needs updating
                if let Some(stored) = stored_status {
                    if actual_status != stored {
                        tracing::info!(
                            "VM '{}' status changed: {:?} -> {:?} (triggering Raft update)", 
                            name, stored, actual_status
                        );
                        
                        // Trigger Raft status update
                        match node_state.update_vm_status_through_raft(name.clone(), actual_status, node_state.config.id).await {
                            Ok(_) => {
                                tracing::info!("Successfully triggered status update for VM '{}' to {:?}", name, actual_status);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to trigger status update for VM '{}': {}", name, e);
                            }
                        }
                        return;
                    } else if matches!(actual_status, VmStatus::Running | VmStatus::Stopped | VmStatus::Failed) {
                        // VM has reached a stable state, stop monitoring
                        tracing::debug!("VM '{}' status monitoring complete: {:?}", name, actual_status);
                        return;
                    }
                }
                
                attempts += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            
            tracing::warn!("VM '{}' status monitoring timed out after {} attempts", name, MAX_ATTEMPTS);
        });
    }
    
    /// Schedule VM placement using the intelligent scheduler
    /// 
    /// This method uses the VM scheduler to determine the best node for VM placement
    /// based on resource requirements and placement strategy. The actual VM creation
    /// still goes through Raft consensus for distributed coordination.
    pub async fn schedule_vm_placement(
        &self,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        let scheduler = VmScheduler::new(Arc::clone(&self.database));
        scheduler.schedule_vm_placement(vm_config, strategy).await
    }
    
    /// Get cluster-wide resource summary
    /// 
    /// This provides visibility into cluster resource utilization for
    /// management and monitoring purposes.
    pub async fn get_cluster_resource_summary(&self) -> BlixardResult<crate::vm_scheduler::ClusterResourceSummary> {
        let scheduler = VmScheduler::new(Arc::clone(&self.database));
        scheduler.get_cluster_resource_summary().await
    }
    
    /// Create a VM with automatic placement
    /// 
    /// This is a convenience method that combines scheduling and VM creation.
    /// It automatically selects the best node using the specified strategy,
    /// then creates the VM through the normal Raft consensus process.
    pub async fn create_vm_with_scheduling(
        &self,
        vm_config: VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        // Schedule placement
        let placement = self.schedule_vm_placement(&vm_config, strategy).await?;
        
        tracing::info!(
            "Scheduled VM '{}' for placement on node {}: {}",
            vm_config.name,
            placement.selected_node_id,
            placement.reason
        );
        
        // Propose VM creation through Raft consensus
        let vm_command = VmCommand::Create {
            config: vm_config,
            node_id: placement.selected_node_id,
        };
        
        // Submit the command through Raft for distributed consensus
        let proposal_data = crate::raft_manager::ProposalData::CreateVm(vm_command);
        let proposal = crate::raft_manager::RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: proposal_data,
            response_tx: None, // Fire-and-forget for scheduling
        };
        
        self.node_state.send_raft_proposal(proposal).await?;
        
        Ok(placement)
    }
}

/// Mock VM backend for testing
/// 
/// This implementation logs operations without actually managing VMs,
/// useful for testing the distributed systems logic without VM dependencies.
pub struct MockVmBackend {
    database: Arc<Database>,
    // Track simulated VM states for realistic status transitions
    simulated_states: std::sync::RwLock<std::collections::HashMap<String, (VmStatus, std::time::Instant)>>,
}

impl MockVmBackend {
    pub fn new(database: Arc<Database>) -> Self {
        Self { 
            database,
            simulated_states: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl VmBackend for MockVmBackend {
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        tracing::info!("Mock: Creating VM '{}' on node {}", config.name, node_id);
        
        // Simulate VM creation with status transition
        // Start with "Creating", will transition to "Running" after a short delay
        {
            let mut states = self.simulated_states.write().unwrap();
            states.insert(config.name.clone(), (crate::types::VmStatus::Creating, std::time::Instant::now()));
        }
        
        Ok(())
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Starting VM '{}'", name);
        
        // Simulate VM start with status transition to Running
        {
            let mut states = self.simulated_states.write().unwrap();
            states.insert(name.to_string(), (crate::types::VmStatus::Running, std::time::Instant::now()));
        }
        
        Ok(())
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Stopping VM '{}'", name);
        
        // Simulate VM stop with status transition to Stopped
        {
            let mut states = self.simulated_states.write().unwrap();
            states.insert(name.to_string(), (crate::types::VmStatus::Stopped, std::time::Instant::now()));
        }
        
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
        // Check simulated states for realistic status transitions
        {
            let states = self.simulated_states.read().unwrap();
            if let Some((status, created_at)) = states.get(name) {
                let elapsed = created_at.elapsed();
                
                // Simulate realistic status transitions based on time
                let current_status = match status {
                    crate::types::VmStatus::Creating => {
                        // After 1 second, transition from Creating to Running
                        if elapsed.as_secs() >= 1 {
                            crate::types::VmStatus::Running
                        } else {
                            *status
                        }
                    }
                    _ => *status, // Running, Stopped, etc. stay as they are
                };
                
                tracing::debug!("Mock: VM '{}' status: {:?} (elapsed: {:?})", name, current_status, elapsed);
                return Ok(Some(current_status));
            }
        }
        
        // Fallback to database if no simulated state (for existing VMs)
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

/// Factory trait for creating VM backends
/// 
/// This trait enables the plugin architecture for VM backends.
/// Different implementations (microvm.nix, Docker, Firecracker, etc.)
/// can register factories to create their specific backend instances.
pub trait VmBackendFactory: Send + Sync {
    /// Create a new VM backend instance
    fn create_backend(
        &self, 
        config_dir: PathBuf, 
        data_dir: PathBuf,
        database: Arc<Database>
    ) -> BlixardResult<Arc<dyn VmBackend>>;
    
    /// Get the name of this backend type
    fn backend_type(&self) -> &'static str;
    
    /// Get a description of this backend
    fn description(&self) -> &'static str;
}

/// Registry for VM backend factories
/// 
/// This provides a centralized way to register and discover VM backends.
/// Backends register themselves at startup, and the Node can create
/// the appropriate backend based on configuration.
pub struct VmBackendRegistry {
    factories: HashMap<String, Arc<dyn VmBackendFactory>>,
}

impl VmBackendRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }
    
    /// Register a backend factory
    pub fn register(&mut self, factory: Arc<dyn VmBackendFactory>) {
        let backend_type = factory.backend_type().to_string();
        tracing::info!("Registering VM backend: {} ({})", backend_type, factory.description());
        self.factories.insert(backend_type, factory);
    }
    
    /// Create a backend of the specified type
    pub fn create_backend(
        &self,
        backend_type: &str,
        config_dir: PathBuf,
        data_dir: PathBuf,
        database: Arc<Database>
    ) -> BlixardResult<Arc<dyn VmBackend>> {
        let factory = self.factories.get(backend_type)
            .ok_or_else(|| BlixardError::ConfigError(
                format!("Unknown VM backend type: '{}'. Available backends: {:?}", 
                    backend_type, self.list_available_backends())
            ))?;
        
        factory.create_backend(config_dir, data_dir, database)
    }
    
    /// List all available backend types
    pub fn list_available_backends(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
    
    /// Get information about all registered backends
    pub fn get_backend_info(&self) -> Vec<(String, String)> {
        self.factories.values()
            .map(|f| (f.backend_type().to_string(), f.description().to_string()))
            .collect()
    }
}

impl Default for VmBackendRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        
        // Register the built-in mock backend
        registry.register(Arc::new(MockVmBackendFactory));
        
        registry
    }
}

/// Factory for the mock VM backend (built-in for testing)
pub struct MockVmBackendFactory;

impl VmBackendFactory for MockVmBackendFactory {
    fn create_backend(
        &self,
        _config_dir: PathBuf,
        _data_dir: PathBuf,
        database: Arc<Database>
    ) -> BlixardResult<Arc<dyn VmBackend>> {
        Ok(Arc::new(MockVmBackend::new(database)))
    }
    
    fn backend_type(&self) -> &'static str {
        "mock"
    }
    
    fn description(&self) -> &'static str {
        "Mock VM backend for testing (no actual VMs created)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_vm_backend_registry() {
        let mut registry = VmBackendRegistry::new();
        
        // Should start empty (no default mock in new())
        assert!(registry.list_available_backends().is_empty());
        
        // Register mock backend
        registry.register(Arc::new(MockVmBackendFactory));
        assert_eq!(registry.list_available_backends(), vec!["mock"]);
        
        // Test default() includes mock
        let default_registry = VmBackendRegistry::default();
        assert!(default_registry.list_available_backends().contains(&"mock"));
    }
    
    #[tokio::test]
    async fn test_mock_backend_creation() {
        let registry = VmBackendRegistry::default();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(db_path).unwrap());
        
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        // Should successfully create mock backend
        let backend = registry.create_backend(
            "mock",
            config_dir,
            data_dir,
            database
        ).unwrap();
        
        // Test basic operations
        let vm_config = crate::types::VmConfig {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory: 512,
        };
        
        backend.create_vm(&vm_config, 1).await.unwrap();
        backend.start_vm("test-vm").await.unwrap();
        backend.stop_vm("test-vm").await.unwrap();
        backend.delete_vm("test-vm").await.unwrap();
    }
    
    #[test]
    fn test_unknown_backend_error() {
        let registry = VmBackendRegistry::default();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(db_path).unwrap());
        
        let result = registry.create_backend(
            "unknown",
            PathBuf::new(),
            PathBuf::new(),
            database
        );
        
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown VM backend type"));
    }
}