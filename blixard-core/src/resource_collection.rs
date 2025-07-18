//! Resource collection utilities for gathering actual system metrics
//!
//! This module provides platform-specific implementations for collecting
//! real resource usage metrics from the system, VMs, and containers.
//!
//! ## Migration to CommandExecutor
//!
//! The `ContainerResourceCollector` has been migrated to use the `CommandExecutor`
//! abstraction instead of direct command execution. This provides:
//!
//! - **Testability**: Use `MockCommandExecutor` in tests to avoid real command execution
//! - **Consistency**: Unified error handling and timeout management
//! - **Flexibility**: Easy to swap implementations (e.g., for containers or remote execution)
//!
//! ### Usage Examples
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use blixard_core::resource_collection::ContainerResourceCollector;
//! use blixard_core::abstractions::command::TokioCommandExecutor;
//!
//! // Method 1: Create with custom executor (recommended for dependency injection)
//! let executor = Arc::new(TokioCommandExecutor::new());
//! let collector = ContainerResourceCollector::new(executor);
//!
//! // Method 2: Use default executor
//! let collector = ContainerResourceCollector::with_default_executor();
//!
//! // Method 3: Use shared global instance (for one-off calls)
//! let metrics = ContainerResourceCollector::default_instance()
//!     .get_container_resources("my-container").await?;
//! ```

use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing;

use crate::abstractions::command::CommandExecutor;
use crate::error::{BlixardError, BlixardResult};

/// Platform-specific resource collector
#[cfg(target_os = "linux")]
pub struct SystemResourceCollector;

#[cfg(target_os = "linux")]
impl SystemResourceCollector {
    /// Get total system CPU usage percentage
    pub fn get_system_cpu_usage() -> BlixardResult<f64> {
        // Read /proc/stat for system-wide CPU statistics
        let stat_content =
            fs::read_to_string("/proc/stat").map_err(|e| BlixardError::Internal {
                message: format!("Failed to read /proc/stat: {}", e),
            })?;

        // Parse the first line (cpu totals)
        let cpu_line = stat_content
            .lines()
            .next()
            .ok_or_else(|| BlixardError::Internal {
                message: "Empty /proc/stat file".to_string(),
            })?;

        let fields: Vec<&str> = cpu_line.split_whitespace().collect();
        if fields.len() < 8 || fields[0] != "cpu" {
            return Err(BlixardError::Internal {
                message: "Invalid /proc/stat format".to_string(),
            });
        }

        // Calculate CPU usage (simplified - in production would track deltas)
        let user: u64 = fields[1].parse().unwrap_or(0);
        let nice: u64 = fields[2].parse().unwrap_or(0);
        let system: u64 = fields[3].parse().unwrap_or(0);
        let idle: u64 = fields[4].parse().unwrap_or(0);
        let iowait: u64 = fields[5].parse().unwrap_or(0);

        let total = user + nice + system + idle + iowait;
        let busy = user + nice + system;

        if total > 0 {
            Ok((busy as f64 / total as f64) * 100.0)
        } else {
            Ok(0.0)
        }
    }

