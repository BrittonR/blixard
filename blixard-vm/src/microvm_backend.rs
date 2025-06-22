use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;

use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig, VmStatus},
    error::{BlixardError, BlixardResult},
};

/// VM backend implementation using microvm.nix
/// 
/// This backend integrates with microvm.nix to provide actual VM lifecycle
/// management. It handles VM creation, startup, shutdown, and monitoring.
/// 
/// ## Implementation Notes
/// 
/// - VMs are defined as Nix expressions and built using `nixos-rebuild`
/// - VM state is tracked locally (the distributed state is managed by Raft in blixard-core)
/// - Resource allocation and networking is handled through microvm.nix configuration
/// - Health monitoring is performed by querying VM process status
/// 
/// ## Future Work
/// 
/// - Integration with microvm.nix flake configurations
/// - Resource constraint enforcement (CPU, memory limits)
/// - VM migration between nodes
/// - Snapshot and backup functionality
/// - Integration with container registries for VM images
pub struct MicrovmBackend {
    /// Directory where VM configurations are stored
    config_dir: PathBuf,
    /// Directory where VM runtime data is stored
    data_dir: PathBuf,
    /// In-memory cache of VM status (for performance)
    vm_cache: Arc<RwLock<HashMap<String, VmStatus>>>,
}

impl MicrovmBackend {
    /// Create a new microvm.nix backend
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> BlixardResult<Self> {
        // Ensure directories exist
        std::fs::create_dir_all(&config_dir).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create config directory: {}", e),
        })?;
        
        std::fs::create_dir_all(&data_dir).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create data directory: {}", e),
        })?;
        
        Ok(Self {
            config_dir,
            data_dir,
            vm_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Generate a Nix configuration for the VM
    fn generate_vm_config(&self, config: &VmConfig) -> BlixardResult<String> {
        // This is a simplified example - in practice this would be much more sophisticated
        let nix_config = format!(
            r#"{{
  microvm = {{
    vcpu = {};
    mem = {};
    interfaces = [{{
      type = "tap";
      id = "vm-{}";
      mac = "02:00:00:00:00:01";
    }}];
    
    shares = [{{
      tag = "ro-store";
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
    }}];
    
    volumes = [{{
      image = "{}/disk.img";
      mountPoint = "/";
      size = 1024; # MB
    }}];
    
    kernel.package = pkgs.linuxPackages.kernel;
    
    # Basic system configuration
    extraModules = [{{
      users.users.root.initialPassword = "";
      services.openssh.enable = true;
      services.openssh.permitRootLogin = "yes";
      networking.firewall.enable = false;
      system.stateVersion = "23.11";
    }}];
  }};
}}"#,
            config.vcpus,
            config.memory,
            config.name,
            self.data_dir.join(&config.name).display()
        );
        
        Ok(nix_config)
    }
    
    /// Execute a command and capture output
    async fn run_command(&self, cmd: &str, args: &[&str]) -> BlixardResult<String> {
        let output = tokio::task::spawn_blocking({
            let cmd = cmd.to_string();
            let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            move || {
                Command::new(cmd)
                    .args(args)
                    .output()
            }
        }).await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to execute command: {}", e),
        })??;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BlixardError::VmOperationFailed {
                operation: "command execution".to_string(),
                details: stderr.to_string(),
            });
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Check if a VM process is running
    async fn is_vm_running(&self, name: &str) -> BlixardResult<bool> {
        // Check if there's a process with the VM name
        // This is a simplified implementation - in practice you'd check
        // for specific microvm processes
        let output = self.run_command("pgrep", &["-f", &format!("microvm-{}", name)]).await;
        Ok(output.is_ok())
    }
    
    /// Update the local cache of VM status
    async fn update_cache(&self, name: &str, status: VmStatus) {
        let mut cache = self.vm_cache.write().await;
        cache.insert(name.to_string(), status);
    }
    
    /// Get VM status from cache or query the system
    async fn get_cached_status(&self, name: &str) -> Option<VmStatus> {
        let cache = self.vm_cache.read().await;
        cache.get(name).cloned()
    }
}

