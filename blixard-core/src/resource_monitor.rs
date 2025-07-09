//! Real-time resource utilization monitoring
//!
//! This module provides real-time monitoring of actual resource usage vs allocated resources
//! to enable overcommit protection and resource usage optimization.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info};

use crate::{
    error::{BlixardError, BlixardResult},
    metrics_otel,
    node_shared::SharedNodeState,
    types::VmStatus,
    vm_backend::VmManager,
};

/// Real-time resource utilization data for a VM
#[derive(Debug, Clone)]
pub struct VmResourceUsage {
    pub vm_name: String,
    pub allocated_vcpus: u32,
    pub allocated_memory_mb: u64,
    pub allocated_disk_gb: u64,
    pub actual_cpu_percent: f64,
    pub actual_memory_mb: u64,
    pub actual_disk_gb: u64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Node-level resource utilization summary
#[derive(Debug, Clone)]
pub struct NodeResourceUtilization {
    pub node_id: u64,
    pub physical_vcpus: u32,
    pub physical_memory_mb: u64,
    pub physical_disk_gb: u64,
    pub total_allocated_vcpus: u32,
    pub total_allocated_memory_mb: u64,
    pub total_allocated_disk_gb: u64,
    pub actual_cpu_percent: f64,
    pub actual_memory_mb: u64,
    pub actual_disk_gb: u64,
    pub vm_count: usize,
    pub overcommit_ratio_cpu: f64,
    pub overcommit_ratio_memory: f64,
    pub overcommit_ratio_disk: f64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Real-time resource utilization monitor
pub struct ResourceMonitor {
    node_state: Arc<SharedNodeState>,
    vm_manager: Arc<VmManager>,
    monitoring_interval: Duration,
    handle: Option<JoinHandle<()>>,
    /// Per-VM resource usage tracking
    vm_usage: Arc<RwLock<HashMap<String, VmResourceUsage>>>,
    /// Node-level utilization summary
    node_utilization: Arc<RwLock<Option<NodeResourceUtilization>>>,
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        monitoring_interval: Duration,
    ) -> Self {
        Self {
            node_state,
            vm_manager,
            monitoring_interval,
            handle: None,
            vm_usage: Arc::new(RwLock::new(HashMap::new())),
            node_utilization: Arc::new(RwLock::new(None)),
        }
    }

    /// Start resource monitoring
    pub async fn start(&mut self) -> BlixardResult<()> {
        if self.handle.is_some() {
            return Err(BlixardError::Internal {
                message: "Resource monitor is already running".to_string(),
            });
        }

        let node_state = self.node_state.clone();
        let vm_manager = self.vm_manager.clone();
        let vm_usage = self.vm_usage.clone();
        let node_utilization = self.node_utilization.clone();
        let monitoring_interval = self.monitoring_interval;

        let handle = tokio::spawn(async move {
            let mut interval = interval(monitoring_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::collect_resource_metrics(
                    &node_state,
                    &vm_manager,
                    &vm_usage,
                    &node_utilization,
                ).await {
                    error!("Failed to collect resource metrics: {}", e);
                }
            }
        });

        self.handle = Some(handle);
        info!("Resource monitor started with interval: {:?}", monitoring_interval);
        Ok(())
    }

