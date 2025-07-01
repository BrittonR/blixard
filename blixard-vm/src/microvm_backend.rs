use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};
use std::net::Ipv4Addr;

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

/// IP address pool manager for routed VM networking
/// 
/// Manages IP address allocation within a subnet for VM instances.
/// Default subnet is 10.0.0.0/24 with gateway at 10.0.0.1.
/// VM IPs are allocated from 10.0.0.10 onwards.
#[derive(Debug, Clone)]
struct IpAddressPool {
    /// The subnet CIDR (e.g., "10.0.0.0/24")
    subnet: String,
    /// Gateway IP address (e.g., "10.0.0.1")
    gateway: Ipv4Addr,
    /// Network base address (e.g., 10.0.0.0)
    network_base: Ipv4Addr,
    /// Subnet mask bits (e.g., 24 for /24)
    prefix_len: u8,
    /// Start of VM allocation range (e.g., 10.0.0.10)
    allocation_start: Ipv4Addr,
    /// Currently allocated IP addresses
    allocated_ips: HashSet<Ipv4Addr>,
    /// Next IP to try for allocation
    next_ip: Ipv4Addr,
}

impl IpAddressPool {
    /// Create a new IP address pool with default subnet 10.0.0.0/24
    fn new() -> Self {
        let gateway = Ipv4Addr::new(10, 0, 0, 1);
        let network_base = Ipv4Addr::new(10, 0, 0, 0);
        let allocation_start = Ipv4Addr::new(10, 0, 0, 10);
        
        Self {
            subnet: "10.0.0.0/24".to_string(),
            gateway,
            network_base,
            prefix_len: 24,
            allocation_start,
            allocated_ips: HashSet::new(),
            next_ip: allocation_start,
        }
    }
    
    /// Allocate a new IP address from the pool
    fn allocate_ip(&mut self) -> BlixardResult<Ipv4Addr> {
        let max_attempts = 254; // Maximum IPs in /24 subnet
        let mut attempts = 0;
        
        while attempts < max_attempts {
            let candidate_ip = self.next_ip;
            
            // Check if IP is available
            if !self.allocated_ips.contains(&candidate_ip) && 
               candidate_ip != self.gateway &&
               self.is_ip_in_subnet(candidate_ip) {
                
                self.allocated_ips.insert(candidate_ip);
                self.advance_next_ip();
                info!("Allocated IP address: {}", candidate_ip);
                return Ok(candidate_ip);
            }
            
            self.advance_next_ip();
            attempts += 1;
        }
        
        Err(BlixardError::VmOperationFailed {
            operation: "allocate_ip".to_string(),
            details: format!("No available IP addresses in subnet {}", self.subnet),
        })
    }
    
    /// Release an IP address back to the pool
    fn release_ip(&mut self, ip: Ipv4Addr) {
        if self.allocated_ips.remove(&ip) {
            info!("Released IP address: {}", ip);
        } else {
            warn!("Attempted to release unallocated IP: {}", ip);
        }
    }
    
    /// Generate a unique MAC address for a VM
    fn generate_mac_address(&self, vm_name: &str) -> String {
        // Generate a deterministic MAC address based on VM name
        // Use a hash of the VM name to ensure uniqueness
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        vm_name.hash(&mut hasher);
        let hash = hasher.finish();
        
        // Use hash to generate last 3 octets of MAC
        // First 3 octets are 02:00:00 (locally administered)
        format!("02:00:00:{:02x}:{:02x}:{:02x}", 
                (hash >> 16) & 0xff,
                (hash >> 8) & 0xff, 
                hash & 0xff)
    }
    
    /// Check if an IP address is within the subnet range
    fn is_ip_in_subnet(&self, ip: Ipv4Addr) -> bool {
        let ip_num = u32::from(ip);
        let network_num = u32::from(self.network_base);
        let mask = !0u32 << (32 - self.prefix_len);
        
        (ip_num & mask) == (network_num & mask) &&
        ip_num > u32::from(self.allocation_start)
    }
    
    /// Advance to the next IP address candidate
    fn advance_next_ip(&mut self) {
        let next_num = u32::from(self.next_ip) + 1;
        self.next_ip = Ipv4Addr::from(next_num);
        
        // Wrap around if we go past the allocation range
        if !self.is_ip_in_subnet(self.next_ip) {
            self.next_ip = self.allocation_start;
        }
    }
    
    /// Get the gateway IP address
    fn gateway(&self) -> Ipv4Addr {
        self.gateway
    }
    
