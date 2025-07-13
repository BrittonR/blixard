//! Cluster management API schemas

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;
use super::ResourceMetadata;

/// Node status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatusDto {
    /// Node is uninitialized
    Uninitialized,
    /// Node is initialized but not part of cluster
    Initialized,
    /// Node is joining the cluster
    JoiningCluster,
    /// Node is active in the cluster
    Active,
    /// Node is leaving the cluster
    LeavingCluster,
    /// Node is in error state
    Error,
}

/// Request to join a cluster
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct JoinClusterRequest {
    /// Address of a cluster node to join through
    #[validate(length(min = 1, max = 255))]
    pub cluster_address: String,
    
    /// Optional authentication token
    #[validate(length(min = 1, max = 1024))]
    pub auth_token: Option<String>,
    
    /// Join timeout in seconds (default: 30)
    #[validate(range(min = 5, max = 300))]
    #[serde(default = "default_join_timeout")]
    pub timeout_seconds: u32,
    
    /// Whether to force join even if already in a cluster
    #[serde(default)]
    pub force: bool,
}

fn default_join_timeout() -> u32 {
    30
}

/// Response when joining a cluster
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinClusterResponse {
    /// Whether join was successful
    pub success: bool,
    
    /// Join operation message
    pub message: String,
    
    /// Assigned node ID in the cluster
    pub node_id: u64,
    
    /// Current cluster status
    pub cluster_status: ClusterStatusResponse,
    
    /// Join request ID for tracking
    pub request_id: String,
}

/// Request to leave the cluster
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct LeaveClusterRequest {
    /// Whether to force leave immediately (default: false)
    #[serde(default)]
    pub force: bool,
    
    /// Leave timeout in seconds (default: 30)
    #[validate(range(min = 5, max = 300))]
    #[serde(default = "default_leave_timeout")]
    pub timeout_seconds: u32,
    
    /// Whether to drain VMs before leaving (default: true)
    #[serde(default = "default_drain_vms")]
    pub drain_vms: bool,
}

fn default_leave_timeout() -> u32 {
    30
}

fn default_drain_vms() -> bool {
    true
}

/// Response when leaving a cluster
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaveClusterResponse {
    /// Whether leave was successful
    pub success: bool,
    
    /// Leave operation message
    pub message: String,
    
    /// Number of VMs that were drained
    pub drained_vms: u32,
    
    /// Leave request ID for tracking
    pub request_id: String,
}

/// Cluster status information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClusterStatusResponse {
    /// Cluster unique identifier
    pub cluster_id: String,
    
    /// Total number of nodes in cluster
    pub total_nodes: u32,
    
    /// Number of active (healthy) nodes
    pub active_nodes: u32,
    
    /// Current leader node ID
    pub leader_node_id: Option<u64>,
    
    /// This node's ID
    pub node_id: u64,
    
    /// This node's status
    pub node_status: NodeStatusDto,
    
    /// Whether this node is the cluster leader
    pub is_leader: bool,
    
    /// Raft term information
    pub raft_term: u64,
    
    /// Raft log index information
    pub raft_log_index: u64,
    
    /// List of all cluster nodes
    pub nodes: Vec<NodeInfo>,
    
    /// Cluster resource summary
    pub resources: ClusterResourceSummary,
}

