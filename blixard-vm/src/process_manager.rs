use crate::types::*;
use blixard_core::error::{BlixardError, BlixardResult};
use blixard_core::types::VmStatus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Manages VM processes and their lifecycle
pub struct VmProcessManager {
    /// Currently running VM processes
    processes: Arc<RwLock<HashMap<String, VmProcess>>>,
    /// Runtime directory for VM artifacts
    runtime_dir: PathBuf,
    /// Command executor for testing
    command_executor: Box<dyn CommandExecutor>,
}

/// Information about a running VM process
pub struct VmProcess {
    pub name: String,
    pub child: Option<Child>,
    pub pid: u32,
    pub started_at: SystemTime,
    pub hypervisor: Hypervisor,
    pub runner_path: PathBuf,
    pub _temp_dir: Option<tempfile::TempDir>, // Keep temp directory alive
}

/// Trait for executing commands - allows mocking in tests
#[async_trait::async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn execute(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, std::io::Error>;

    async fn spawn(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<(Child, u32), std::io::Error>;

    async fn kill(&self, pid: u32) -> Result<(), std::io::Error>;
}

pub struct CommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Default command executor that runs real commands
pub struct SystemCommandExecutor;

#[async_trait::async_trait]
impl CommandExecutor for SystemCommandExecutor {
    async fn execute(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, std::io::Error> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let output = cmd.output().await?;

        Ok(CommandOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    async fn spawn(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<(Child, u32), std::io::Error> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()?;
        let pid = child.id().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to get process ID")
        })?;

        Ok((child, pid))
    }

    async fn kill(&self, pid: u32) -> Result<(), std::io::Error> {
        // Use tokio's process kill functionality instead of nix
        use std::process::Command as StdCommand;

        StdCommand::new("kill")
            .args(&["-TERM", &pid.to_string()])
            .status()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(())
    }
}