    /// Get the subnet CIDR string
    fn subnet(&self) -> &str {
        &self.subnet
    }
}

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
    /// Original VM metadata from core config
    vm_metadata: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// IP address pool manager for routed networking
    ip_pool: Arc<RwLock<IpAddressPool>>,
    /// Optional Nix image store for P2P image distribution
    nix_image_store: Option<Arc<blixard_core::nix_image_store::NixImageStore>>,
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
            vm_metadata: Arc::new(RwLock::new(HashMap::new())),
            ip_pool: Arc::new(RwLock::new(IpAddressPool::new())),
            nix_image_store: None,
        })
    }
    
    /// Set the Nix image store for P2P image distribution
    pub fn set_nix_image_store(&mut self, store: Arc<blixard_core::nix_image_store::NixImageStore>) {
        self.nix_image_store = Some(store);
    }
    
    /// Allocate a unique IP address for a VM
    async fn allocate_vm_ip(&self, vm_name: &str) -> BlixardResult<(String, String, String, String)> {
        let mut ip_pool = self.ip_pool.write().await;
        
        // Allocate IP address
        let vm_ip = ip_pool.allocate_ip()?;
        
        // Generate MAC address
        let mac_address = ip_pool.generate_mac_address(vm_name);
        
        // Get network configuration
        let gateway = ip_pool.gateway().to_string();
        let subnet = ip_pool.subnet().to_string();
        
        Ok((vm_ip.to_string(), mac_address, gateway, subnet))
    }
    
    /// Release an allocated IP address
    async fn release_vm_ip(&self, ip: &str) {
        if let Ok(ip_addr) = ip.parse::<Ipv4Addr>() {
            let mut ip_pool = self.ip_pool.write().await;
            ip_pool.release_ip(ip_addr);
        } else {
            warn!("Invalid IP address format for release: {}", ip);
        }
    }
    
    /// Convert core VM config to our enhanced VM config with routed networking
    async fn convert_config(&self, core_config: &CoreVmConfig) -> BlixardResult<vm_types::VmConfig> {
        // Allocate IP address and generate network config for the VM
        let (vm_ip, mac_address, gateway, subnet) = self.allocate_vm_ip(&core_config.name).await?;
        
        // Extract VM index from IP address (last octet of 10.0.0.x)
        let vm_index = vm_ip.parse::<std::net::Ipv4Addr>()
            .map_err(|e| BlixardError::Internal {
                message: format!("Invalid IP address format: {}", e),
            })?
            .octets()[3] as u32;
        
        // Generate interface ID based on VM name
        let interface_id = format!("vm-{}", core_config.name);
        
        // Check if this VM is using a Nix image
        let (nixos_modules, kernel) = if let Some(metadata) = &core_config.metadata {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                // This VM is using a P2P-distributed Nix image
                info!("VM {} is using Nix image: {}", core_config.name, nix_image_id);
                
                // The actual download/preparation would happen in start_vm
                // For now, we'll set up the module to reference the image
                let nixos_module = vm_types::NixModule::Inline(format!(r#"
                    # Reference P2P-distributed Nix image
                    # Image ID: {}
                    # This will be resolved to actual paths during VM start
                    boot.loader.grub.enable = false;
                    boot.isContainer = true;
                "#, nix_image_id));
                
                (vec![nixos_module], None)
            } else {
                (vec![], None)
            }
        } else {
            (vec![], None)
        };
        
        Ok(vm_types::VmConfig {
            name: core_config.name.clone(),
            vm_index,
            hypervisor: vm_types::Hypervisor::Qemu,
            vcpus: core_config.vcpus,
            memory: core_config.memory,
            networks: vec![
                vm_types::NetworkConfig::Routed {
                    id: interface_id,
                    mac: mac_address,
                    ip: vm_ip,
                    gateway,
                    subnet,
                }
            ],
            volumes: vec![
                vm_types::VolumeConfig::RootDisk {
                    size: 10240, // 10GB default
                }
            ],
            nixos_modules,
            flake_modules: vec![],
            kernel,
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
    
    /// Get the IP address of a VM
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        let configs = self.vm_configs.read().await;
        
        if let Some(vm_config) = configs.get(name) {
            // Find routed network configuration
            for network in &vm_config.networks {
                if let vm_types::NetworkConfig::Routed { ip, .. } = network {
                    return Ok(Some(ip.clone()));
                }
            }
        }
        
        Ok(None)
    }
    
    /// Update VM configuration to use downloaded Nix image path
    async fn update_vm_config_with_image_path(
        &self,
        vm_name: &str,
        image_path: &PathBuf,
    ) -> BlixardResult<()> {
        let mut configs = self.vm_configs.write().await;
        
        if let Some(vm_config) = configs.get_mut(vm_name) {
            // Update the NixOS module to reference the downloaded image
            let image_module = vm_types::NixModule::Inline(format!(r#"
                # Use P2P-downloaded Nix image
                imports = [ {} ];
                
                # Override boot configuration for microVM
                boot.loader.grub.enable = false;
                boot.isContainer = true;
            "#, image_path.display()));
            
            // Replace or add the image module
            vm_config.nixos_modules = vec![image_module];
            
            // Also update the flake on disk
            let flake_dir = self.get_vm_flake_dir(vm_name);
            self.flake_generator.write_flake(vm_config, &flake_dir)?;
            
            info!("Updated VM {} configuration to use image at {:?}", vm_name, image_path);
        }
        
        Ok(())
    }
}

#[async_trait]
impl VmBackend for MicrovmBackend {
    async fn create_vm(&self, config: &CoreVmConfig, _node_id: u64) -> BlixardResult<()> {
        info!("Creating microVM '{}'", config.name);
        
        // Check if we need to download a Nix image
        if let Some(metadata) = &config.metadata {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                if let Some(store) = &self.nix_image_store {
                    info!("VM {} requires Nix image {}, checking availability", config.name, nix_image_id);
                    
                    // Check if we already have the image
                    let images = store.list_images().await?;
                    let has_image = images.iter().any(|img| &img.id == nix_image_id);
                    
                    if !has_image {
                        info!("Nix image {} not found locally, will download on start", nix_image_id);
                    } else {
                        info!("Nix image {} already available locally", nix_image_id);
                    }
                } else {
                    warn!("VM {} requires Nix image {} but no image store configured", config.name, nix_image_id);
                }
            }
        }
        
        // Convert to our enhanced config with allocated IP address
        let vm_config = self.convert_config(config).await?;
        
        // Generate and write the flake
        let flake_dir = self.get_vm_flake_dir(&config.name);
        self.flake_generator.write_flake(&vm_config, &flake_dir)?;
        
        // Store the configuration including metadata
        {
            let mut configs = self.vm_configs.write().await;
            configs.insert(config.name.clone(), vm_config);
            
            if let Some(metadata) = &config.metadata {
                let mut vm_metadata = self.vm_metadata.write().await;
                vm_metadata.insert(config.name.clone(), metadata.clone());
            }
        }
        
        info!("Successfully created microVM '{}' configuration", config.name);
        Ok(())
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Starting microVM '{}'", name);
        
        // Check if this VM uses a Nix image that needs to be downloaded
        if let Some(metadata) = self.vm_metadata.read().await.get(name) {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                if let Some(store) = &self.nix_image_store {
                    info!("VM {} uses Nix image {}, ensuring availability", name, nix_image_id);
                    
                    // Check if image is already available locally
                    let images = store.list_images().await?;
                    let has_image = images.iter().any(|img| &img.id == nix_image_id);
                    
                    if !has_image {
                        info!("Nix image {} not found locally, downloading...", nix_image_id);
                        
                        // Record cache miss
                        blixard_core::metrics_otel::record_p2p_cache_access(false, "vm_image");
                        
                        // Download the image
                        let vm_images_dir = self.data_dir.join("vm-images");
                        std::fs::create_dir_all(&vm_images_dir)?;
                        
                        let (image_path, stats) = store.download_image(
                            nix_image_id,
                            Some(&vm_images_dir),
                        ).await?;
                        
                        info!(
                            "Downloaded Nix image {} to {:?} ({} MB in {:?})",
                            nix_image_id,
                            image_path,
                            stats.bytes_transferred / 1_048_576,
                            stats.duration
                        );
                        
                        // Verify the downloaded image
                        let is_valid = store.verify_image(nix_image_id).await?;
                        if !is_valid {
                            return Err(BlixardError::VmOperationFailed {
                                operation: "start".to_string(),
                                details: format!("Downloaded image {} failed verification", nix_image_id),
                            });
                        }
                        
                        info!("Nix image {} verified successfully", nix_image_id);
                        
                        // Update the VM configuration to use the downloaded image path
                        self.update_vm_config_with_image_path(name, &image_path).await?;
                    } else {
                        info!("Nix image {} already available locally", nix_image_id);
                        
                        // Record cache hit
                        blixard_core::metrics_otel::record_p2p_cache_access(true, "vm_image");
                    }
                } else {
                    warn!("VM {} requires Nix image {} but no image store configured", name, nix_image_id);
                }
            }
        }
        
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
        
        // Release allocated IP address before removing config
        {
            let mut configs = self.vm_configs.write().await;
            if let Some(vm_config) = configs.get(name) {
                // Find and release IP address
                for network in &vm_config.networks {
                    if let vm_types::NetworkConfig::Routed { ip, .. } = network {
                        self.release_vm_ip(ip).await;
                        info!("Released IP address {} for VM '{}'", ip, name);
                    }
                }
            }
            configs.remove(name);
            
            // Also remove metadata
            let mut metadata = self.vm_metadata.write().await;
            metadata.remove(name);
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
                ip_address: None,  // TODO: Extract from VM config if available
                tenant_id: "default".to_string(),  // TODO: Extract from VM config
                metadata: None,
                anti_affinity: None,
                priority: 500,  // Default medium priority
                preemptible: false,  // Default to non-preemptible
                locality_preference: Default::default(),
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
                                ip_address: None,  // TODO: Extract from VM config if available
                                tenant_id: "default".to_string(),  // TODO: Extract from VM config
                                metadata: None,
                                anti_affinity: None,
                                priority: 500,  // Default medium priority
                                preemptible: false,  // Default to non-preemptible
                                locality_preference: Default::default(),
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
    
    async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        self.get_vm_ip(name).await
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