use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
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
    /// Port allocator for SSH forwarding
    allocated_ports: Arc<RwLock<HashSet<u16>>>,
    /// Next available SSH port
    next_ssh_port: Arc<RwLock<u16>>,
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
            allocated_ports: Arc::new(RwLock::new(HashSet::new())),
            next_ssh_port: Arc::new(RwLock::new(2222)), // Start from 2222
        })
    }
    
    /// Allocate a unique SSH port for a VM
    async fn allocate_ssh_port(&self) -> BlixardResult<u16> {
        let mut allocated_ports = self.allocated_ports.write().await;
        let mut next_port = self.next_ssh_port.write().await;
        
        // Find next available port starting from current next_port
        let mut port = *next_port;
        while allocated_ports.contains(&port) {
            port += 1;
            if port > 9999 { // Reasonable upper limit
                return Err(BlixardError::VmOperationFailed {
                    operation: "allocate_port".to_string(),
                    details: "No available SSH ports in range 2222-9999".to_string(),
                });
            }
        }
        
        allocated_ports.insert(port);
        *next_port = port + 1;
        
        Ok(port)
    }
    
    /// Release an allocated SSH port
    async fn release_ssh_port(&self, port: u16) {
        let mut allocated_ports = self.allocated_ports.write().await;
        allocated_ports.remove(&port);
    }
    
    /// Convert core VM config to our enhanced VM config with dynamic port allocation
    async fn convert_config(&self, core_config: &CoreVmConfig) -> BlixardResult<vm_types::VmConfig> {
        // Allocate SSH port for the VM
        let ssh_port = self.allocate_ssh_port().await?;
        
        Ok(vm_types::VmConfig {
            name: core_config.name.clone(),
            hypervisor: vm_types::Hypervisor::Qemu,
            vcpus: core_config.vcpus,
            memory: core_config.memory,
            networks: vec![
                vm_types::NetworkConfig::User {
                    ssh_port: Some(ssh_port),
                }
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
        })
    }
    
    /// Get the flake directory for a VM
    fn get_vm_flake_dir(&self, name: &str) -> PathBuf {
        self.config_dir.join("vms").join(name)
    }
    
    /// Connect to VM console for interactive access
    pub async fn connect_to_console(&self, name: &str) -> BlixardResult<()> {
        self.process_manager.connect_to_console(name).await
    }
}

#[async_trait]
impl VmBackend for MicrovmBackend {
    async fn create_vm(&self, config: &CoreVmConfig, _node_id: u64) -> BlixardResult<()> {
        info!("Creating microVM '{}'", config.name);
        
        // Convert to our enhanced config with allocated SSH port
        let vm_config = self.convert_config(config).await?;
        
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
        
        // Get the VM configuration
        let vm_configs = self.vm_configs.read().await;
        let vm_config = vm_configs.get(name)
            .ok_or_else(|| BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("VM '{}' configuration not found in memory", name),
            })?;
        
        // Start the VM using user systemd service management for better logging
        self.process_manager.start_vm_systemd(name, &flake_dir, vm_config).await?;
        
        info!("Successfully started microVM '{}'", name);
        Ok(())
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping microVM '{}'", name);
        
        // Stop the VM using user systemd service management
        self.process_manager.stop_vm_systemd(name).await?;
        
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
        
        // Release allocated SSH port before removing config
        {
            let mut configs = self.vm_configs.write().await;
            if let Some(vm_config) = configs.get(name) {
                // Find and release SSH port
                for network in &vm_config.networks {
                    if let vm_types::NetworkConfig::User { ssh_port: Some(port) } = network {
                        self.release_ssh_port(*port).await;
                        info!("Released SSH port {} for VM '{}'", port, name);
                    }
                }
            }
            configs.remove(name);
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
        // Check with the process manager using user systemd service management
        self.process_manager.get_vm_status_systemd(name).await
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