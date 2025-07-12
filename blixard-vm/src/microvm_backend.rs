use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use blixard_core::{
    error::{BlixardError, BlixardResult},
    common::error_context::StorageContext,
    types::{VmConfig as CoreVmConfig, VmState, VmStatus},
    vm_backend::VmBackend,
    vm_health_types::{HealthCheckResult, HealthCheckType, VmHealthStatus},
    vm_state_persistence::{VmStatePersistence, VmPersistenceConfig},
};
use chrono::Utc;

use crate::{
    config_converter::VmConfigConverter,
    health_check_helpers::health_check_result,
    ip_pool::IpAddressPool,
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
    /// Configuration converter for VM configs
    config_converter: VmConfigConverter,
    /// In-memory cache of VM configurations
    vm_configs: Arc<RwLock<HashMap<String, vm_types::VmConfig>>>,
    /// Original VM metadata from core config
    vm_metadata: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// IP address pool manager for routed networking
    /// Currently unused pending automatic IP allocation implementation
    #[allow(dead_code)]
    ip_pool: Arc<RwLock<IpAddressPool>>,
    /// Optional Nix image store for P2P image distribution
    nix_image_store: Option<Arc<blixard_core::nix_image_store::NixImageStore>>,
    /// Health status tracking for VMs
    vm_health_status: Arc<RwLock<HashMap<String, VmHealthStatus>>>,
    /// VM state persistence manager
    vm_persistence: VmStatePersistence,
}

impl MicrovmBackend {
    /// Create a new microvm.nix backend
    pub fn new(
        config_dir: PathBuf,
        data_dir: PathBuf,
        database: std::sync::Arc<redb::Database>,
    ) -> BlixardResult<Self> {
        // Ensure directories exist
        std::fs::create_dir_all(&config_dir)
            .storage_context("create config directory")?;
        std::fs::create_dir_all(&data_dir)
            .storage_context("create data directory")?;

        // Create runtime directory for process manager
        let runtime_dir = data_dir.join("runtime");
        std::fs::create_dir_all(&runtime_dir)
            .storage_context("create runtime directory")?;

        // Create modules directory for flake generator
        let modules_dir = config_dir.join("modules");
        std::fs::create_dir_all(&modules_dir)
            .storage_context("create modules directory")?;

        let flake_generator = NixFlakeGenerator::new(config_dir.clone(), modules_dir)?;

        let process_manager = VmProcessManager::new(runtime_dir);

        // Initialize VM state persistence
        let vm_persistence = VmStatePersistence::new(database, VmPersistenceConfig::default());

        // Initialize IP pool and config converter
        let ip_pool = Arc::new(RwLock::new(IpAddressPool::new()));
        let config_converter = VmConfigConverter::new(ip_pool.clone());

        Ok(Self {
            config_dir,
            data_dir,
            flake_generator,
            process_manager,
            config_converter,
            vm_configs: Arc::new(RwLock::new(HashMap::new())),
            vm_metadata: Arc::new(RwLock::new(HashMap::new())),
            ip_pool,
            nix_image_store: None,
            vm_health_status: Arc::new(RwLock::new(HashMap::new())),
            vm_persistence,
        })
    }

    /// Set the Nix image store for P2P image distribution
    pub fn set_nix_image_store(
        &mut self,
        store: Arc<blixard_core::nix_image_store::NixImageStore>,
    ) {
        self.nix_image_store = Some(store);
    }

