use anyhow::{Result, Context};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, error};

use crate::types::*;

/// Interface to microvm.nix
pub struct MicroVm;

impl MicroVm {
    /// Generate a microvm.nix configuration file
    pub async fn generate_config(vm_config: &VmConfig) -> Result<String> {
        // For now, use the provided config path
        // Later we can generate Nix expressions programmatically
        Ok(vm_config.config_path.clone())
    }
    
    /// Start a VM using microvm.nix
    pub async fn start(name: &str, config_path: &str) -> Result<()> {
        info!("Starting VM '{}' with config: {}", name, config_path);
        
        let output = Command::new("microvm")
            .args(&["-c", config_path])
            .arg("--name")
            .arg(name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute microvm command")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to start VM: {}", stderr);
            anyhow::bail!("microvm command failed: {}", stderr);
        }
        
        debug!("VM '{}' started successfully", name);
        Ok(())
    }
    
    /// Stop a VM
    pub async fn stop(name: &str) -> Result<()> {
        info!("Stopping VM '{}'", name);
        
        // Stop the systemd service
        let service_name = format!("microvm@{}.service", name);
        let output = Command::new("systemctl")
            .args(&["stop", &service_name])
            .output()
            .await
            .context("Failed to stop systemd service")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to stop VM service: {}", stderr);
            anyhow::bail!("systemctl stop failed: {}", stderr);
        }
        
        Ok(())
    }
    
    /// Get VM status from systemd
    pub async fn get_status(name: &str) -> Result<VmStatus> {
        let service_name = format!("microvm@{}.service", name);
        
        let output = Command::new("systemctl")
            .args(&["is-active", &service_name])
            .output()
            .await
            .context("Failed to check service status")?;
        
        let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        let status = match status_str.as_str() {
            "active" => VmStatus::Running,
            "activating" => VmStatus::Starting,
            "deactivating" => VmStatus::Stopping,
            "inactive" => VmStatus::Stopped,
            "failed" => VmStatus::Failed,
            _ => {
                debug!("Unknown systemd status: {}", status_str);
                VmStatus::Failed
            }
        };
        
        Ok(status)
    }
    
    /// List all microvm services
    pub async fn list_all() -> Result<Vec<String>> {
        let output = Command::new("systemctl")
            .args(&["list-units", "--all", "microvm@*.service", "--no-legend"])
            .output()
            .await
            .context("Failed to list microvm services")?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let vms: Vec<String> = output_str
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(service_name) = parts.get(0) {
                    // Extract VM name from microvm@NAME.service
                    if let Some(name) = service_name.strip_prefix("microvm@") {
                        if let Some(name) = name.strip_suffix(".service") {
                            return Some(name.to_string());
                        }
                    }
                }
                None
            })
            .collect();
        
        Ok(vms)
    }
    
    /// Check if a VM exists
    pub async fn exists(name: &str) -> Result<bool> {
        let vms = Self::list_all().await?;
        Ok(vms.contains(&name.to_string()))
    }
    
    /// Get resource usage for a VM (via systemd cgroups)
    pub async fn get_resource_usage(name: &str) -> Result<ResourceUsage> {
        let service_name = format!("microvm@{}.service", name);
        
        // Get memory usage
        let mem_output = Command::new("systemctl")
            .args(&["show", &service_name, "--property=MemoryCurrent"])
            .output()
            .await?;
        
        let mem_str = String::from_utf8_lossy(&mem_output.stdout);
        let memory_bytes = mem_str
            .trim()
            .strip_prefix("MemoryCurrent=")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        
        // Get CPU usage (this is more complex, simplified for now)
        let cpu_output = Command::new("systemctl")
            .args(&["show", &service_name, "--property=CPUUsageNSec"])
            .output()
            .await?;
        
        let cpu_str = String::from_utf8_lossy(&cpu_output.stdout);
        let cpu_usage_ns = cpu_str
            .trim()
            .strip_prefix("CPUUsageNSec=")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        
        Ok(ResourceUsage {
            memory_bytes,
            cpu_usage_ns,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub cpu_usage_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_vm_status_parsing() {
        // This test doesn't require actual VMs
        let status = MicroVm::get_status("nonexistent-vm").await;
        // Should return Stopped for non-existent VMs
        match status {
            Ok(VmStatus::Stopped) => (),
            _ => panic!("Expected Stopped status for non-existent VM"),
        }
    }
}