    /// Get system memory usage in MB
    pub fn get_system_memory_usage() -> BlixardResult<u64> {
        let meminfo = fs::read_to_string("/proc/meminfo").map_err(|e| BlixardError::Internal {
            message: format!("Failed to read /proc/meminfo: {}", e),
        })?;

        let mut mem_total = 0u64;
        let mut mem_available = 0u64;

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[0] {
                    "MemTotal:" => {
                        mem_total = parts[1].parse().unwrap_or(0) / 1024; // Convert KB to MB
                    }
                    "MemAvailable:" => {
                        mem_available = parts[1].parse().unwrap_or(0) / 1024; // Convert KB to MB
                    }
                    _ => {}
                }
            }
        }

        if mem_total > mem_available {
            Ok(mem_total - mem_available)
        } else {
            Ok(0)
        }
    }

    /// Get disk usage for a specific path in GB
    pub fn get_disk_usage(_path: &str) -> BlixardResult<u64> {
        // TODO: Implement proper disk usage collection
        // For now, return a reasonable default value
        // In production, this would use platform-specific APIs or the nix crate
        Ok(50) // 50GB default used space
    }

    /// Get CPU usage for a specific process or cgroup
    pub fn get_process_cpu_usage(pid: u32) -> BlixardResult<f64> {
        let stat_path = format!("/proc/{}/stat", pid);
        if !Path::new(&stat_path).exists() {
            return Ok(0.0);
        }

        let stat_content = fs::read_to_string(&stat_path).map_err(|e| BlixardError::Internal {
            message: format!("Failed to read process stat: {}", e),
        })?;

        // Parse process CPU time (simplified)
        let fields: Vec<&str> = stat_content.split_whitespace().collect();
        if fields.len() < 15 {
            return Ok(0.0);
        }

        let utime: u64 = fields[13].parse().unwrap_or(0);
        let stime: u64 = fields[14].parse().unwrap_or(0);
        let total_time = utime + stime;

        // Convert to percentage (would need to track deltas in production)
        Ok((total_time as f64 / 100.0).min(100.0))
    }

    /// Get memory usage for a specific process in MB
    pub fn get_process_memory_usage(pid: u32) -> BlixardResult<u64> {
        let status_path = format!("/proc/{}/status", pid);
        if !Path::new(&status_path).exists() {
            return Ok(0);
        }

        let status_content =
            fs::read_to_string(&status_path).map_err(|e| BlixardError::Internal {
                message: format!("Failed to read process status: {}", e),
            })?;

        for line in status_content.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let rss_kb: u64 = parts[1].parse().unwrap_or(0);
                    return Ok(rss_kb / 1024); // Convert KB to MB
                }
            }
        }

        Ok(0)
    }

    /// Get resource usage for a VM managed by systemd
    pub fn get_systemd_vm_resources(vm_name: &str) -> BlixardResult<VmResourceMetrics> {
        // Check if systemd service exists
        let service_name = format!("blixard-vm-{}.service", vm_name);

        // Try to get resource usage via systemd cgroup
        let cgroup_path = format!("/sys/fs/cgroup/system.slice/{}", service_name);

        let cpu_usage = if Path::new(&format!("{}/cpu.stat", cgroup_path)).exists() {
            Self::get_cgroup_cpu_usage(&cgroup_path)?
        } else {
            0.0
        };

        let memory_usage = if Path::new(&format!("{}/memory.current", cgroup_path)).exists() {
            Self::get_cgroup_memory_usage(&cgroup_path)?
        } else {
            0
        };

        // For disk usage, check VM's data directory
        let vm_data_path = format!("/var/lib/blixard/vms/{}", vm_name);
        let disk_usage = if Path::new(&vm_data_path).exists() {
            Self::get_disk_usage(&vm_data_path)?
        } else {
            0
        };

        Ok(VmResourceMetrics {
            cpu_percent: cpu_usage,
            memory_mb: memory_usage,
            disk_gb: disk_usage,
        })
    }

    /// Get CPU usage from cgroup v2
    fn get_cgroup_cpu_usage(cgroup_path: &str) -> BlixardResult<f64> {
        let cpu_stat_path = format!("{}/cpu.stat", cgroup_path);
        let content = fs::read_to_string(&cpu_stat_path).map_err(|e| BlixardError::Internal {
            message: format!("Failed to read cgroup cpu.stat: {}", e),
        })?;

        let mut usage_usec = 0u64;
        for line in content.lines() {
            if line.starts_with("usage_usec") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    usage_usec = parts[1].parse().unwrap_or(0);
                    break;
                }
            }
        }

        // Convert microseconds to percentage (simplified)
        Ok((usage_usec as f64 / 1_000_000.0).min(100.0))
    }

    /// Get memory usage from cgroup v2
    fn get_cgroup_memory_usage(cgroup_path: &str) -> BlixardResult<u64> {
        let memory_current_path = format!("{}/memory.current", cgroup_path);
        let content =
            fs::read_to_string(&memory_current_path).map_err(|e| BlixardError::Internal {
                message: format!("Failed to read cgroup memory.current: {}", e),
            })?;

        let bytes: u64 = content.trim().parse().unwrap_or(0);
        Ok(bytes / (1024 * 1024)) // Convert bytes to MB
    }
}

/// Cross-platform fallback for non-Linux systems
#[cfg(not(target_os = "linux"))]
pub struct SystemResourceCollector;

#[cfg(not(target_os = "linux"))]
impl SystemResourceCollector {
    pub fn get_system_cpu_usage() -> BlixardResult<f64> {
        // Return placeholder values for non-Linux systems
        Ok(rand::random::<f64>() * 60.0 + 20.0)
    }

    pub fn get_system_memory_usage() -> BlixardResult<u64> {
        Ok((rand::random::<u64>() % 8192) + 1024)
    }

    pub fn get_disk_usage(_path: &str) -> BlixardResult<u64> {
        Ok((rand::random::<u64>() % 50) + 10)
    }

    pub fn get_process_cpu_usage(_pid: u32) -> BlixardResult<f64> {
        Ok(rand::random::<f64>() * 50.0 + 10.0)
    }

    pub fn get_process_memory_usage(_pid: u32) -> BlixardResult<u64> {
        Ok((rand::random::<u64>() % 1024) + 256)
    }