    /// Recover VMs from persisted state after node restart
    /// 
    /// Note: This method is deprecated. VM recovery is now handled by the VmManager
    /// which has proper access to the Arc<dyn VmBackend> required by the persistence layer.
    /// Use VmManager::recover_persisted_vms() instead.
    pub async fn recover_persisted_vms(&self) -> BlixardResult<()> {
        info!("VM recovery is now handled by VmManager - this method is deprecated");
        Ok(())
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
    /// Currently unused pending Nix image management implementation
    #[allow(dead_code)]
    async fn update_vm_config_with_image_path(
        &self,
        vm_name: &str,
        image_path: &PathBuf,
    ) -> BlixardResult<()> {
        let mut configs = self.vm_configs.write().await;

        if let Some(vm_config) = configs.get_mut(vm_name) {
            // Update the NixOS module to reference the downloaded image
            let image_module = vm_types::NixModule::Inline(format!(
                r#"
                # Use P2P-downloaded Nix image
                imports = [ {} ];
                
                # Override boot configuration for microVM
                boot.loader.grub.enable = false;
                boot.isContainer = true;
            "#,
                image_path.display()
            ));

            // Replace or add the image module
            vm_config.nixos_modules = vec![image_module];

            // Also update the flake on disk
            let flake_dir = self.get_vm_flake_dir(vm_name);
            self.flake_generator.write_flake(vm_config, &flake_dir)?;

            info!(
                "Updated VM {} configuration to use image at {:?}",
                vm_name, image_path
            );
        }

        Ok(())
    }

    /// Perform an HTTP health check
    async fn perform_http_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        url: &str,
        expected_status: u16,
        timeout_secs: u64,
        headers: Option<&Vec<(String, String)>>,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        // Replace localhost/127.0.0.1 with VM IP if needed
        let vm_url = if url.contains("localhost") || url.contains("127.0.0.1") {
            if let Some(vm_ip) = self.get_vm_ip(vm_name).await? {
                url.replace("localhost", &vm_ip)
                    .replace("127.0.0.1", &vm_ip)
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        };

        // Create HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(tokio::time::Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        // Build request
        let mut request = client.get(&vm_url);

        // Add custom headers if provided
        if let Some(headers_vec) = headers {
            for (key, value) in headers_vec {
                request = request.header(key, value);
            }
        }

        // Send request
        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let success = status.as_u16() == expected_status;
                let message = format!(
                    "HTTP check returned status {} (expected {})",
                    status.as_u16(),
                    expected_status
                );
                let error = if !success {
                    Some(format!("Unexpected status code: {}", status))
                } else {
                    None
                };

                Ok(health_check_result(check_name, start_time, timestamp)
                    .result(success, message, error))
            }
            Err(e) => {
                let message = format!("HTTP check failed: {}", e);
                Ok(health_check_result(check_name, start_time, timestamp)
                    .failure(message, Some(e.to_string())))
            }
        }
    }

    /// Perform a TCP health check
    async fn perform_tcp_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        address: &str,
        timeout_secs: u64,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        // Replace localhost/127.0.0.1 with VM IP if needed
        let vm_address = if address.contains("localhost") || address.contains("127.0.0.1") {
            if let Some(vm_ip) = self.get_vm_ip(vm_name).await? {
                address
                    .replace("localhost", &vm_ip)
                    .replace("127.0.0.1", &vm_ip)
            } else {
                address.to_string()
            }
        } else {
            address.to_string()
        };

        let timeout = tokio::time::Duration::from_secs(timeout_secs);

        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&vm_address)).await {
            Ok(Ok(_stream)) => {
                let message = format!("TCP connection to {} successful", vm_address);
                Ok(health_check_result(check_name, start_time, timestamp).success(message))
            }
            Ok(Err(e)) => {
                let message = format!("TCP connection to {} failed: {}", vm_address, e);
                Ok(health_check_result(check_name, start_time, timestamp)
                    .failure(message, Some(e.to_string())))
            }
            Err(_) => {
                let message = format!(
                    "TCP connection to {} timed out after {} seconds",
                    vm_address, timeout_secs
                );
                Ok(health_check_result(check_name, start_time, timestamp)
                    .failure(message, Some("Connection timeout".to_string())))
            }
        }
    }

    /// Perform a script health check by executing a command in the VM
    async fn perform_script_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        command: &str,
        args: &[String],
        expected_exit_code: i32,
        timeout_secs: u64,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        match self
            .execute_command_in_vm(vm_name, command, args, timeout_secs)
            .await
        {
            Ok((exit_code, stdout, stderr)) => {
                let success = exit_code == expected_exit_code;

                Ok(HealthCheckResult {
                    check_name: check_name.to_string(),
                    success,
                    message: format!(
                        "Script exited with code {} (expected {}). stdout: {}, stderr: {}",
                        exit_code,
                        expected_exit_code,
                        stdout.trim(),
                        stderr.trim()
                    ),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    timestamp_secs: timestamp
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    error: if !success {
                        Some(format!("Unexpected exit code: {}", exit_code))
                    } else {
                        None
                    },
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
                })
            }
            Err(e) => Ok(HealthCheckResult {
                check_name: check_name.to_string(),
                success: false,
                message: format!("Failed to execute script: {}", e),
                duration_ms: start_time.elapsed().as_millis() as u64,
                timestamp_secs: timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                error: Some(e.to_string()),
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
            }),
        }
    }

    /// Perform a console health check
    async fn perform_console_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        _healthy_pattern: &str,
        _unhealthy_pattern: Option<&String>,
        _timeout_secs: u64,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        let socket_path = format!("/tmp/{}-console.sock", vm_name);

        // Check if console socket exists
        if tokio::fs::metadata(&socket_path).await.is_err() {
            return Ok(HealthCheckResult {
                check_name: check_name.to_string(),
                success: false,
                message: format!("Console socket not found at {}", socket_path),
                duration_ms: start_time.elapsed().as_millis() as u64,
                timestamp_secs: timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                error: Some("Console not accessible".to_string()),
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
            });
        }

        // For now, just check socket existence
        // TODO: Implement actual console reading with pattern matching
        Ok(HealthCheckResult {
            check_name: check_name.to_string(),
            success: true,
            message: "Console socket is accessible".to_string(),
            duration_ms: start_time.elapsed().as_millis() as u64,
            timestamp_secs: timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: None,
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
        })
    }

    /// Perform a process health check using systemctl
    async fn perform_process_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        process_name: &str,
        min_instances: u32,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        // Check systemd service status
        let service_name = format!("blixard-vm-{}", vm_name);

        let output = tokio::process::Command::new("systemctl")
            .args(&["--user", "status", &service_name])
            .output()
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "process_check".to_string(),
                details: format!("Failed to check systemd status: {}", e),
            })?;

        let status_output = String::from_utf8_lossy(&output.stdout);
        let is_active = output.status.success() || status_output.contains("active (running)");

        // Count process instances if service is active
        let instance_count = if is_active {
            // For now, assume 1 instance for systemd service
            // TODO: Implement actual process counting inside VM
            1
        } else {
            0
        };

        let success = instance_count >= min_instances;

        Ok(HealthCheckResult {
            check_name: check_name.to_string(),
            success,
            message: format!(
                "Found {} instances of process '{}' (minimum required: {})",
                instance_count, process_name, min_instances
            ),
            duration_ms: start_time.elapsed().as_millis() as u64,
            timestamp_secs: timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: if !success {
                Some(format!(
                    "Insufficient process instances: {} < {}",
                    instance_count, min_instances
                ))
            } else {
                None
            },
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: false,
            recovery_action: None,
        })
    }

    /// Perform a guest agent health check
    async fn perform_guest_agent_health_check(
        &self,
        _vm_name: &str,
        check_name: &str,
        _timeout_secs: u64,
        start_time: std::time::Instant,
        timestamp: std::time::SystemTime,
    ) -> BlixardResult<HealthCheckResult> {
        // Guest agent not yet implemented
        // TODO: Implement guest agent protocol
        Ok(HealthCheckResult {
            check_name: check_name.to_string(),
            success: false,
            message: "Guest agent health check not yet implemented".to_string(),
            duration_ms: start_time.elapsed().as_millis() as u64,
            timestamp_secs: timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: Some("NotImplemented".to_string()),
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
        })
    }
}