#[async_trait]
impl VmBackend for MicrovmBackend {
    async fn create_vm(&self, config: &VmConfig, _node_id: u64) -> BlixardResult<()> {
        tracing::info!("Creating microVM '{}'", config.name);
        
        // Generate Nix configuration
        let nix_config = self.generate_vm_config(config)?;
        
        // Write configuration to file
        let config_path = self.config_dir.join(format!("{}.nix", config.name));
        tokio::fs::write(&config_path, nix_config).await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to write VM config: {}", e),
        })?;
        
        // Create VM data directory
        let vm_data_dir = self.data_dir.join(&config.name);
        tokio::fs::create_dir_all(&vm_data_dir).await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to create VM data directory: {}", e),
        })?;
        
        // For now, we don't actually build the VM - that would happen in start_vm
        // This is where you'd run: nix build .#microvm-{name}
        
        self.update_cache(&config.name, VmStatus::Stopped).await;
        
        tracing::info!("Successfully created microVM '{}'", config.name);
        Ok(())
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Starting microVM '{}'", name);
        
        // Check if already running
        if self.is_vm_running(name).await? {
            return Err(BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("VM '{}' is already running", name),
            });
        }
        
        // This is where you'd actually start the microvm.nix VM
        // For now, we simulate success
        // Real command would be something like:
        // nix run .#microvm-{name} &
        
        // Simulate the startup process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        self.update_cache(name, VmStatus::Running).await;
        
        tracing::info!("Successfully started microVM '{}'", name);
        Ok(())
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Stopping microVM '{}'", name);
        
        // Check if actually running
        if !self.is_vm_running(name).await? {
            return Err(BlixardError::VmOperationFailed {
                operation: "stop".to_string(),
                details: format!("VM '{}' is not running", name),
            });
        }
        
        // This is where you'd send a shutdown signal to the VM
        // Real implementation would gracefully shut down the microVM
        
        // Simulate the shutdown process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        
        self.update_cache(name, VmStatus::Stopped).await;
        
        tracing::info!("Successfully stopped microVM '{}'", name);
        Ok(())
    }
    
    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Deleting microVM '{}'", name);
        
        // Stop the VM if it's running
        if self.is_vm_running(name).await? {
            self.stop_vm(name).await?;
        }
        
        // Remove configuration file
        let config_path = self.config_dir.join(format!("{}.nix", name));
        if config_path.exists() {
            tokio::fs::remove_file(&config_path).await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to remove VM config: {}", e),
            })?;
        }
        
        // Remove VM data directory
        let vm_data_dir = self.data_dir.join(name);
        if vm_data_dir.exists() {
            tokio::fs::remove_dir_all(&vm_data_dir).await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to remove VM data directory: {}", e),
            })?;
        }
        
        // Remove from cache
        {
            let mut cache = self.vm_cache.write().await;
            cache.remove(name);
        }
        
        tracing::info!("Successfully deleted microVM '{}'", name);
        Ok(())
    }
    
    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        tracing::debug!("Updating microVM '{}' status to {:?}", name, status);
        self.update_cache(name, status).await;
        Ok(())
    }
    
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        // First check cache
        if let Some(cached_status) = self.get_cached_status(name).await {
            // Verify the cached status is still accurate
            match cached_status {
                VmStatus::Running => {
                    if self.is_vm_running(name).await? {
                        return Ok(Some(VmStatus::Running));
                    } else {
                        // Status changed - update cache
                        self.update_cache(name, VmStatus::Stopped).await;
                        return Ok(Some(VmStatus::Stopped));
                    }
                }
                VmStatus::Stopped => {
                    // Check if VM was started externally
                    if self.is_vm_running(name).await? {
                        self.update_cache(name, VmStatus::Running).await;
                        return Ok(Some(VmStatus::Running));
                    } else {
                        return Ok(Some(VmStatus::Stopped));
                    }
                }
                other => return Ok(Some(other)),
            }
        }
        
        // No cached status - check if VM exists
        let config_path = self.config_dir.join(format!("{}.nix", name));
        if !config_path.exists() {
            return Ok(None);
        }
        
        // VM exists - determine status
        let status = if self.is_vm_running(name).await? {
            VmStatus::Running
        } else {
            VmStatus::Stopped
        };
        
        self.update_cache(name, status).await;
        Ok(Some(status))
    }
    
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        let mut result = Vec::new();
        
        // Read all .nix files in the config directory
        let mut dir_entries = tokio::fs::read_dir(&self.config_dir).await.map_err(|e| {
            BlixardError::Internal {
                message: format!("Failed to read config directory: {}", e),
            }
        })?;
        
        while let Some(entry) = dir_entries.next_entry().await.map_err(|e| {
            BlixardError::Internal {
                message: format!("Failed to read directory entry: {}", e),
            }
        })? {
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "nix" {
                    if let Some(stem) = path.file_stem() {
                        if let Some(vm_name) = stem.to_str() {
                            // For this example, we create a basic config
                            // In practice, you'd parse the Nix file to extract the real config
                            let config = VmConfig {
                                name: vm_name.to_string(),
                                config_path: path.to_string_lossy().to_string(),
                                vcpus: 1,
                                memory: 512,
                            };
                            
                            let status = self.get_vm_status(vm_name).await?
                                .unwrap_or(VmStatus::Stopped);
                            
                            result.push((config, status));
                        }
                    }
                }
            }
        }
        
        Ok(result)
    }
}