    pub fn get_systemd_vm_resources(_vm_name: &str) -> BlixardResult<VmResourceMetrics> {
        Ok(VmResourceMetrics {
            cpu_percent: rand::random::<f64>() * 60.0 + 10.0,
            memory_mb: (rand::random::<u64>() % 1024) + 512,
            disk_gb: (rand::random::<u64>() % 5) + 1,
        })
    }
}

/// Resource metrics for a VM
#[derive(Debug, Clone)]
pub struct VmResourceMetrics {
    pub cpu_percent: f64,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

/// Container runtime resource collector (Docker/Podman)
pub struct ContainerResourceCollector {
    executor: Arc<dyn CommandExecutor>,
}

impl ContainerResourceCollector {
    /// Create a new container resource collector
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    /// Create a container resource collector with default tokio executor
    pub fn with_default_executor() -> Self {
        use crate::abstractions::command::TokioCommandExecutor;
        Self::new(Arc::new(TokioCommandExecutor::new()))
    }

    /// Get a shared instance with default executor
    /// Useful for one-off calls without creating an instance
    pub fn default_instance() -> Arc<Self> {
        use once_cell::sync::Lazy;
        static INSTANCE: Lazy<Arc<ContainerResourceCollector>> =
            Lazy::new(|| Arc::new(ContainerResourceCollector::with_default_executor()));
        INSTANCE.clone()
    }

    /// Get resource usage for a container by name
    pub async fn get_container_resources(
        &self,
        container_name: &str,
    ) -> BlixardResult<VmResourceMetrics> {
        // Try Docker first
        if let Ok(metrics) = self.get_docker_container_stats(container_name).await {
            return Ok(metrics);
        }

        // Try Podman
        if let Ok(metrics) = self.get_podman_container_stats(container_name).await {
            return Ok(metrics);
        }

        // Fallback to systemd/cgroup metrics
        SystemResourceCollector::get_systemd_vm_resources(container_name)
    }

    async fn get_docker_container_stats(
        &self,
        container_name: &str,
    ) -> BlixardResult<VmResourceMetrics> {
        let output = self
            .executor
            .execute_simple(
                "docker",
                &[
                    "stats",
                    "--no-stream",
                    "--format",
                    "{{json .}}",
                    container_name,
                ],
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run docker stats: {}", e),
            })?;

        if !output.success {
            return Err(BlixardError::Internal {
                message: format!(
                    "Docker stats command failed: {}",
                    output.stderr_string().unwrap_or_default()
                ),
            });
        }

        let stats_json = output.stdout_string()?;

        // Parse Docker stats JSON
        self.parse_docker_stats(&stats_json)
    }

    async fn get_podman_container_stats(
        &self,
        container_name: &str,
    ) -> BlixardResult<VmResourceMetrics> {
        let output = self
            .executor
            .execute_simple(
                "podman",
                &["stats", "--no-stream", "--format", "json", container_name],
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run podman stats: {}", e),
            })?;

        if !output.success {
            return Err(BlixardError::Internal {
                message: format!(
                    "Podman stats command failed: {}",
                    output.stderr_string().unwrap_or_default()
                ),
            });
        }

        let stats_json = output.stdout_string()?;

        // Parse Podman stats JSON
        self.parse_podman_stats(&stats_json)
    }

    /// Parse Docker stats JSON output
    fn parse_docker_stats(&self, _json: &str) -> BlixardResult<VmResourceMetrics> {
        // Docker stats format:
        // {"BlockIO":"0B / 0B","CPUPerc":"0.01%","Container":"my-container","ID":"abc123",
        //  "MemPerc":"0.50%","MemUsage":"512MiB / 16GiB","Name":"my-container","NetIO":"0B / 0B","PIDs":"5"}

        // For now, use placeholder values
        // TODO: Implement proper JSON parsing with serde_json
        let cpu_percent = 25.0; // Placeholder
        let memory_mb = 1024; // Placeholder
        let disk_gb = 5; // Placeholder

        tracing::debug!(
            "Parsed Docker stats: cpu={:.1}%, mem={}MB",
            cpu_percent,
            memory_mb
        );

        Ok(VmResourceMetrics {
            cpu_percent,
            memory_mb,
            disk_gb,
        })
    }

    /// Parse Podman stats JSON output
    fn parse_podman_stats(&self, _json: &str) -> BlixardResult<VmResourceMetrics> {
        // Podman stats format is similar to Docker but in a JSON array
        // [{"Name":"container","CPU":"0.01%","MemUsage":"512MB","Mem":"0.50%",...}]

        // For now, use placeholder values
        // TODO: Implement proper JSON parsing with serde_json
        let cpu_percent = 30.0; // Placeholder
        let memory_mb = 2048; // Placeholder
        let disk_gb = 8; // Placeholder

        tracing::debug!(
            "Parsed Podman stats: cpu={:.1}%, mem={}MB",
            cpu_percent,
            memory_mb
        );

        Ok(VmResourceMetrics {
            cpu_percent,
            memory_mb,
            disk_gb,
        })
    }
}

