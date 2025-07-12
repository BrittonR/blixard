//! Resource quotas and limits system for preventing resource exhaustion
//!
//! This module provides:
//! - Per-tenant VM and resource limits
//! - API rate limiting capabilities
//! - Quota tracking and enforcement
//! - Resource allocation validation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Default quota configuration constants
mod constants {
    /// Default maximum VMs per tenant
    pub const DEFAULT_MAX_VMS: u32 = 50;

    /// Default maximum VCPUs per tenant
    pub const DEFAULT_MAX_VCPUS: u32 = 200;

    /// Default maximum memory per tenant (200GB in MB)
    pub const DEFAULT_MAX_MEMORY_MB: u64 = 204800;

    /// Default maximum disk space per tenant (2TB in GB)
    pub const DEFAULT_MAX_DISK_GB: u64 = 2048;

    /// Default maximum VMs per node
    pub const DEFAULT_MAX_VMS_PER_NODE: u32 = 25;

    /// Default overcommit ratio
    pub const DEFAULT_OVERCOMMIT_RATIO: f32 = 1.2;

    /// Default priority level
    pub const DEFAULT_PRIORITY: u8 = 100;

    /// Default API requests per second
    pub const DEFAULT_REQUESTS_PER_SECOND: u32 = 100;

    /// Default burst capacity
    pub const DEFAULT_BURST_CAPACITY: u32 = 200;

    /// Default maximum concurrent requests
    pub const DEFAULT_MAX_CONCURRENT_REQUESTS: u32 = 50;

    /// Default VM creation requests per minute
    pub const DEFAULT_VM_CREATE_PER_MINUTE: u32 = 10;

    /// Default VM deletion requests per minute
    pub const DEFAULT_VM_DELETE_PER_MINUTE: u32 = 20;

    /// Default cluster join requests per hour
    pub const DEFAULT_CLUSTER_JOIN_PER_HOUR: u32 = 5;

    /// Default status query requests per second
    pub const DEFAULT_STATUS_QUERY_PER_SECOND: u32 = 50;

    /// Default config change requests per hour
    pub const DEFAULT_CONFIG_CHANGE_PER_HOUR: u32 = 10;

    /// Default maximum storage per tenant (1TB in GB)
    pub const DEFAULT_MAX_STORAGE_GB: u64 = 1024;

    /// Default maximum disk images per tenant
    pub const DEFAULT_MAX_DISK_IMAGES: u32 = 100;

    /// Default maximum image size per tenant (100GB)
    pub const DEFAULT_MAX_IMAGE_SIZE_GB: u64 = 100;

    /// Default maximum backup storage per tenant (512GB)
    pub const DEFAULT_MAX_BACKUP_STORAGE_GB: u64 = 512;

    /// Default maximum IOPS per tenant
    pub const DEFAULT_MAX_IOPS: u32 = 10000;
}

/// Tenant identifier for resource quota management
pub type TenantId = String;

/// Resource quota configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantQuota {
    /// Tenant identifier
    pub tenant_id: TenantId,

    /// VM resource limits
    pub vm_limits: VmResourceLimits,

    /// API rate limits
    pub api_limits: ApiRateLimits,

    /// Storage limits
    pub storage_limits: StorageLimits,

    /// Enable/disable quota enforcement
    pub enabled: bool,

    /// Quota creation timestamp
    pub created_at: SystemTime,

    /// Quota last updated timestamp
    pub updated_at: SystemTime,
}

/// VM resource limits for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmResourceLimits {
    /// Maximum number of VMs
    pub max_vms: u32,

    /// Maximum total vCPUs across all VMs
    pub max_vcpus: u32,

    /// Maximum total memory in MB across all VMs
    pub max_memory_mb: u64,

    /// Maximum total disk space in GB across all VMs
    pub max_disk_gb: u64,

    /// Maximum VMs per individual node
    pub max_vms_per_node: u32,

    /// Resource overcommit ratio (1.0 = no overcommit)
    pub overcommit_ratio: f32,

    /// Priority level for resource allocation (higher = more priority)
    pub priority: u8,
}

/// API rate limits for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiRateLimits {
    /// Requests per second limit
    pub requests_per_second: u32,

    /// Burst capacity (max requests in short bursts)
    pub burst_capacity: u32,

    /// Concurrent request limit
    pub max_concurrent_requests: u32,

    /// Specific operation limits
    pub operation_limits: OperationLimits,
}