#[async_trait]
impl VmBackend for MicrovmBackend {
    async fn create_vm(&self, config: &CoreVmConfig, _node_id: u64) -> BlixardResult<()> {
        info!("Creating microVM '{}'", config.name);

        // Check if we need to download a Nix image
        if let Some(metadata) = &config.metadata {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                if let Some(_store) = &self.nix_image_store {
                    info!(
                        "VM {} requires Nix image {}, checking availability",
                        config.name, nix_image_id
                    );

                    // TODO: Implement NixImageStore integration
                    // Check if we already have the image
                    // let images = store.list_images().await?;
                    // let has_image = images.iter().any(|img| &img.id == nix_image_id);

                    // if !has_image {
                    //     info!(
                    //         "Nix image {} not found locally, will download on start",
                    //         nix_image_id
                    //     );
                    // } else {
                    //     info!("Nix image {} already available locally", nix_image_id);
                    // }
                    info!("Nix image {} check skipped (TODO: implement)", nix_image_id);
                } else {
                    warn!(
                        "VM {} requires Nix image {} but no image store configured",
                        config.name, nix_image_id
                    );
                }
            }
        }

        // Convert to our enhanced config with allocated IP address
        let vm_config = self.config_converter.convert_config(config).await?;

        // Generate and write the flake
        let flake_dir = self.get_vm_flake_dir(&config.name);
        self.flake_generator.write_flake(&vm_config, &flake_dir)?;