    /// Stop resource monitoring
    pub async fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            info!("Resource monitor stopped");
        }
    }

    /// Get current VM resource usage
    pub async fn get_vm_usage(&self, vm_name: &str) -> Option<VmResourceUsage> {
        self.vm_usage.read().await.get(vm_name).cloned()
    }

    /// Get all VM resource usage
    pub async fn get_all_vm_usage(&self) -> HashMap<String, VmResourceUsage> {
        self.vm_usage.read().await.clone()
    }

    /// Get current node resource utilization
    pub async fn get_node_utilization(&self) -> Option<NodeResourceUtilization> {
        self.node_utilization.read().await.clone()
    }

    /// Get resource efficiency metrics (actual vs allocated)
    pub async fn get_resource_efficiency(&self) -> ResourceEfficiency {
        let vm_usage = self.vm_usage.read().await;
        let mut total_allocated_vcpus = 0u32;
        let mut total_allocated_memory_mb = 0u64;
        let mut total_actual_cpu_percent = 0.0;
        let mut total_actual_memory_mb = 0u64;

        for usage in vm_usage.values() {
            total_allocated_vcpus += usage.allocated_vcpus;
            total_allocated_memory_mb += usage.allocated_memory_mb;
            total_actual_cpu_percent += usage.actual_cpu_percent;
            total_actual_memory_mb += usage.actual_memory_mb;
        }

        ResourceEfficiency {
            cpu_efficiency: if total_allocated_vcpus > 0 {
                total_actual_cpu_percent / (total_allocated_vcpus as f64 * 100.0)
            } else {
                0.0
            },
            memory_efficiency: if total_allocated_memory_mb > 0 {
                total_actual_memory_mb as f64 / total_allocated_memory_mb as f64
            } else {
                0.0
            },
            vm_count: vm_usage.len(),
        }
    }

    /// Check if node is approaching resource limits
    pub async fn check_resource_pressure(&self) -> ResourcePressure {
        let utilization = self.node_utilization.read().await;
        
        if let Some(util) = utilization.as_ref() {
            let cpu_pressure = util.actual_cpu_percent / 100.0;
            let memory_pressure = util.actual_memory_mb as f64 / util.physical_memory_mb as f64;
            let disk_pressure = util.actual_disk_gb as f64 / util.physical_disk_gb as f64;

            ResourcePressure {
                cpu_pressure,
                memory_pressure,
                disk_pressure,
                overcommit_cpu: util.overcommit_ratio_cpu,
                overcommit_memory: util.overcommit_ratio_memory,
                overcommit_disk: util.overcommit_ratio_disk,
                is_high_pressure: cpu_pressure > 0.8 || memory_pressure > 0.8 || disk_pressure > 0.8,
            }
        } else {
            ResourcePressure::default()
        }
    }

    /// Collect resource metrics for all VMs and update tracking
    async fn collect_resource_metrics(
        node_state: &Arc<SharedNodeState>,
        vm_manager: &Arc<VmManager>,
        vm_usage: &Arc<RwLock<HashMap<String, VmResourceUsage>>>,
        node_utilization: &Arc<RwLock<Option<NodeResourceUtilization>>>,
    ) -> BlixardResult<()> {
        let node_id = node_state.get_id();
        
        // Get all VMs on this node
        let vms = vm_manager.list_vms().await?;
        let mut new_vm_usage = HashMap::new();
        let mut total_allocated_vcpus = 0u32;
        let mut total_allocated_memory_mb = 0u64;
        let mut total_allocated_disk_gb = 0u64;
        let mut total_actual_cpu_percent = 0.0;
        let mut total_actual_memory_mb = 0u64;
        let mut total_actual_disk_gb = 0u64;
        let mut vm_count = 0;

        for (vm_config, vm_status) in vms {
            if vm_status == VmStatus::Running {
                // Get actual resource usage (simplified - in production this would 
                // query system metrics, container runtime, or hypervisor APIs)
                let actual_cpu_percent = Self::get_vm_cpu_usage(&vm_config.name).await?;
                let actual_memory_mb = Self::get_vm_memory_usage(&vm_config.name).await?;
                let actual_disk_gb = Self::get_vm_disk_usage(&vm_config.name).await?;

                let usage = VmResourceUsage {
                    vm_name: vm_config.name.clone(),
                    allocated_vcpus: vm_config.vcpus,
                    allocated_memory_mb: vm_config.memory as u64,
                    allocated_disk_gb: 5, // Default disk allocation
                    actual_cpu_percent,
                    actual_memory_mb,
                    actual_disk_gb,
                    last_updated: chrono::Utc::now(),
                };

                total_allocated_vcpus += usage.allocated_vcpus;
                total_allocated_memory_mb += usage.allocated_memory_mb;
                total_allocated_disk_gb += usage.allocated_disk_gb;
                total_actual_cpu_percent += usage.actual_cpu_percent;
                total_actual_memory_mb += usage.actual_memory_mb;
                total_actual_disk_gb += usage.actual_disk_gb;
                vm_count += 1;

                new_vm_usage.insert(vm_config.name, usage);
            }
        }

        // Update VM usage tracking
        *vm_usage.write().await = new_vm_usage;

        // Get node physical capabilities
        let worker_capabilities = node_state.get_worker_capabilities().await?;
        
        // Calculate overcommit ratios
        let overcommit_ratio_cpu = if worker_capabilities.cpu_cores > 0 {
            total_allocated_vcpus as f64 / worker_capabilities.cpu_cores as f64
        } else {
            0.0
        };
        
        let overcommit_ratio_memory = if worker_capabilities.memory_mb > 0 {
            total_allocated_memory_mb as f64 / worker_capabilities.memory_mb as f64
        } else {
            0.0
        };
        
        let overcommit_ratio_disk = if worker_capabilities.disk_gb > 0 {
            total_allocated_disk_gb as f64 / worker_capabilities.disk_gb as f64
        } else {
            0.0
        };

        // Update node utilization summary
        let node_util = NodeResourceUtilization {
            node_id,
            physical_vcpus: worker_capabilities.cpu_cores,
            physical_memory_mb: worker_capabilities.memory_mb,
            physical_disk_gb: worker_capabilities.disk_gb,
            total_allocated_vcpus,
            total_allocated_memory_mb,
            total_allocated_disk_gb,
            actual_cpu_percent: total_actual_cpu_percent,
            actual_memory_mb: total_actual_memory_mb,
            actual_disk_gb: total_actual_disk_gb,
            vm_count,
            overcommit_ratio_cpu,
            overcommit_ratio_memory,
            overcommit_ratio_disk,
            last_updated: chrono::Utc::now(),
        };

        *node_utilization.write().await = Some(node_util.clone());

        // Record metrics for observability
        metrics_otel::record_resource_utilization(
            node_id,
            total_actual_cpu_percent,
            total_actual_memory_mb,
            total_actual_disk_gb,
            overcommit_ratio_cpu,
            overcommit_ratio_memory,
            overcommit_ratio_disk,
        );

        Ok(())
    }

    /// Get CPU usage percentage for a VM (simplified implementation)
    async fn get_vm_cpu_usage(_vm_name: &str) -> BlixardResult<f64> {
        // In production, this would query:
        // - /proc/stat for the VM's processes
        // - Container runtime metrics (Docker, containerd)
        // - Hypervisor APIs (libvirt, QEMU monitor)
        // - System metrics (node_exporter, cadvisor)
        
        // For now, return a realistic simulated value
        Ok(rand::random::<f64>() * 60.0 + 10.0) // 10-70% CPU usage
    }

    /// Get memory usage in MB for a VM (simplified implementation)
    async fn get_vm_memory_usage(_vm_name: &str) -> BlixardResult<u64> {
        // In production, this would query:
        // - /proc/meminfo for the VM's memory usage
        // - Container runtime metrics
        // - Hypervisor memory statistics
        // - System metrics collectors
        
        // For now, return a realistic simulated value
        Ok((rand::random::<u64>() % 1024) + 512) // 512-1536 MB
    }

    /// Get disk usage in GB for a VM (simplified implementation)
    async fn get_vm_disk_usage(_vm_name: &str) -> BlixardResult<u64> {
        // In production, this would query:
        // - df command for VM filesystem usage
        // - Container runtime disk metrics
        // - Hypervisor disk statistics
        // - Storage backend metrics
        
        // For now, return a realistic simulated value
        Ok((rand::random::<u64>() % 3) + 1) // 1-4 GB
    }
}

