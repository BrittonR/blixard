use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};

use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig as CoreVmConfig, VmStatus},
    error::{BlixardError, BlixardResult},
};

use crate::{
    nix_generator::NixFlakeGenerator,
    process_manager::VmProcessManager,
    types as vm_types,
};

/// VM backend implementation using microvm.nix
/// 
/// This backend integrates with microvm.nix to provide actual VM lifecycle
/// management. It handles VM creation, startup, shutdown, and monitoring.
/// 
/// ## Implementation Notes
/// 
/// - VMs are defined as Nix flakes and built using the Nix CLI
/// - VM processes are managed by VmProcessManager
/// - Each VM gets its own flake directory with generated configuration
/// - Resource allocation and networking is handled through microvm.nix configuration
/// - Health monitoring is performed by querying VM process status
/// 
/// ## Future Work
/// 
/// - Resource constraint enforcement (CPU, memory limits)
/// - VM migration between nodes
/// - Snapshot and backup functionality
/// - Integration with container registries for VM images
pub struct MicrovmBackend {
    /// Directory where VM configurations are stored
    config_dir: PathBuf,
    /// Directory where VM runtime data is stored
    data_dir: PathBuf,
    /// Nix flake generator for creating VM configurations
    flake_generator: NixFlakeGenerator,
    /// VM process manager for lifecycle operations
    process_manager: VmProcessManager,
    /// In-memory cache of VM configurations
    vm_configs: Arc<RwLock<HashMap<String, vm_types::VmConfig>>>,
}

impl MicrovmBackend {
    /// Create a new microvm.nix backend
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> BlixardResult<Self> {
        // Ensure directories exist
        std::fs::create_dir_all(&config_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        
        // Create runtime directory for process manager
        let runtime_dir = data_dir.join("runtime");
        std::fs::create_dir_all(&runtime_dir)?;
        
        // Create modules directory for flake generator
        let modules_dir = config_dir.join("modules");
        std::fs::create_dir_all(&modules_dir)?;
        
        let flake_generator = NixFlakeGenerator::new(
            config_dir.clone(),
            modules_dir,
        )?;
        
        let process_manager = VmProcessManager::new(runtime_dir);
        
        Ok(Self {
            config_dir,
            data_dir,
            flake_generator,
            process_manager,
            vm_configs: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Convert core VM config to our enhanced VM config
    fn convert_config(&self, core_config: &CoreVmConfig) -> vm_types::VmConfig {
        vm_types::VmConfig {
            name: core_config.name.clone(),
            hypervisor: vm_types::Hypervisor::Qemu,
            vcpus: core_config.vcpus,
            memory: core_config.memory,
            networks: vec![
                vm_types::NetworkConfig::User
            ],
            volumes: vec![
                vm_types::VolumeConfig::RootDisk {
                    size: 10240, // 10GB default
                }
            ],
            nixos_modules: vec![],
            flake_modules: vec![],
            kernel: None,
            init_command: None,
        }
    }
    
    /// Get the flake directory for a VM
    fn get_vm_flake_dir(&self, name: &str) -> PathBuf {
        self.config_dir.join("vms").join(name)
    }
}

#[async_trait]
impl VmBackend for MicrovmBackend {
    async fn create_vm(&self, config: &CoreVmConfig, _node_id: u64) -> BlixardResult<()> {
        info!("Creating microVM '{}'", config.name);
        
        // Convert to our enhanced config
        let vm_config = self.convert_config(config);
        
        // Generate and write the flake
        let flake_dir = self.get_vm_flake_dir(&config.name);
        self.flake_generator.write_flake(&vm_config, &flake_dir)?;
        
        // Store the configuration
        {
            let mut configs = self.vm_configs.write().await;
            configs.insert(config.name.clone(), vm_config);
        }
        
        info!("Successfully created microVM '{}' configuration", config.name);
        Ok(())
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Starting microVM '{}'", name);
        
        // Get the flake directory
        let flake_dir = self.get_vm_flake_dir(name);
        if !flake_dir.exists() {
            return Err(BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("VM '{}' configuration not found", name),
            });
        }
        
        // Start the VM using the process manager
        self.process_manager.start_vm(name, &flake_dir).await?;
        
        info!("Successfully started microVM '{}'", name);
        Ok(())
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping microVM '{}'", name);
        
        // Stop the VM using the process manager
        self.process_manager.stop_vm(name).await?;
        
        info!("Successfully stopped microVM '{}'", name);
        Ok(())
    }
    
    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Deleting microVM '{}'", name);
        
        // Stop the VM if it's running
        if let Some(status) = self.get_vm_status(name).await? {
            if status == VmStatus::Running {
                self.stop_vm(name).await?;
            }
        }
        
        // Remove the flake directory
        let flake_dir = self.get_vm_flake_dir(name);
        if flake_dir.exists() {
            tokio::fs::remove_dir_all(&flake_dir).await?;
        }
        
        // Remove VM data directory
        let vm_data_dir = self.data_dir.join(name);
        if vm_data_dir.exists() {
            tokio::fs::remove_dir_all(&vm_data_dir).await?;
        }
        
        // Remove from configuration cache
        {
            let mut configs = self.vm_configs.write().await;
            configs.remove(name);
        }
        
        info!("Successfully deleted microVM '{}'", name);
        Ok(())
    }
    
    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        debug!("Updating microVM '{}' status to {:?}", name, status);
        
        // The process manager tracks the actual status
        // This method is primarily for distributed state management
        // and doesn't need to do anything in this implementation
        
        Ok(())
    }
    
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        // Check with the process manager
        self.process_manager.get_vm_status(name).await
    }
    
