use crate::types::*;
use blixard_core::error::{BlixardResult, BlixardError};
use blixard_core::types::VmStatus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::process::{Command, Child};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};
use std::process::Stdio;

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
}

/// Trait for executing commands - allows mocking in tests
#[async_trait::async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn execute(&self, program: &str, args: &[&str], cwd: Option<&Path>) 
        -> Result<CommandOutput, std::io::Error>;
    
    async fn spawn(&self, program: &str, args: &[&str], cwd: Option<&Path>)
        -> Result<(Child, u32), std::io::Error>;
    
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
    async fn execute(&self, program: &str, args: &[&str], cwd: Option<&Path>) 
        -> Result<CommandOutput, std::io::Error> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
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
    
    async fn spawn(&self, program: &str, args: &[&str], cwd: Option<&Path>)
        -> Result<(Child, u32), std::io::Error> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
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
    
    /// Start a VM from a flake path
    pub async fn start_vm(&self, name: &str, flake_path: &Path) -> BlixardResult<()> {
        info!("Starting VM '{}' from flake at {}", name, flake_path.display());
        
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
        
        // Use nix run to start the VM directly
        debug!("Starting VM '{}' with nix run", name);
        let (child, pid) = self.command_executor
            .spawn(
                "nix",
                &[
                    "--extra-experimental-features", "nix-command flakes",
                    "run",
                    &format!("path:{}#nixosConfigurations.{}.config.microvm.runner.qemu", flake_path.display(), name),
                    "--impure",
                    "--",
                    "--no-reboot",
                ],
                None
            )
            .await
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("Failed to start VM: {}", e),
            })?;
        
        info!("Started VM '{}' with PID {}", name, pid);
        
        // Track the process
        let vm_process = VmProcess {
            name: name.to_string(),
            child: Some(child),
            pid,
            started_at: SystemTime::now(),
            hypervisor: Hypervisor::Qemu,
            runner_path: PathBuf::new(),
        };
        
        self.processes.write().await.insert(name.to_string(), vm_process);
        
        Ok(())
    }
    
    /// Stop a running VM
    pub async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        info!("Stopping VM '{}'", name);
        
        let mut processes = self.processes.write().await;
        let vm_process = processes.remove(name)
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
    
    /// Build the VM runner using nix
    async fn build_vm_runner(&self, name: &str, flake_path: &Path) -> BlixardResult<PathBuf> {
        let out_link = self.runtime_dir.join(format!("{}-runner", name));
        
        debug!("Building VM runner for '{}' with nix build", name);
        
        let output = self.command_executor
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
            .status() {
            Ok(status) => Ok(status.success()),
            Err(e) => Err(BlixardError::SystemError(format!("Failed to check process: {}", e))),
        }
    }
    
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::os::unix::process::ExitStatusExt;
    
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
                spawn_should_fail: false,
            }
        }
    }
    
    #[async_trait::async_trait]
    impl CommandExecutor for MockCommandExecutor {
        async fn execute(&self, program: &str, args: &[&str], _cwd: Option<&Path>) 
            -> Result<CommandOutput, std::io::Error> {
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
                Err(std::io::Error::new(std::io::ErrorKind::Other, "Unknown command"))
            }
        }
        
        async fn spawn(&self, _program: &str, _args: &[&str], _cwd: Option<&Path>)
            -> Result<(Child, u32), std::io::Error> {
            if self.spawn_should_fail {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Spawn failed"));
            }
            
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
            
            // Return a dummy child process
            // In real tests, we'd need to create a proper mock
            let child = Command::new("true").spawn()?;
            
            Ok((child, pid))
        }
        
        async fn kill(&self, pid: u32) -> Result<(), std::io::Error> {
            self.killed_pids.lock().unwrap().push(pid);
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_vm_lifecycle() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::new());
        let manager = VmProcessManager::with_executor(
            temp_dir.path().to_path_buf(),
            executor,
        );
        
        // Start a VM
        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();
        
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
        let manager = VmProcessManager::with_executor(
            temp_dir.path().to_path_buf(),
            executor,
        );
        
        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();
        
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
        let manager = VmProcessManager::with_executor(
            temp_dir.path().to_path_buf(),
            executor,
        );
        
        // Try to stop non-existent VM
        let result = manager.stop_vm("test-vm").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not running"));
    }
    
    #[tokio::test]
    async fn test_build_failure() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let executor = Box::new(MockCommandExecutor::failing_build());
        let manager = VmProcessManager::with_executor(
            temp_dir.path().to_path_buf(),
            executor,
        );
        
        let flake_path = temp_dir.path().join("test-flake");
        std::fs::create_dir_all(&flake_path).unwrap();
        
        // Start should fail due to build failure
        let result = manager.start_vm("test-vm", &flake_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Build failed"));
    }
}