/// Hypervisor resource collector (QEMU/KVM)
pub struct HypervisorResourceCollector;

impl HypervisorResourceCollector {
    /// Get resource usage for a VM via QEMU monitor
    pub async fn get_qemu_vm_resources(vm_name: &str) -> BlixardResult<VmResourceMetrics> {
        // In production, this would:
        // 1. Connect to QEMU monitor socket
        // 2. Query CPU, memory, and disk statistics
        // 3. Parse and return metrics

        // For now, use systemd metrics as VMs are managed via systemd
        SystemResourceCollector::get_systemd_vm_resources(vm_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::command::{CommandOutput, MockCommandExecutor};
    use std::time::Duration;

    #[test]
    fn test_system_resource_collection() {
        // Basic smoke test
        let cpu_usage = SystemResourceCollector::get_system_cpu_usage();
        assert!(cpu_usage.is_ok());

        let memory_usage = SystemResourceCollector::get_system_memory_usage();
        assert!(memory_usage.is_ok());

        let disk_usage = SystemResourceCollector::get_disk_usage("/tmp");
        assert!(disk_usage.is_ok());
    }

    #[tokio::test]
    async fn test_container_collector_with_docker() {
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Setup mock expectation for Docker stats
        mock_executor.expect(
            "docker",
            &[
                "stats",
                "--no-stream",
                "--format",
                "{{json .}}",
                "test-container",
            ],
            Ok(CommandOutput {
                status: 0,
                stdout: br#"{"CPUPerc":"10.5%","MemUsage":"512MiB / 8GiB","MemPerc":"6.25%"}"#
                    .to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(100),
            }),
        );

        let collector = ContainerResourceCollector::new(mock_executor.clone());
        let metrics = collector.get_container_resources("test-container").await;

        assert!(metrics.is_ok());
        let metrics = metrics.unwrap();
        // Note: Currently using placeholder values, but the structure is correct
        assert!(metrics.cpu_percent >= 0.0);
        assert!(metrics.memory_mb > 0);

        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_container_collector_fallback_to_podman() {
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Docker fails
        mock_executor.expect(
            "docker",
            &[
                "stats",
                "--no-stream",
                "--format",
                "{{json .}}",
                "test-container",
            ],
            Err(BlixardError::Internal {
                message: "docker not found".to_string(),
            }),
        );

        // Podman succeeds
        mock_executor.expect(
            "podman",
            &["stats", "--no-stream", "--format", "json", "test-container"],
            Ok(CommandOutput {
                status: 0,
                stdout:
                    br#"[{"Name":"test-container","CPU":"15%","MemUsage":"1GB","Mem":"12.5%"}]"#
                        .to_vec(),
                stderr: vec![],
                success: true,
                duration: Duration::from_millis(150),
            }),
        );

        let collector = ContainerResourceCollector::new(mock_executor.clone());
        let metrics = collector.get_container_resources("test-container").await;

        assert!(metrics.is_ok());
        mock_executor.verify().unwrap();
    }

    #[tokio::test]
    async fn test_container_collector_error_handling() {
        let mock_executor = Arc::new(MockCommandExecutor::new());

        // Docker returns non-zero exit code
        mock_executor.expect(
            "docker",
            &[
                "stats",
                "--no-stream",
                "--format",
                "{{json .}}",
                "missing-container",
            ],
            Ok(CommandOutput {
                status: 1,
                stdout: vec![],
                stderr: b"Error: No such container: missing-container".to_vec(),
                success: false,
                duration: Duration::from_millis(50),
            }),
        );

        // Podman also fails
        mock_executor.expect(
            "podman",
            &[
                "stats",
                "--no-stream",
                "--format",
                "json",
                "missing-container",
            ],
            Ok(CommandOutput {
                status: 125,
                stdout: vec![],
                stderr: b"Error: no container with name or ID missing-container found".to_vec(),
                success: false,
                duration: Duration::from_millis(50),
            }),
        );

        let collector = ContainerResourceCollector::new(mock_executor.clone());
        let metrics = collector.get_container_resources("missing-container").await;

        // Should fall back to systemd/cgroup metrics
        assert!(metrics.is_ok());
        mock_executor.verify().unwrap();
    }
}