    async fn list_vms(&self) -> BlixardResult<Vec<(CoreVmConfig, VmStatus)>> {
        let mut result = Vec::new();
        
        // Get all configured VMs
        let configs = self.vm_configs.read().await;
        
        for (name, vm_config) in configs.iter() {
            // Convert back to core config
            let core_config = CoreVmConfig {
                name: name.clone(),
                config_path: self.get_vm_flake_dir(name).to_string_lossy().to_string(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
            };
            
            // Get current status
            let status = self.get_vm_status(name).await?
                .unwrap_or(VmStatus::Stopped);
            
            result.push((core_config, status));
        }
        
        // Also check for VMs that exist on disk but not in cache
        let vms_dir = self.config_dir.join("vms");
        if vms_dir.exists() {
            let mut dir_entries = tokio::fs::read_dir(&vms_dir).await?;
            
            while let Some(entry) = dir_entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(vm_name) = path.file_name().and_then(|n| n.to_str()) {
                        if !configs.contains_key(vm_name) {
                            // Found a VM that's not in our cache
                            // Create a basic config for it
                            let core_config = CoreVmConfig {
                                name: vm_name.to_string(),
                                config_path: path.to_string_lossy().to_string(),
                                vcpus: 1,
                                memory: 512,
                            };
                            
                            let status = self.get_vm_status(vm_name).await?
                                .unwrap_or(VmStatus::Stopped);
                            
                            result.push((core_config, status));
                        }
                    }
                }
            }
        }
        
        Ok(result)
    }
}

/// Factory for creating MicrovmBackend instances
/// 
/// This factory implements the VmBackendFactory trait from blixard-core,
/// allowing the MicrovmBackend to be registered and created through the
/// factory pattern for modular VM backend architecture.
pub struct MicrovmBackendFactory;

impl blixard_core::vm_backend::VmBackendFactory for MicrovmBackendFactory {
    fn create_backend(
        &self,
        config_dir: std::path::PathBuf,
        data_dir: std::path::PathBuf,
        _database: std::sync::Arc<redb::Database>
    ) -> BlixardResult<std::sync::Arc<dyn blixard_core::vm_backend::VmBackend>> {
        let backend = MicrovmBackend::new(config_dir, data_dir)?;
        Ok(std::sync::Arc::new(backend))
    }
    
    fn backend_type(&self) -> &'static str {
        "microvm"
    }
    
    fn description(&self) -> &'static str {
        "MicroVM backend using microvm.nix for lightweight NixOS VMs"
    }
}