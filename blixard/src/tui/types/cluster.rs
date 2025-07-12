//! Cluster-related types for the TUI

use std::time::Instant;

/// Information about a specific node in the cluster
///
/// Represents the current state and identity of a cluster member,
/// used for displaying node status in the TUI and tracking
/// cluster membership changes.
///
/// # Fields
///
/// * `id` - Unique numeric identifier for the node
/// * `address` - Network address for connecting to this node
/// * `state` - Current Raft state ("Leader", "Follower", "Candidate", etc.)
/// * `is_current` - Whether this is the node the TUI is connected to
#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    pub id: u64,
    pub address: String,
    pub state: String,
    pub is_current: bool,
}

/// Overall cluster status and membership information
///
/// Provides a comprehensive view of the cluster's current state,
/// including leadership, consensus status, and member nodes.
/// This is the primary data structure for cluster-wide information
/// displayed in the TUI.
///
/// # Fields
///
/// * `leader_id` - ID of the current Raft leader node
/// * `term` - Current Raft consensus term number
/// * `node_count` - Total number of nodes in the cluster
/// * `current_node_id` - ID of the node this TUI instance is connected to
/// * `current_node_state` - Raft state of the connected node
/// * `nodes` - Detailed information about all cluster members
///
/// # Examples
///
/// ```rust
/// # use blixard::tui::types::cluster::ClusterInfo;
/// // Check if cluster has a leader
/// fn has_leader(cluster: &ClusterInfo) -> bool {
///     cluster.leader_id > 0
/// }
///
/// // Get leader node info
/// fn get_leader_info(cluster: &ClusterInfo) -> Option<&blixard::tui::types::cluster::ClusterNodeInfo> {
///     cluster.nodes.iter().find(|n| n.id == cluster.leader_id)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub leader_id: u64,
    pub term: u64,
    pub node_count: usize,
    pub current_node_id: u64,
    pub current_node_state: String,
    pub nodes: Vec<ClusterNodeInfo>,
}

/// Resource usage and performance metrics for the entire cluster
///
/// Aggregates resource utilization data across all cluster nodes,
/// providing insights into capacity, usage patterns, and cluster health.
/// Used by the TUI for displaying dashboards and monitoring views.
///
/// # Fields
///
/// * `total_nodes` - Number of nodes in the cluster
/// * `healthy_nodes` - Number of nodes currently healthy
/// * `total_vms` - Total VMs across all nodes
/// * `running_vms` - Number of VMs currently running
/// * `total_cpu` - Total CPU cores available in cluster
/// * `used_cpu` - CPU cores currently allocated to VMs
/// * `total_memory` - Total memory available in MB
/// * `used_memory` - Memory currently allocated to VMs in MB
/// * `leader_id` - Current cluster leader node ID
/// * `raft_term` - Current Raft consensus term
/// * `last_updated` - Timestamp when these metrics were last refreshed
#[derive(Debug, Clone)]
pub struct ClusterMetrics {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub total_vms: u32,
    pub running_vms: u32,
    pub total_cpu: u32,
    pub used_cpu: u32,
    pub total_memory: u64,
    pub used_memory: u64,
    pub leader_id: Option<u64>,
    pub raft_term: u64,
    #[allow(dead_code)]
    pub last_updated: Instant,
}

/// Overall health status of the cluster
///
/// Represents the aggregate health state based on node availability,
/// resource utilization, and operational status. Used to provide
/// quick visual indicators of cluster state in the TUI.
///
/// # Variants
///
/// * `Healthy` - All nodes operational, resources available
/// * `Degraded` - Some nodes down but majority operational
/// * `Critical` - Majority of nodes down or severe resource constraints
/// * `Unknown` - Health status cannot be determined
#[derive(Debug, Clone, PartialEq)]
pub enum ClusterHealth {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

/// Template for creating or configuring cluster deployments
///
/// Defines a reusable cluster configuration that specifies
/// the desired cluster topology, resource allocation, and
/// operational parameters. Used for quick cluster setup
/// or capacity planning.
///
/// # Fields
///
/// * `name` - Display name for this cluster template
/// * `description` - Human-readable description of the cluster purpose
/// * `node_count` - Target number of nodes in the cluster
/// * `vm_count` - Expected number of VMs to be deployed
/// * `total_vcpus` - Total CPU cores across all nodes
/// * `total_memory_gb` - Total memory across all nodes in GB
/// * `replication_factor` - Raft replication factor for data safety
#[derive(Debug, Clone)]
pub struct ClusterTemplate {
    pub name: String,
    pub description: String,
    pub node_count: u32,
    pub vm_count: u32,
    pub total_vcpus: u32,
    pub total_memory_gb: u32,
    pub replication_factor: u32,
}

/// Information about a cluster discovered through service discovery
///
/// Represents a cluster that was found via network discovery mechanisms
/// (mDNS, DNS-SD, etc.) and is available for connection. Used by the
/// TUI to present users with available clusters to join.
///
/// # Fields
///
/// * `id` - Unique identifier for the discovered cluster
/// * `leader_address` - Network address of the cluster leader
/// * `node_count` - Number of nodes detected in the cluster
/// * `discovered_at` - Timestamp when this cluster was first discovered
/// * `is_compatible` - Whether this cluster is compatible with the current client
#[derive(Debug, Clone)]
pub struct DiscoveredCluster {
    pub id: String,
    pub leader_address: String,
    pub node_count: u32,
    pub discovered_at: Instant,
    pub is_compatible: bool,
}