/// Per-operation rate limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationLimits {
    /// VM creation requests per minute
    pub vm_create_per_minute: u32,

    /// VM deletion requests per minute
    pub vm_delete_per_minute: u32,

    /// Cluster join requests per hour
    pub cluster_join_per_hour: u32,

    /// Status query requests per second
    pub status_query_per_second: u32,

    /// Configuration change requests per hour
    pub config_change_per_hour: u32,
}

/// Storage limits for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageLimits {
    /// Maximum total storage usage in GB
    pub max_storage_gb: u64,

    /// Maximum number of disk images
    pub max_disk_images: u32,

    /// Maximum size per disk image in GB
    pub max_image_size_gb: u64,

    /// Maximum backup storage in GB
    pub max_backup_storage_gb: u64,

    /// I/O operations per second limit
    pub max_iops: u32,
}

/// Current resource usage for a tenant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantUsage {
    /// Tenant identifier
    pub tenant_id: TenantId,

    /// Current VM resource usage
    pub vm_usage: VmResourceUsage,

    /// Current API usage
    pub api_usage: ApiUsage,

    /// Current storage usage
    pub storage_usage: StorageUsage,

    /// Last updated timestamp
    pub updated_at: SystemTime,
}

/// Current VM resource usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmResourceUsage {
    /// Number of active VMs
    pub active_vms: u32,

    /// Total vCPUs in use
    pub used_vcpus: u32,

    /// Total memory in use (MB)
    pub used_memory_mb: u64,

    /// Total disk space in use (GB)
    pub used_disk_gb: u64,

    /// VMs per node distribution
    pub vms_per_node: HashMap<u64, u32>, // node_id -> vm_count
}

/// Current API usage tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiUsage {
    /// Requests in current second
    pub requests_current_second: u32,

    /// Requests in current minute
    pub requests_current_minute: u32,

    /// Requests in current hour
    pub requests_current_hour: u32,

    /// Current concurrent requests
    pub concurrent_requests: u32,

    /// Operation-specific usage
    pub operation_usage: OperationUsage,

    /// Request timestamps for rate limiting
    pub request_timestamps: Vec<SystemTime>,
}

/// Per-operation usage tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationUsage {
    /// VM creation requests in current minute
    pub vm_create_current_minute: u32,

    /// VM deletion requests in current minute
    pub vm_delete_current_minute: u32,

    /// Cluster join requests in current hour
    pub cluster_join_current_hour: u32,

    /// Status queries in current second
    pub status_query_current_second: u32,

    /// Config changes in current hour
    pub config_change_current_hour: u32,
}

/// Current storage usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageUsage {
    /// Total storage used in GB
    pub used_storage_gb: u64,

    /// Number of disk images
    pub disk_image_count: u32,

    /// Backup storage used in GB
    pub backup_storage_gb: u64,

    /// Current IOPS usage
    pub current_iops: u32,
}

/// Resource quota violation error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuotaViolation {
    /// VM limit exceeded
    VmLimitExceeded {
        limit: u32,
        current: u32,
        requested: u32,
    },

    /// CPU limit exceeded
    CpuLimitExceeded {
        limit: u32,
        current: u32,
        requested: u32,
    },

    /// Memory limit exceeded
    MemoryLimitExceeded {
        limit: u64,
        current: u64,
        requested: u64,
    },

    /// Disk limit exceeded
    DiskLimitExceeded {
        limit: u64,
        current: u64,
        requested: u64,
    },

    /// Per-node VM limit exceeded
    PerNodeVmLimitExceeded {
        node_id: u64,
        limit: u32,
        current: u32,
    },

    /// Rate limit exceeded
    RateLimitExceeded {
        operation: String,
        limit: u32,
        current: u32,
    },

    /// Storage limit exceeded
    StorageLimitExceeded {
        limit: u64,
        current: u64,
        requested: u64,
    },
}

/// Resource allocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequest {
    /// Tenant making the request
    pub tenant_id: TenantId,

    /// Target node (if specified)
    pub node_id: Option<u64>,

    /// Required vCPUs
    pub vcpus: u32,

    /// Required memory in MB
    pub memory_mb: u64,

    /// Required disk space in GB
    pub disk_gb: u64,

    /// Request timestamp
    pub timestamp: SystemTime,
}