impl VmProcessManager {
    /// Create a new VM process manager
    pub fn new(runtime_dir: PathBuf) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            runtime_dir,
            command_executor: Box::new(SystemCommandExecutor),
        }
    }

    /// Create a new VM process manager with a custom command executor (for testing)
    pub fn with_executor(runtime_dir: PathBuf, executor: Box<dyn CommandExecutor>) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            runtime_dir,
            command_executor: executor,
        }
    }

    /// Start a VM from a flake path using microvm command
    pub async fn start_vm(&self, name: &str, flake_path: &Path) -> BlixardResult<()> {
        info!(
            "Starting VM '{}' from flake at {} using nix run command",
            name,
            flake_path.display()
        );

        // Check if VM is already running
        {
            let processes = self.processes.read().await;
            if processes.contains_key(name) {
                return Err(BlixardError::VmOperationFailed {
                    operation: "start".to_string(),
                    details: format!("VM '{}' is already running", name),
                });
            }
        }

        // Create a temporary flake copy outside git repository to avoid git tracking issues
        let temp_dir = tempfile::TempDir::new().map_err(|e| BlixardError::VmOperationFailed {
            operation: "start".to_string(),
            details: format!("Failed to create temp directory: {}", e),
        })?;

        let temp_flake_path = temp_dir.path().join("flake.nix");
        std::fs::copy(flake_path.join("flake.nix"), &temp_flake_path).map_err(|e| {
            BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("Failed to copy flake: {}", e),
            }
        })?;

        // Use nix run to start the VM directly (microvm command requires /var/lib/microvms)
        debug!("Starting VM '{}' with nix run command", name);
        let nix_path = format!(
            "path:{}#nixosConfigurations.{}.config.microvm.declaredRunner",
            temp_dir.path().display(),
            name
        );
        let (child, pid) = self
            .command_executor
            .spawn(
                "nix",
                &[
                    "run",
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "--impure",
                    "--accept-flake-config",
                    &nix_path,
                ],
                Some(temp_dir.path()),
            )
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("Failed to start VM with nix run command: {}", e),
            })?;

        info!(
            "Started VM '{}' with PID {} using nix run command",
            name, pid
        );

        // Track the process
        let vm_process = VmProcess {
            name: name.to_string(),
            child: Some(child),
            pid,
            started_at: SystemTime::now(),
            hypervisor: Hypervisor::Qemu, // Default, could be detected from config
            runner_path: PathBuf::new(),
            _temp_dir: Some(temp_dir), // Keep temp directory alive for the VM
        };

        self.processes
            .write()
            .await
            .insert(name.to_string(), vm_process);

        Ok(())
    }

    /// Stop a running VM
    pub async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping VM '{}'", name);

        let mut processes = self.processes.write().await;
        let vm_process = processes
            .remove(name)
            .ok_or_else(|| BlixardError::VmOperationFailed {
                operation: "stop".to_string(),
                details: format!("VM '{}' is not running", name),
            })?;

        // Kill the process
        if let Some(mut child) = vm_process.child {
            match child.kill().await {
                Ok(_) => info!("Successfully killed VM '{}' process", name),
                Err(e) => warn!("Error killing VM '{}': {}", name, e),
            }
        } else {
            // Try to kill by PID
            if let Err(e) = self.command_executor.kill(vm_process.pid).await {
                warn!("Failed to kill VM '{}' by PID: {}", name, e);
            }
        }

        info!("Successfully stopped VM '{}'", name);
        Ok(())
    }

    /// Get the status of a VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        let processes = self.processes.read().await;

        if let Some(process) = processes.get(name) {
            // Check if process is still running
            if let Ok(exists) = self.process_exists(process.pid).await {
                if exists {
                    return Ok(Some(VmStatus::Running));
                }
            }

            // Process no longer exists, clean up
            drop(processes);
            self.processes.write().await.remove(name);
            Ok(Some(VmStatus::Stopped))
        } else {
            Ok(None)
        }
    }

    /// List all running VMs
    pub async fn list_vms(&self) -> BlixardResult<Vec<String>> {
        let processes = self.processes.read().await;
        Ok(processes.keys().cloned().collect())
    }

    /// Connect to VM console socket for interactive access
    pub async fn connect_to_console(&self, name: &str) -> BlixardResult<()> {
        info!("Connecting to console for VM '{}'", name);

        // Check if VM is running
        let processes = self.processes.read().await;
        if !processes.contains_key(name) {
            return Err(BlixardError::VmOperationFailed {
                operation: "console".to_string(),
                details: format!("VM '{}' is not running", name),
            });
        }

        let socket_path = format!("/tmp/{}-console.sock", name);

        info!(
            "Console socket for VM '{}' should be available at: {}",
            name, socket_path
        );
        info!(
            "To connect manually, use: socat - UNIX-CONNECT:{}",
            socket_path
        );

        // For now, just return info about how to connect
        // In a full implementation, we could spawn socat or provide a direct connection
        Ok(())
    }

    /// Build the VM runner using nix
    async fn build_vm_runner(&self, name: &str, flake_path: &Path) -> BlixardResult<PathBuf> {
        let out_link = self.runtime_dir.join(format!("{}-runner", name));

        debug!("Building VM runner for '{}' with nix build", name);

        let output = self
            .command_executor
            .execute(
                "nix",
                &[
                    "build",
                    &format!(".#nixosConfigurations.{}.config.microvm.runner.qemu", name),
                    "--out-link",
                    &out_link.to_string_lossy(),
                    "--impure", // Allow building in dirty git tree
                ],
                Some(flake_path),
            )
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "build".to_string(),
                details: format!("Failed to execute nix build: {}", e),
            })?;

        if !output.status.success() {
            return Err(BlixardError::VmOperationFailed {
                operation: "build".to_string(),
                details: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(out_link)
    }

    /// Detect the hypervisor type from the runner script
    async fn detect_hypervisor(&self, _runner_path: &Path) -> BlixardResult<Hypervisor> {
        // For now, default to cloud-hypervisor
        // In a real implementation, we could inspect the runner script
        Ok(Hypervisor::CloudHypervisor)
    }

    /// Check if a process with given PID exists
    async fn process_exists(&self, pid: u32) -> BlixardResult<bool> {
        // Use kill -0 to check if process exists
        use std::process::Command as StdCommand;

        match StdCommand::new("kill")
            .args(&["-0", &pid.to_string()])
            .status()
        {
            Ok(status) => Ok(status.success()),
            Err(e) => Err(BlixardError::SystemError(format!(
                "Failed to check process: {}",
                e
            ))),
        }
    }

    /// Start a VM using systemd service management
    pub async fn start_vm_systemd(
        &self,
        name: &str,
        flake_path: &Path,
        config: &VmConfig,
    ) -> BlixardResult<()> {
        info!("Starting VM '{}' using systemd", name);

        // Create systemd service file
        self.create_systemd_service(name, flake_path, config)
            .await?;

        // Start the user service
        let output = self
            .command_executor
            .execute(
                "systemctl",
                &["--user", "start", &format!("blixard-vm-{}", name)],
                None,
            )
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "systemd_start".to_string(),
                details: format!("Failed to start systemd service: {}", e),
            })?;

        if !output.status.success() {
            return Err(BlixardError::VmOperationFailed {
                operation: "systemd_start".to_string(),
                details: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        info!("Successfully started VM '{}' as systemd service", name);
        Ok(())
    }

    /// Stop a VM using systemd service management
    pub async fn stop_vm_systemd(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping VM '{}' using systemd", name);

        let service_name = format!("blixard-vm-{}", name);

        // Stop the user service
        let output = self
            .command_executor
            .execute("systemctl", &["--user", "stop", &service_name], None)
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "systemd_stop".to_string(),
                details: format!("Failed to stop systemd service: {}", e),
            })?;

        if !output.status.success() {
            warn!(
                "Failed to stop systemd service: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Remove the service file
        self.remove_systemd_service(name).await?;

        info!("Successfully stopped VM '{}' systemd service", name);
        Ok(())
    }

    /// Get VM status from systemd
    pub async fn get_vm_status_systemd(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        let service_name = format!("blixard-vm-{}", name);

        let output = self
            .command_executor
            .execute("systemctl", &["--user", "is-active", &service_name], None)
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "systemd_status".to_string(),
                details: format!("Failed to check systemd service status: {}", e),
            })?;

        let status_str = String::from_utf8_lossy(&output.stdout);
        let status_str = status_str.trim();
        let status = match status_str {
            "active" => VmStatus::Running,
            "activating" => VmStatus::Starting,
            "deactivating" => VmStatus::Stopping,
            "failed" => VmStatus::Failed,
            "inactive" => VmStatus::Stopped,
            _ => VmStatus::Failed, // Unknown status, treat as failed
        };

        Ok(Some(status))
    }

    /// Create systemd service file for a VM (user service)
    async fn create_systemd_service(
        &self,
        name: &str,
        flake_path: &Path,
        config: &VmConfig,
    ) -> BlixardResult<()> {
        let service_name = format!("blixard-vm-{}.service", name);
        let user_name = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        let service_path = format!("/home/{}/.config/systemd/user/{}", user_name, service_name);
        let user_home = std::env::var("HOME").map_err(|e| {
            BlixardError::ConfigError(format!("Failed to get HOME directory: {}", e))
        })?;

        // Read template
        let template_path = PathBuf::from("blixard-vm/templates/vm.service.template");
        let template = fs::read_to_string(&template_path).await.map_err(|e| {
            BlixardError::ConfigError(format!("Failed to read service template: {}", e))
        })?;

        // Replace template variables
        let cpu_quota = config.vcpus * 100; // 100% per vCPU
        let _service_content = template
            .replace("{{ vm_name }}", name)
            .replace("{{ flake_path }}", &flake_path.display().to_string())
            .replace("{{ memory_mb }}", &config.memory.to_string())
            .replace("{{ cpu_quota }}", &cpu_quota.to_string())
            .replace("{{ working_dir }}", &flake_path.display().to_string())
            .replace(
                "{{ user }}",
                &std::env::var("USER").unwrap_or_else(|_| "user".to_string()),
            )
            .replace("{{ home }}", &user_home);

        // Ensure user systemd directory exists
        std::fs::create_dir_all(&format!("/home/{}/.config/systemd/user", user_name)).map_err(
            |e| {
                BlixardError::ConfigError(format!("Failed to create user systemd directory: {}", e))
            },
        )?;

        // Write service file to temporary location first
        let temp_path = format!("/tmp/blixard-vm-{}.service", name);
        fs::write(&temp_path, _service_content).await.map_err(|e| {
            BlixardError::VmOperationFailed {
                operation: "create_service".to_string(),
                details: format!("Failed to write temporary service file: {}", e),
            }
        })?;

        // Copy to user systemd location
        let copy_output = self
            .command_executor
            .execute("cp", &[&temp_path, &service_path], None)
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "create_service".to_string(),
                details: format!("Failed to copy service file with sudo: {}", e),
            })?;

        if !copy_output.status.success() {
            return Err(BlixardError::VmOperationFailed {
                operation: "create_service".to_string(),
                details: format!(
                    "Failed to copy service file: {}",
                    String::from_utf8_lossy(&copy_output.stderr)
                ),
            });
        }

        // Reload user systemd
        let output = self
            .command_executor
            .execute("systemctl", &["--user", "daemon-reload"], None)
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "daemon_reload".to_string(),
                details: format!("Failed to reload systemd: {}", e),
            })?;

        if !output.status.success() {
            return Err(BlixardError::VmOperationFailed {
                operation: "daemon_reload".to_string(),
                details: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    /// Remove systemd service file for a VM
    async fn remove_systemd_service(&self, name: &str) -> BlixardResult<()> {
        let service_name = format!("blixard-vm-{}.service", name);
        let user_name = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        let service_path = format!("/home/{}/.config/systemd/user/{}", user_name, service_name);

        // Remove service file
        std::fs::remove_file(&service_path).ok(); // Ignore errors if file doesn't exist

        // Reload user systemd
        let output = self
            .command_executor
            .execute("systemctl", &["--user", "daemon-reload"], None)
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "daemon_reload".to_string(),
                details: format!("Failed to reload systemd: {}", e),
            })?;

        if !output.status.success() {
            warn!(
                "Failed to reload systemd: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    /// Mock command executor for testing
    struct MockCommandExecutor {
        next_pid: AtomicU32,
        killed_pids: Arc<Mutex<Vec<u32>>>,
        build_should_fail: bool,
        spawn_should_fail: bool,
    }

    impl MockCommandExecutor {
        fn new() -> Self {
            Self {
                next_pid: AtomicU32::new(1000),
                killed_pids: Arc::new(Mutex::new(Vec::new())),
                build_should_fail: false,
                spawn_should_fail: false,
            }
        }

        fn failing_build() -> Self {
            Self {
                next_pid: AtomicU32::new(1000),
                killed_pids: Arc::new(Mutex::new(Vec::new())),
                build_should_fail: true,
                spawn_should_fail: true, // Make spawn fail for microvm command
            }
        }
    }

    #[async_trait::async_trait]
    impl CommandExecutor for MockCommandExecutor {
        async fn execute(
            &self,
            program: &str,
            args: &[&str],
            _cwd: Option<&Path>,
        ) -> Result<CommandOutput, std::io::Error> {
            if program == "nix" && args.get(0) == Some(&"build") {
                if self.build_should_fail {
                    return Ok(CommandOutput {
                        status: std::process::ExitStatus::from_raw(1),
                        stdout: vec![],
                        stderr: b"Build failed".to_vec(),
                    });
                }

                Ok(CommandOutput {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"Build successful".to_vec(),
                    stderr: vec![],
                })
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Unknown command",
                ))
            }
        }

        async fn spawn(
            &self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&Path>,
        ) -> Result<(Child, u32), std::io::Error> {
            if self.spawn_should_fail {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Spawn failed",
                ));
            }

            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);

            // Return a dummy child process
            // In real tests, we'd need to create a proper mock
            let child = Command::new("true").spawn()?;

            Ok((child, pid))
        }

        async fn kill(&self, pid: u32) -> Result<(), std::io::Error> {
            self.killed_pids
                .lock()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"))?
                .push(pid);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_vm_lifecycle() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::new());
        let manager = VmProcessManager::with_executor(temp_dir.path().to_path_buf(), executor);

        // Start a VM
        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();

        // Create a minimal flake.nix file
        let flake_content = r#"
{
  outputs = { self }: {
    nixosConfigurations.test-vm = {};
  };
}
"#;
        std::fs::write(flake_path.join("flake.nix"), flake_content).unwrap();

        manager.start_vm("test-vm", &flake_path).await.unwrap();

        // Check it's running
        let vms = manager.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert!(vms.contains(&"test-vm".to_string()));

        // Stop the VM
        manager.stop_vm("test-vm").await.unwrap();

        // Check it's stopped
        let vms = manager.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }

    #[tokio::test]
    async fn test_start_already_running() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::new());
        let manager = VmProcessManager::with_executor(temp_dir.path().to_path_buf(), executor);

        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();

        // Create a minimal flake.nix file
        let flake_content = r#"
{
  outputs = { self }: {
    nixosConfigurations.test-vm = {};
  };
}
"#;
        std::fs::write(flake_path.join("flake.nix"), flake_content).unwrap();

        // Start VM
        manager.start_vm("test-vm", &flake_path).await.unwrap();

        // Try to start again
        let result = manager.start_vm("test-vm", &flake_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));
    }

    #[tokio::test]
    async fn test_stop_not_running() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::new());
        let manager = VmProcessManager::with_executor(temp_dir.path().to_path_buf(), executor);

        // Try to stop non-existent VM
        let result = manager.stop_vm("test-vm").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }

    #[tokio::test]
    async fn test_microvm_start_failure() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::failing_build());
        let manager = VmProcessManager::with_executor(temp_dir.path().to_path_buf(), executor);

        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();

        // Create a minimal flake.nix file
        let flake_content = r#"
{
  outputs = { self }: {
    nixosConfigurations.test-vm = {};
  };
}
"#;
        std::fs::write(flake_path.join("flake.nix"), flake_content).unwrap();

        // Start should fail due to spawn failure (simulating nix run command failure)
        let result = manager.start_vm("test-vm", &flake_path).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to start VM with nix run command"));
    }
}
