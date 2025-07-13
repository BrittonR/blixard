//! VM management API schemas

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;
use super::ResourceMetadata;

/// VM status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VmStatusDto {
    /// VM is being created
    Creating,
    /// VM is starting up
    Starting,
    /// VM is running and operational
    Running,
    /// VM is stopping
    Stopping,
    /// VM is stopped
    Stopped,
    /// VM has failed
    Failed,
}

/// Hypervisor type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HypervisorDto {
    /// Cloud Hypervisor
    CloudHypervisor,
    /// Firecracker VMM
    Firecracker,
    /// QEMU
    Qemu,
}

/// Request to create a new VM
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateVmRequest {
    /// Unique VM name (3-63 characters, alphanumeric and hyphens)
    #[validate(length(min = 3, max = 63), regex = "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$")]
    pub name: String,
    
    /// Number of virtual CPUs (1-64)
    #[validate(range(min = 1, max = 64))]
    pub vcpus: u32,
    
    /// Memory in MB (512-65536)
    #[validate(range(min = 512, max = 65536))]
    pub memory_mb: u32,
    
    /// Hypervisor type
    pub hypervisor: HypervisorDto,
    
    /// Optional tenant ID for multi-tenancy
    #[validate(length(min = 1, max = 255))]
    pub tenant_id: Option<String>,
    
    /// VM priority (0-1000, higher is more important)
    #[validate(range(min = 0, max = 1000))]
    #[serde(default = "default_priority")]
    pub priority: u32,
    
    /// Whether this VM can be preempted for higher priority VMs
    #[serde(default = "default_preemptible")]
    pub preemptible: bool,
    
    /// Optional metadata tags
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    
    /// Target node ID for VM placement (optional - uses scheduler if not specified)
    pub target_node_id: Option<u64>,
}

fn default_priority() -> u32 {
    500
}

fn default_preemptible() -> bool {
    true
}

/// Response when creating a VM
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateVmResponse {
    /// Created VM information
    pub vm: VmInfo,
    
    /// Node ID where the VM was placed
    pub node_id: u64,
    
    /// Creation request ID for tracking
    pub request_id: String,
}

/// VM information response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VmInfo {
    /// VM name
    pub name: String,
    
    /// Current VM status
    pub status: VmStatusDto,
    
    /// VM configuration
    pub config: VmConfigDto,
    
    /// Node ID where VM is running
    pub node_id: u64,
    
    /// Resource metadata
    #[serde(flatten)]
    pub metadata: ResourceMetadata,
    
    /// VM runtime information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<VmRuntimeInfo>,
}

/// VM configuration details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VmConfigDto {
    /// Number of virtual CPUs
    pub vcpus: u32,
    
    /// Memory in MB
    pub memory_mb: u32,
    
    /// Hypervisor type
    pub hypervisor: HypervisorDto,
    
    /// Tenant ID
    pub tenant_id: String,
    
    /// VM priority
    pub priority: u32,
    
    /// Whether VM is preemptible
    pub preemptible: bool,
    
    /// VM metadata/labels
    pub metadata: std::collections::HashMap<String, String>,
}

/// VM runtime information (only available when running)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VmRuntimeInfo {
    /// VM process ID (if available)
    pub pid: Option<u32>,
    
    /// VM IP address (if available)
    pub ip_address: Option<String>,
    
    /// Resource usage statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_usage: Option<VmResourceUsage>,
    
    /// VM uptime in seconds
    pub uptime_seconds: Option<u64>,
}

/// VM resource usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VmResourceUsage {
    /// CPU usage percentage (0.0-100.0)
    pub cpu_percent: f64,
    
    /// Memory usage in MB
    pub memory_used_mb: u64,
    
    /// Memory usage percentage (0.0-100.0)
    pub memory_percent: f64,
    
    /// Disk I/O read bytes
    pub disk_read_bytes: u64,
    
    /// Disk I/O write bytes
    pub disk_write_bytes: u64,
    
    /// Network received bytes
    pub network_rx_bytes: u64,
    
    /// Network transmitted bytes
    pub network_tx_bytes: u64,
}

/// Request to start a VM
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct StartVmRequest {
    /// Optional timeout in seconds (default: 30)
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: Option<u32>,
}