/// API operation type for rate limiting
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiOperation {
    VmCreate,
    VmDelete,
    VmStart,
    VmStop,
    ClusterJoin,
    ClusterLeave,
    StatusQuery,
    ConfigChange,
    MetricsQuery,
}

impl std::fmt::Display for ApiOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiOperation::VmCreate => write!(f, "vm_create"),
            ApiOperation::VmDelete => write!(f, "vm_delete"),
            ApiOperation::VmStart => write!(f, "vm_start"),
            ApiOperation::VmStop => write!(f, "vm_stop"),
            ApiOperation::ClusterJoin => write!(f, "cluster_join"),
            ApiOperation::ClusterLeave => write!(f, "cluster_leave"),
            ApiOperation::StatusQuery => write!(f, "status_query"),
            ApiOperation::ConfigChange => write!(f, "config_change"),
            ApiOperation::MetricsQuery => write!(f, "metrics_query"),
        }
    }
}

// Default implementations
impl Default for VmResourceLimits {
    fn default() -> Self {
        Self {
            max_vms: constants::DEFAULT_MAX_VMS,
            max_vcpus: constants::DEFAULT_MAX_VCPUS,
            max_memory_mb: constants::DEFAULT_MAX_MEMORY_MB,
            max_disk_gb: constants::DEFAULT_MAX_DISK_GB,
            max_vms_per_node: constants::DEFAULT_MAX_VMS_PER_NODE,
            overcommit_ratio: constants::DEFAULT_OVERCOMMIT_RATIO,
            priority: constants::DEFAULT_PRIORITY,
        }
    }
}

impl Default for ApiRateLimits {
    fn default() -> Self {
        Self {
            requests_per_second: constants::DEFAULT_REQUESTS_PER_SECOND,
            burst_capacity: constants::DEFAULT_BURST_CAPACITY,
            max_concurrent_requests: constants::DEFAULT_MAX_CONCURRENT_REQUESTS,
            operation_limits: OperationLimits::default(),
        }
    }
}

impl Default for OperationLimits {
    fn default() -> Self {
        Self {
            vm_create_per_minute: constants::DEFAULT_VM_CREATE_PER_MINUTE,
            vm_delete_per_minute: constants::DEFAULT_VM_DELETE_PER_MINUTE,
            cluster_join_per_hour: constants::DEFAULT_CLUSTER_JOIN_PER_HOUR,
            status_query_per_second: constants::DEFAULT_STATUS_QUERY_PER_SECOND,
            config_change_per_hour: constants::DEFAULT_CONFIG_CHANGE_PER_HOUR,
        }
    }
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_storage_gb: constants::DEFAULT_MAX_STORAGE_GB,
            max_disk_images: constants::DEFAULT_MAX_DISK_IMAGES,
            max_image_size_gb: constants::DEFAULT_MAX_IMAGE_SIZE_GB,
            max_backup_storage_gb: constants::DEFAULT_MAX_BACKUP_STORAGE_GB,
            max_iops: constants::DEFAULT_MAX_IOPS,
        }
    }
}

impl Default for VmResourceUsage {
    fn default() -> Self {
        Self {
            active_vms: 0,
            used_vcpus: 0,
            used_memory_mb: 0,
            used_disk_gb: 0,
            vms_per_node: HashMap::new(),
        }
    }
}

impl Default for ApiUsage {
    fn default() -> Self {
        Self {
            requests_current_second: 0,
            requests_current_minute: 0,
            requests_current_hour: 0,
            concurrent_requests: 0,
            operation_usage: OperationUsage::default(),
            request_timestamps: Vec::new(),
        }
    }
}

impl Default for OperationUsage {
    fn default() -> Self {
        Self {
            vm_create_current_minute: 0,
            vm_delete_current_minute: 0,
            cluster_join_current_hour: 0,
            status_query_current_second: 0,
            config_change_current_hour: 0,
        }
    }
}

impl Default for StorageUsage {
    fn default() -> Self {
        Self {
            used_storage_gb: 0,
            disk_image_count: 0,
            backup_storage_gb: 0,
            current_iops: 0,
        }
    }
}