        // Store the configuration including metadata
        {
            let mut configs = self.vm_configs.write().await;
            configs.insert(config.name.clone(), vm_config);

            let mut vm_metadata = self.vm_metadata.write().await;
            let mut metadata = config.metadata.clone().unwrap_or_default();
            
            // Track if IP was centrally allocated
            if config.ip_address.is_some() {
                metadata.insert("centrally_allocated_ip".to_string(), "true".to_string());
            }
            
            vm_metadata.insert(config.name.clone(), metadata);
        }

        // Persist VM state to database
        let vm_state = VmState {
            name: config.name.clone(),
            config: config.clone(),
            status: VmStatus::Creating,
            node_id: _node_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        self.vm_persistence.persist_vm_state(&vm_state).await?;

        info!(
            "Successfully created microVM '{}' configuration",
            config.name
        );
        Ok(())
    }

    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Starting microVM '{}'", name);

        // Ensure Nix images are available if needed
        self.ensure_nix_image_availability_impl(name).await?;

        // Validate VM configuration and get required data
        let (flake_dir, vm_config) = self.validate_vm_for_start_impl(name).await?;

        // Start the VM using user systemd service management for better logging
        self.process_manager
            .start_vm_systemd(name, &flake_dir, &vm_config)
            .await?;

        // Update VM status to Running in persistence
        self.vm_persistence.update_vm_status(name, VmStatus::Running).await?;