/// Resource efficiency metrics
#[derive(Debug, Clone)]
pub struct ResourceEfficiency {
    pub cpu_efficiency: f64,    // Actual CPU usage / Allocated CPU (0.0-1.0)
    pub memory_efficiency: f64, // Actual memory usage / Allocated memory (0.0-1.0)
    pub vm_count: usize,
}

/// Resource pressure indicators
#[derive(Debug, Clone)]
pub struct ResourcePressure {
    pub cpu_pressure: f64,      // Actual CPU usage / Physical CPU (0.0-1.0)
    pub memory_pressure: f64,   // Actual memory usage / Physical memory (0.0-1.0)
    pub disk_pressure: f64,     // Actual disk usage / Physical disk (0.0-1.0)
    pub overcommit_cpu: f64,    // Allocated CPU / Physical CPU
    pub overcommit_memory: f64, // Allocated memory / Physical memory
    pub overcommit_disk: f64,   // Allocated disk / Physical disk
    pub is_high_pressure: bool, // True if any resource is >80% utilized
}

impl Default for ResourcePressure {
    fn default() -> Self {
        Self {
            cpu_pressure: 0.0,
            memory_pressure: 0.0,
            disk_pressure: 0.0,
            overcommit_cpu: 0.0,
            overcommit_memory: 0.0,
            overcommit_disk: 0.0,
            is_high_pressure: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_resource_efficiency_calculation() {
        use crate::types::NodeConfig;
        use crate::vm_backend::MockVmBackend;
        use tempfile::TempDir;
        
        let vm_usage = Arc::new(RwLock::new(HashMap::new()));
        
        // Add some test VM usage data
        let mut usage_map = HashMap::new();
        usage_map.insert("vm1".to_string(), VmResourceUsage {
            vm_name: "vm1".to_string(),
            allocated_vcpus: 4,
            allocated_memory_mb: 8192,
            allocated_disk_gb: 20,
            actual_cpu_percent: 50.0,
            actual_memory_mb: 4096,
            actual_disk_gb: 10,
            last_updated: chrono::Utc::now(),
        });
        usage_map.insert("vm2".to_string(), VmResourceUsage {
            vm_name: "vm2".to_string(),
            allocated_vcpus: 2,
            allocated_memory_mb: 4096,
            allocated_disk_gb: 10,
            actual_cpu_percent: 30.0,
            actual_memory_mb: 2048,
            actual_disk_gb: 5,
            last_updated: chrono::Utc::now(),
        });
        
        *vm_usage.write().await = usage_map;

        // Create test database and SharedNodeState
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(db_path).unwrap());
        
        let config = NodeConfig {
            id: 1,
            bind_addr: "127.0.0.1:9001".parse().unwrap(),
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: Default::default(),
        };
        
        let node_state = Arc::new(crate::node_shared::SharedNodeState::new(config));
        
        // Note: get_worker_capabilities() returns hardcoded values:
        // cpu_cores: 8, memory_mb: 16384, disk_gb: 100
        // These match what we need for the test
        
        // Create VM backend and manager
        let vm_backend = Arc::new(MockVmBackend::new(database.clone()));
        let vm_manager = Arc::new(crate::vm_backend::VmManager::new(
            database,
            vm_backend,
            node_state.clone(),
        ));

        // Create resource monitor for testing
        let monitor = ResourceMonitor {
            node_state,
            vm_manager,
            monitoring_interval: Duration::from_secs(60),
            handle: None,
            vm_usage,
            node_utilization: Arc::new(RwLock::new(None)),
        };

        let efficiency = monitor.get_resource_efficiency().await;
        
        // CPU efficiency: (50% + 30%) / (4 + 2) VCPUs = 80% / 6 VCPUs = 13.33%
        assert!((efficiency.cpu_efficiency - 0.1333).abs() < 0.01);
        
        // Memory efficiency: (4096 + 2048) / (8192 + 4096) = 6144 / 12288 = 50%
        assert!((efficiency.memory_efficiency - 0.5).abs() < 0.01);
        
        assert_eq!(efficiency.vm_count, 2);
    }
}