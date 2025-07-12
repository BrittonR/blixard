//! Node-related types for the TUI

use std::time::Instant;

/// Information about a cluster node and its current state
///
/// Represents a single node in the Blixard cluster, including
/// its role in Raft consensus, resource utilization, and health status.
/// Used throughout the TUI for node monitoring and management.
///
/// # Fields
///
/// * `id` - Unique numeric identifier for this node
/// * `address` - Network address for connecting to this node
/// * `status` - Current health status (Healthy, Warning, Critical, Offline)
/// * `role` - Current Raft role (Leader, Follower, Candidate)
/// * `cpu_usage` - Current CPU utilization percentage (0.0-100.0)
/// * `memory_usage` - Current memory utilization percentage (0.0-100.0)
/// * `vm_count` - Number of VMs currently running on this node
/// * `last_seen` - Timestamp of last successful communication
/// * `version` - Software version running on this node (if available)
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub status: NodeStatus,
    pub role: NodeRole,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub vm_count: u32,
    pub last_seen: Option<Instant>,
    #[allow(dead_code)]
    pub version: Option<String>,
}

/// Health status of a cluster node
///
/// Indicates the operational state of a node based on resource usage,
/// connectivity, and service responsiveness. Used to provide visual
/// health indicators in the TUI and trigger alerts.
///
/// # Variants
///
/// * `Healthy` - Node is operating normally
/// * `Warning` - Node has minor issues (high resource usage, etc.)
/// * `Critical` - Node has serious issues affecting availability
/// * `Offline` - Node is unreachable or disconnected
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Healthy,
    Warning,
    Critical,
    #[allow(dead_code)]
    Offline,
}

impl NodeStatus {
    /// Get the color associated with this node status for TUI display
    ///
    /// Returns appropriate colors for visual indication of node health:
    /// - Healthy: Green
    /// - Warning: Yellow  
    /// - Critical: Red
    /// - Offline: Gray
    ///
    /// # Returns
    ///
    /// A `ratatui::style::Color` suitable for styling TUI elements.
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            NodeStatus::Healthy => Color::Green,
            NodeStatus::Warning => Color::Yellow,
            NodeStatus::Critical => Color::Red,
            NodeStatus::Offline => Color::Gray,
        }
    }
}

/// Role of a node in the Raft consensus protocol
///
/// Indicates the current role of a node in the distributed consensus
/// algorithm. Roles can change dynamically based on elections and
/// network conditions.
///
/// # Variants
///
/// * `Leader` - Node is the current Raft leader, handles all writes
/// * `Follower` - Node follows the leader and replicates data
/// * `Candidate` - Node is attempting to become leader during election
/// * `Unknown` - Node role cannot be determined
#[derive(Debug, Clone, PartialEq)]
pub enum NodeRole {
    Leader,
    Follower,
    #[allow(dead_code)]
    Candidate,
    #[allow(dead_code)]
    Unknown,
}

/// Filter criteria for displaying nodes in the TUI
///
/// Allows users to filter the node list based on health status,
/// Raft role, or other criteria. Used by the TUI to show relevant
/// subsets of cluster nodes.
///
/// # Variants
///
/// * `All` - Show all nodes regardless of status
/// * `Healthy` - Show only nodes with Healthy status
/// * `Unhealthy` - Show nodes with Warning, Critical, or Offline status
/// * `Warning` - Show only nodes with Warning status
/// * `Critical` - Show only nodes with Critical status
/// * `Leader` - Show the current leader node
/// * `Leaders` - Show all nodes that are or have been leaders
/// * `Followers` - Show only follower nodes
#[derive(Debug, Clone, PartialEq)]
pub enum NodeFilter {
    All,
    Healthy,
    Unhealthy,
    Warning,
    Critical,
    Leader,
    Leaders,
    Followers,
}

/// Template configuration for provisioning new cluster nodes
///
/// Defines the hardware specification and capabilities for a
/// cluster node. Used for capacity planning and automated
/// node provisioning.
///
/// # Fields
///
/// * `name` - Display name for this node template
/// * `cpu_cores` - Number of physical CPU cores
/// * `memory_gb` - Total memory capacity in GB
/// * `disk_gb` - Total disk capacity in GB
/// * `features` - Special capabilities (GPU, high-memory, etc.)
/// * `location` - Physical or logical location identifier
#[derive(Debug, Clone)]
pub struct NodeTemplate {
    pub name: String,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub disk_gb: u32,
    pub features: Vec<String>,
    pub location: Option<String>,
}

/// Detailed resource utilization information for a cluster node
///
/// Provides comprehensive resource usage data for capacity planning,
/// VM placement decisions, and resource monitoring. Used by the
/// scheduler and monitoring systems.
///
/// # Fields
///
/// * `node_id` - Unique identifier for this node
/// * `cpu_cores` - Total number of CPU cores available
/// * `memory_mb` - Total memory capacity in MB
/// * `disk_gb` - Total disk capacity in GB
/// * `used_vcpus` - Virtual CPU cores currently allocated to VMs
/// * `used_memory_mb` - Memory currently allocated to VMs in MB
/// * `used_disk_gb` - Disk space currently used by VMs in GB
/// * `running_vms` - Number of VMs currently running on this node
/// * `features` - Special capabilities available on this node
#[derive(Debug, Clone)]
pub struct NodeResourceInfo {
    pub node_id: u64,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
    pub features: Vec<String>,
}