impl TenantQuota {
    /// Create a new tenant quota with default limits
    pub fn new(tenant_id: TenantId) -> Self {
        let now = SystemTime::now();
        Self {
            tenant_id,
            vm_limits: VmResourceLimits::default(),
            api_limits: ApiRateLimits::default(),
            storage_limits: StorageLimits::default(),
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update quota limits
    pub fn update_limits(
        &mut self,
        vm_limits: Option<VmResourceLimits>,
        api_limits: Option<ApiRateLimits>,
        storage_limits: Option<StorageLimits>,
    ) {
        if let Some(vm) = vm_limits {
            self.vm_limits = vm;
        }
        if let Some(api) = api_limits {
            self.api_limits = api;
        }
        if let Some(storage) = storage_limits {
            self.storage_limits = storage;
        }
        self.updated_at = SystemTime::now();
    }
}

impl TenantUsage {
    /// Create new tenant usage tracking
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            tenant_id,
            vm_usage: VmResourceUsage::default(),
            api_usage: ApiUsage::default(),
            storage_usage: StorageUsage::default(),
            updated_at: SystemTime::now(),
        }
    }

    /// Update VM usage after VM creation/deletion
    pub fn update_vm_usage(&mut self, vcpus: i32, memory_mb: i64, disk_gb: i64, node_id: u64) {
        // Update totals (can be negative for deletions)
        self.vm_usage.used_vcpus = (self.vm_usage.used_vcpus as i32 + vcpus).max(0) as u32;
        self.vm_usage.used_memory_mb =
            (self.vm_usage.used_memory_mb as i64 + memory_mb).max(0) as u64;
        self.vm_usage.used_disk_gb = (self.vm_usage.used_disk_gb as i64 + disk_gb).max(0) as u64;

        // Update VM counts
        if vcpus > 0 {
            self.vm_usage.active_vms += 1;
            *self.vm_usage.vms_per_node.entry(node_id).or_insert(0) += 1;
        } else if vcpus < 0 {
            self.vm_usage.active_vms = self.vm_usage.active_vms.saturating_sub(1);
            if let Some(count) = self.vm_usage.vms_per_node.get_mut(&node_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.vm_usage.vms_per_node.remove(&node_id);
                }
            }
        }

        self.updated_at = SystemTime::now();
    }
}

impl std::fmt::Display for QuotaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuotaViolation::VmLimitExceeded {
                limit,
                current,
                requested,
            } => {
                write!(
                    f,
                    "VM limit exceeded: limit={}, current={}, requested={}",
                    limit, current, requested
                )
            }
            QuotaViolation::CpuLimitExceeded {
                limit,
                current,
                requested,
            } => {
                write!(
                    f,
                    "CPU limit exceeded: limit={}, current={}, requested={}",
                    limit, current, requested
                )
            }
            QuotaViolation::MemoryLimitExceeded {
                limit,
                current,
                requested,
            } => {
                write!(
                    f,
                    "Memory limit exceeded: limit={}MB, current={}MB, requested={}MB",
                    limit, current, requested
                )
            }
            QuotaViolation::DiskLimitExceeded {
                limit,
                current,
                requested,
            } => {
                write!(
                    f,
                    "Disk limit exceeded: limit={}GB, current={}GB, requested={}GB",
                    limit, current, requested
                )
            }
            QuotaViolation::PerNodeVmLimitExceeded {
                node_id,
                limit,
                current,
            } => {
                write!(
                    f,
                    "Per-node VM limit exceeded on node {}: limit={}, current={}",
                    node_id, limit, current
                )
            }
            QuotaViolation::RateLimitExceeded {
                operation,
                limit,
                current,
            } => {
                write!(
                    f,
                    "Rate limit exceeded for {}: limit={}, current={}",
                    operation, limit, current
                )
            }
            QuotaViolation::StorageLimitExceeded {
                limit,
                current,
                requested,
            } => {
                write!(
                    f,
                    "Storage limit exceeded: limit={}GB, current={}GB, requested={}GB",
                    limit, current, requested
                )
            }
        }
    }
}

impl std::error::Error for QuotaViolation {}

/// Quota validation result
pub type QuotaResult<T> = Result<T, QuotaViolation>;