        info!("Successfully started microVM '{}'", name);
        Ok(())
    }

    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping microVM '{}'", name);

        // Stop the VM using user systemd service management
        self.process_manager.stop_vm_systemd(name).await?;

        // Update VM status to Stopped in persistence
        self.vm_persistence.update_vm_status(name, VmStatus::Stopped).await?;

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
                // Only release IP if it was locally allocated (not centrally managed)
                // Check if this VM was created with a pre-allocated IP
                let metadata = self.vm_metadata.read().await;
                let has_central_ip = metadata.get(name)
                    .and_then(|m| m.get("centrally_allocated_ip"))
                    .is_some();
                
                if !has_central_ip {
                    // Find and release locally allocated IP address
                    for network in &vm_config.networks {
                        if let vm_types::NetworkConfig::Routed { ip, .. } = network {
                            self.config_converter.release_vm_ip(ip).await;
                            info!("Released locally allocated IP address {} for VM '{}'", ip, name);
                        }
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

        // Remove VM state from persistence
        self.vm_persistence.remove_vm_state(name).await?;

        info!("Successfully deleted microVM '{}'", name);
        Ok(())
    }

    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        debug!("Updating microVM '{}' status to {:?}", name, status);

        // Update VM status in persistence
        self.vm_persistence.update_vm_status(name, status).await?;

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
                ip_address: None, // TODO: Extract from VM config if available
                tenant_id: "default".to_string(), // TODO: Extract from VM config
                metadata: None,
                anti_affinity: None,
                priority: 500,      // Default medium priority
                preemptible: false, // Default to non-preemptible
                locality_preference: Default::default(),
                health_check_config: None, // TODO: Add health check config support
            };

            // Get current status
            let status = self.get_vm_status(name).await?.unwrap_or(VmStatus::Stopped);

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
                                ip_address: None, // TODO: Extract from VM config if available
                                tenant_id: "default".to_string(), // TODO: Extract from VM config
                                metadata: None,
                                anti_affinity: None,
                                priority: 500,      // Default medium priority
                                preemptible: false, // Default to non-preemptible
                                locality_preference: Default::default(),
                                health_check_config: None, // TODO: Add health check config support
                            };

                            let status = self
                                .get_vm_status(vm_name)
                                .await?
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

    async fn perform_health_check(
        &self,
        vm_name: &str,
        check_name: &str,
        check_type: &HealthCheckType,
    ) -> BlixardResult<HealthCheckResult> {
        let start_time = std::time::Instant::now();
        let timestamp = std::time::SystemTime::now();

        // First check if VM is running
        let vm_status = self.get_vm_status(vm_name).await?;
        if vm_status != Some(VmStatus::Running) {
            return Ok(HealthCheckResult {
                check_name: check_name.to_string(),
                success: false,
                message: format!("VM is not running (status: {:?})", vm_status),
                duration_ms: start_time.elapsed().as_millis() as u64,
                timestamp_secs: timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                error: None,
                    priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                    critical: false,
                    recovery_action: None,
            });
        }

        // Perform the specific health check
        match check_type {
            HealthCheckType::Http {
                url,
                expected_status,
                timeout_secs,
                headers,
            } => {
                self.perform_http_health_check(
                    vm_name,
                    check_name,
                    url,
                    *expected_status,
                    *timeout_secs,
                    headers.as_ref(),
                    start_time,
                    timestamp,
                )
                .await
            }
            HealthCheckType::Tcp {
                address,
                timeout_secs,
            } => {
                self.perform_tcp_health_check(
                    vm_name,
                    check_name,
                    address,
                    *timeout_secs,
                    start_time,
                    timestamp,
                )
                .await
            }
            HealthCheckType::Script {
                command,
                args,
                expected_exit_code,
                timeout_secs,
            } => {
                self.perform_script_health_check(
                    vm_name,
                    check_name,
                    command,
                    args,
                    *expected_exit_code,
                    *timeout_secs,
                    start_time,
                    timestamp,
                )
                .await
            }
            HealthCheckType::Console {
                healthy_pattern,
                unhealthy_pattern,
                timeout_secs,
            } => {
                self.perform_console_health_check(
                    vm_name,
                    check_name,
                    healthy_pattern,
                    unhealthy_pattern.as_ref(),
                    *timeout_secs,
                    start_time,
                    timestamp,
                )
                .await
            }
            HealthCheckType::Process {
                process_name,
                min_instances,
            } => {
                self.perform_process_health_check(
                    vm_name,
                    check_name,
                    process_name,
                    *min_instances,
                    start_time,
                    timestamp,
                )
                .await
            }
            HealthCheckType::GuestAgent { timeout_secs } => {
                self.perform_guest_agent_health_check(
                    vm_name,
                    check_name,
                    *timeout_secs,
                    start_time,
                    timestamp,
                )
                .await
            }
        }
    }

    async fn get_vm_health_status(&self, vm_name: &str) -> BlixardResult<Option<VmHealthStatus>> {
        let health_status = self.vm_health_status.read().await;
        Ok(health_status.get(vm_name).cloned())
    }

    async fn is_console_accessible(&self, vm_name: &str) -> BlixardResult<bool> {
        // Check if VM is running
        let vm_status = self.get_vm_status(vm_name).await?;
        if vm_status != Some(VmStatus::Running) {
            return Ok(false);
        }

        // Check if console socket exists
        let socket_path = format!("/tmp/{}-console.sock", vm_name);
        Ok(tokio::fs::metadata(&socket_path).await.is_ok())
    }

    async fn execute_command_in_vm(
        &self,
        vm_name: &str,
        command: &str,
        args: &[String],
        timeout_secs: u64,
    ) -> BlixardResult<(i32, String, String)> {
        // Get VM IP address
        let vm_ip =
            self.get_vm_ip(vm_name)
                .await?
                .ok_or_else(|| BlixardError::VmOperationFailed {
                    operation: "execute_command".to_string(),
                    details: format!("VM '{}' IP address not found", vm_name),
                })?;

        // Execute command via SSH
        let ssh_args: Vec<String> = vec![
            "-o".to_string(),
            "ConnectTimeout=5".to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(),
            "UserKnownHostsFile=/dev/null".to_string(),
            format!("root@{}", vm_ip),
            command.to_string(),
        ];

        let mut full_args = ssh_args;
        for arg in args {
            full_args.push(arg.clone());
        }

        let timeout = tokio::time::Duration::from_secs(timeout_secs);
        let output = tokio::time::timeout(
            timeout,
            tokio::process::Command::new("ssh")
                .args(&full_args)
                .output(),
        )
        .await
        .map_err(|_| BlixardError::VmOperationFailed {
            operation: "execute_command".to_string(),
            details: format!("Command timed out after {} seconds", timeout_secs),
        })?
        .map_err(|e| BlixardError::VmOperationFailed {
            operation: "execute_command".to_string(),
            details: format!("Failed to execute SSH command: {}", e),
        })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((exit_code, stdout, stderr))
    }

    async fn update_vm_health_status(
        &self,
        vm_name: &str,
        health_status: VmHealthStatus,
    ) -> BlixardResult<()> {
        let mut health_statuses = self.vm_health_status.write().await;
        health_statuses.insert(vm_name.to_string(), health_status);
        Ok(())
    }
}