/// Information about a cluster node
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeInfo {
    /// Node ID
    pub id: u64,
    
    /// Node network address
    pub address: String,
    
    /// Node status
    pub status: NodeStatusDto,
    
    /// Whether node is currently the leader
    pub is_leader: bool,
    
    /// Node capabilities and resources
    pub capabilities: NodeCapabilities,
    
    /// Node resource metadata
    #[serde(flatten)]
    pub metadata: ResourceMetadata,
    
    /// Last seen timestamp (for health monitoring)
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// Node capabilities and resources
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeCapabilities {
    /// Number of CPU cores
    pub cpu_cores: u32,
    
    /// Total memory in MB
    pub memory_mb: u64,
    
    /// Total disk space in GB
    pub disk_gb: u64,
    
    /// Supported features
    pub features: Vec<String>,
    
    /// Current resource utilization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utilization: Option<NodeResourceUtilization>,
}

/// Node resource utilization
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeResourceUtilization {
    /// CPU cores currently allocated
    pub cpu_allocated: u32,
    
    /// Memory currently allocated in MB
    pub memory_allocated_mb: u64,
    
    /// Disk space currently allocated in GB
    pub disk_allocated_gb: u64,
    
    /// Number of VMs currently running
    pub vm_count: u32,
    
    /// CPU utilization percentage (0.0-100.0)
    pub cpu_percent: f64,
    
    /// Memory utilization percentage (0.0-100.0)
    pub memory_percent: f64,
    
    /// Disk utilization percentage (0.0-100.0)
    pub disk_percent: f64,
}

/// Cluster-wide resource summary
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClusterResourceSummary {
    /// Total CPU cores across all nodes
    pub total_cpu_cores: u32,
    
    /// Total memory across all nodes in MB
    pub total_memory_mb: u64,
    
    /// Total disk space across all nodes in GB
    pub total_disk_gb: u64,
    
    /// Currently allocated CPU cores
    pub allocated_cpu_cores: u32,
    
    /// Currently allocated memory in MB
    pub allocated_memory_mb: u64,
    
    /// Currently allocated disk space in GB
    pub allocated_disk_gb: u64,
    
    /// Total number of VMs across cluster
    pub total_vms: u32,
    
    /// Cluster-wide utilization percentages
    pub utilization: ClusterResourceUtilization,
}

/// Cluster-wide resource utilization
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClusterResourceUtilization {
    /// CPU utilization percentage across cluster (0.0-100.0)
    pub cpu_percent: f64,
    
    /// Memory utilization percentage across cluster (0.0-100.0)
    pub memory_percent: f64,
    
    /// Disk utilization percentage across cluster (0.0-100.0)
    pub disk_percent: f64,
    
    /// Average node utilization (0.0-100.0)
    pub average_node_utilization: f64,
}

/// Query parameters for listing nodes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct ListNodesParams {
    /// Filter by node status
    pub status: Option<NodeStatusDto>,
    
    /// Include detailed resource utilization
    #[serde(default)]
    pub include_utilization: bool,
    
    /// Sort field (id, address, status, last_seen)
    #[validate(custom = "validate_node_sort_field")]
    pub sort_by: Option<String>,
    
    /// Sort order (asc, desc)
    #[validate(custom = "validate_sort_order")]
    pub sort_order: Option<String>,
}

fn validate_node_sort_field(field: &str) -> Result<(), validator::ValidationError> {
    match field {
        "id" | "address" | "status" | "last_seen" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_sort_field")),
    }
}

fn validate_sort_order(order: &str) -> Result<(), validator::ValidationError> {
    match order {
        "asc" | "desc" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_sort_order")),
    }
}

/// Convert from internal types to API DTOs
impl From<crate::types::NodeState> for NodeStatusDto {
    fn from(state: crate::types::NodeState) -> Self {
        match state {
            crate::types::NodeState::Uninitialized => NodeStatusDto::Uninitialized,
            crate::types::NodeState::Initialized => NodeStatusDto::Initialized,
            crate::types::NodeState::JoiningCluster => NodeStatusDto::JoiningCluster,
            crate::types::NodeState::Active => NodeStatusDto::Active,
            crate::types::NodeState::LeavingCluster => NodeStatusDto::LeavingCluster,
            crate::types::NodeState::Error => NodeStatusDto::Error,
        }
    }
}

impl From<NodeStatusDto> for crate::types::NodeState {
    fn from(status: NodeStatusDto) -> Self {
        match status {
            NodeStatusDto::Uninitialized => crate::types::NodeState::Uninitialized,
            NodeStatusDto::Initialized => crate::types::NodeState::Initialized,
            NodeStatusDto::JoiningCluster => crate::types::NodeState::JoiningCluster,
            NodeStatusDto::Active => crate::types::NodeState::Active,
            NodeStatusDto::LeavingCluster => crate::types::NodeState::LeavingCluster,
            NodeStatusDto::Error => crate::types::NodeState::Error,
        }
    }
}