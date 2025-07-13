//! VM configuration and scheduling

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// VM configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VmConfig {
    /// Default VM settings
    pub defaults: VmDefaults,

    /// Scheduler configuration
    pub scheduler: SchedulerConfig,

    /// Resource limits
    pub limits: ResourceLimits,
}

/// Default VM settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmDefaults {
    /// Default vCPUs
    pub vcpus: u32,

    /// Default memory in MB
    pub memory_mb: u32,

    /// Default disk in GB
    pub disk_gb: u32,

    /// Default hypervisor
    pub hypervisor: String,
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    /// Default placement strategy
    pub default_strategy: String,

    /// Enable anti-affinity
    pub enable_anti_affinity: bool,

    /// Resource overcommit ratio
    pub overcommit_ratio: f32,

    /// Rebalance interval
    #[serde(with = "humantime_serde")]
    pub rebalance_interval: Duration,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceLimits {
    /// Maximum VMs per node
    pub max_vms_per_node: usize,

    /// Maximum total vCPUs
    pub max_total_vcpus: u32,

    /// Maximum total memory in MB
    pub max_total_memory_mb: u64,

    /// Maximum total disk in GB
    pub max_total_disk_gb: u64,
}

impl Default for VmDefaults {
    fn default() -> Self {
        Self {
            vcpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            hypervisor: "qemu".to_string(),
        }
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            default_strategy: "most_available".to_string(),
            enable_anti_affinity: true,
            overcommit_ratio: 1.2,
            rebalance_interval: Duration::from_secs(300),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_vms_per_node: 100,
            max_total_vcpus: 512,
            max_total_memory_mb: 131_072, // 128GB
            max_total_disk_gb: 4096,     // 4TB
        }
    }
}