impl MicrovmBackend {
    /// Ensure Nix images are available for VM startup
    async fn ensure_nix_image_availability_impl(&self, vm_name: &str) -> BlixardResult<()> {
        // Check if this VM uses a Nix image that needs to be downloaded
        if let Some(metadata) = self.vm_metadata.read().await.get(vm_name) {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                if let Some(_store) = &self.nix_image_store {
                    info!(
                        "VM {} uses Nix image {}, ensuring availability",
                        vm_name, nix_image_id
                    );

                    // TODO: Implement NixImageStore integration
                    // Check if image is already available locally
                    // let images = store.list_images().await?;
                    // if images.iter().any(|img| &img.id == nix_image_id) {
                    //     info!("Nix image {} is already cached locally", nix_image_id);
                    //     return Ok(());
                    // }

                    // Need to download from P2P network
                    info!("Nix image {} not found locally, downloading from P2P network (TODO: implement)", nix_image_id);
                    // TODO: Implement image download
                    // store.download_image(nix_image_id, None).await?;
                }
            }
        }
        Ok(())
    }

    /// Download and verify a Nix image
    /// Currently unused pending P2P image distribution implementation
    #[allow(dead_code)]
    async fn download_and_verify_nix_image_impl(
        &self,
        vm_name: &str,
        image_id: &str,
    ) -> BlixardResult<PathBuf> {
        if let Some(store) = &self.nix_image_store {
            // Download image from P2P network
            info!("Downloading Nix image {} for VM {}", image_id, vm_name);
            
            // Use the P2P image store to download
            let (store_path, _stats) = store.download_image(image_id, None).await?;
            
            // Verify image integrity
            info!("Verifying integrity of Nix image {}", image_id);
            let is_valid = store.verify_image(image_id).await?;
            if !is_valid {
                return Err(BlixardError::Internal {
                    message: format!("Downloaded image {} failed verification", image_id),
                });
            }
            
            Ok(store_path)
        } else {
            Err(BlixardError::Internal {
                message: "Nix image store not configured".to_string(),
            })
        }
    }

    /// Validate VM before starting
    async fn validate_vm_for_start_impl(&self, name: &str) -> BlixardResult<(PathBuf, vm_types::VmConfig)> {
        // Get the flake directory
        let flake_dir = self.get_vm_flake_dir(name);
        if !flake_dir.exists() {
            return Err(BlixardError::NotFound {
                resource: format!("VM configuration directory: {}", flake_dir.display()),
            });
        }

        // Read the config
        let config = self
            .vm_configs
            .read()
            .await
            .get(name)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("VM configuration: {}", name),
            })?
            .clone();

        Ok((flake_dir, config))
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
        _database: std::sync::Arc<redb::Database>,
    ) -> BlixardResult<std::sync::Arc<dyn blixard_core::vm_backend::VmBackend>> {
        let backend = MicrovmBackend::new(config_dir, data_dir, _database)?;
        Ok(std::sync::Arc::new(backend))
    }

    fn backend_type(&self) -> &'static str {
        "microvm"
    }

    fn description(&self) -> &'static str {
        "MicroVM backend using microvm.nix for lightweight NixOS VMs"
    }
}