/// Request to stop a VM
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct StopVmRequest {
    /// Whether to force stop the VM (default: false)
    #[serde(default)]
    pub force: bool,
    
    /// Optional timeout in seconds (default: 30)
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: Option<u32>,
}

/// VM operation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VmOperationResponse {
    /// Operation that was performed
    pub operation: String,
    
    /// VM name
    pub vm_name: String,
    
    /// Whether operation was successful
    pub success: bool,
    
    /// Operation message
    pub message: String,
    
    /// Operation request ID
    pub request_id: String,
    
    /// Updated VM information (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<VmInfo>,
}

/// List VMs query parameters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct ListVmsParams {
    /// Filter by VM status
    pub status: Option<VmStatusDto>,
    
    /// Filter by tenant ID
    #[validate(length(min = 1, max = 255))]
    pub tenant_id: Option<String>,
    
    /// Filter by node ID
    pub node_id: Option<u64>,
    
    /// Filter by metadata tags (key:value format)
    pub tags: Option<Vec<String>>,
    
    /// Sort field (name, created_at, updated_at, status)
    #[validate(custom = "validate_sort_field")]
    pub sort_by: Option<String>,
    
    /// Sort order (asc, desc)
    #[validate(custom = "validate_sort_order")]
    pub sort_order: Option<String>,
}

fn validate_sort_field(field: &str) -> Result<(), validator::ValidationError> {
    match field {
        "name" | "created_at" | "updated_at" | "status" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_sort_field")),
    }
}

fn validate_sort_order(order: &str) -> Result<(), validator::ValidationError> {
    match order {
        "asc" | "desc" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_sort_order")),
    }
}

/// VM migration request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct MigrateVmRequest {
    /// Target node ID for migration
    pub target_node_id: u64,
    
    /// Whether to perform live migration (default: false)
    #[serde(default)]
    pub live_migration: bool,
    
    /// Whether to force migration even if resource checks fail
    #[serde(default)]
    pub force: bool,
    
    /// Migration timeout in seconds (default: 300)
    #[validate(range(min = 30, max = 1800))]
    #[serde(default = "default_migration_timeout")]
    pub timeout_seconds: u32,
}

fn default_migration_timeout() -> u32 {
    300
}

/// Convert from internal types to API DTOs
impl From<crate::types::VmStatus> for VmStatusDto {
    fn from(status: crate::types::VmStatus) -> Self {
        match status {
            crate::types::VmStatus::Creating => VmStatusDto::Creating,
            crate::types::VmStatus::Starting => VmStatusDto::Starting,
            crate::types::VmStatus::Running => VmStatusDto::Running,
            crate::types::VmStatus::Stopping => VmStatusDto::Stopping,
            crate::types::VmStatus::Stopped => VmStatusDto::Stopped,
            crate::types::VmStatus::Failed => VmStatusDto::Failed,
        }
    }
}

impl From<VmStatusDto> for crate::types::VmStatus {
    fn from(status: VmStatusDto) -> Self {
        match status {
            VmStatusDto::Creating => crate::types::VmStatus::Creating,
            VmStatusDto::Starting => crate::types::VmStatus::Starting,
            VmStatusDto::Running => crate::types::VmStatus::Running,
            VmStatusDto::Stopping => crate::types::VmStatus::Stopping,
            VmStatusDto::Stopped => crate::types::VmStatus::Stopped,
            VmStatusDto::Failed => crate::types::VmStatus::Failed,
        }
    }
}

impl From<crate::types::Hypervisor> for HypervisorDto {
    fn from(hypervisor: crate::types::Hypervisor) -> Self {
        match hypervisor {
            crate::types::Hypervisor::CloudHypervisor => HypervisorDto::CloudHypervisor,
            crate::types::Hypervisor::Firecracker => HypervisorDto::Firecracker,
            crate::types::Hypervisor::Qemu => HypervisorDto::Qemu,
        }
    }
}

impl From<HypervisorDto> for crate::types::Hypervisor {
    fn from(hypervisor: HypervisorDto) -> Self {
        match hypervisor {
            HypervisorDto::CloudHypervisor => crate::types::Hypervisor::CloudHypervisor,
            HypervisorDto::Firecracker => crate::types::Hypervisor::Firecracker,
            HypervisorDto::Qemu => crate::types::Hypervisor::Qemu,
        }
    }
}