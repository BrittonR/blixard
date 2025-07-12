//! VM-related types for the TUI

use blixard_core::types::VmStatus;

/// Information about a virtual machine in the cluster
///
/// Represents the current state and configuration of a VM,
/// including resource allocation, placement, and runtime status.
/// This is the primary data structure used throughout the TUI
/// for displaying and managing virtual machines.
///
/// # Fields
///
/// * `name` - Unique identifier for the VM
/// * `status` - Current operational state (Running, Stopped, etc.)
/// * `vcpus` - Number of virtual CPU cores allocated
/// * `memory` - Memory allocation in MB
/// * `node_id` - ID of the cluster node hosting this VM
/// * `ip_address` - Network IP address if assigned
/// * `uptime` - How long the VM has been running (if available)
/// * `cpu_usage` - Current CPU utilization percentage (if monitored)
/// * `memory_usage` - Current memory utilization percentage (if monitored)
#[derive(Debug, Clone)]
pub struct VmInfo {
    pub name: String,
    pub status: VmStatus,
    pub vcpus: u32,
    pub memory: u32,
    pub node_id: u64,
    pub ip_address: Option<String>,
    pub uptime: Option<String>,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<f32>,
    #[allow(dead_code)]
    pub placement_strategy: Option<String>,
    #[allow(dead_code)]
    pub created_at: Option<String>,
    #[allow(dead_code)]
    pub config_path: Option<String>,
}

/// Filter criteria for displaying VMs in the TUI
///
/// Allows users to filter the VM list based on various criteria
/// such as status, location, or name patterns. Used by the TUI
/// to show relevant subsets of VMs.
///
/// # Variants
///
/// * `All` - Show all VMs regardless of status
/// * `Running` - Show only VMs in Running state
/// * `Stopped` - Show only VMs in Stopped state
/// * `Failed` - Show only VMs in Failed state
/// * `ByNode(u64)` - Show only VMs on a specific node
/// * `ByName(String)` - Show VMs matching a name pattern
#[derive(Debug, Clone, PartialEq)]
pub enum VmFilter {
    All,
    Running,
    Stopped,
    Failed,
    ByNode(u64),
    ByName(String),
}

/// Template configuration for creating new VMs
///
/// Defines a reusable VM configuration that users can select
/// when creating new virtual machines. Templates provide
/// preset resource allocations and feature sets for common
/// use cases.
///
/// # Fields
///
/// * `name` - Display name for the template
/// * `description` - Human-readable description of the template's purpose
/// * `vcpus` - Default number of virtual CPU cores
/// * `memory` - Default memory allocation in MB
/// * `disk_gb` - Default disk size in GB
/// * `template_type` - Category/type of workload this template is for
/// * `features` - List of special features or capabilities enabled
#[derive(Debug, Clone)]
pub struct VmTemplate {
    pub name: String,
    pub description: String,
    pub vcpus: u32,
    pub memory: u32,
    pub disk_gb: u32,
    pub template_type: VmTemplateType,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmTemplateType {
    Development,
    WebServer,
    Database,
    Container,
    LoadBalancer,
    Custom,
}

/// Strategy for placing VMs on cluster nodes
///
/// Defines how the scheduler should select a target node
/// when creating or migrating virtual machines. Each strategy
/// optimizes for different goals like resource utilization,
/// load balancing, or administrative control.
///
/// # Variants
///
/// * `MostAvailable` - Place on node with highest available resources
/// * `LeastAvailable` - Place on node with lowest available resources (bin packing)
/// * `RoundRobin` - Distribute VMs evenly across all nodes
/// * `Manual` - User specifies the target node explicitly
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementStrategy {
    MostAvailable,
    LeastAvailable,
    RoundRobin,
    Manual,
}

impl PlacementStrategy {
    /// Get a human-readable string representation of the placement strategy
    ///
    /// Returns a user-friendly name suitable for display in the TUI.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use blixard::tui::types::vm::PlacementStrategy;
    ///
    /// assert_eq!(PlacementStrategy::MostAvailable.as_str(), "Most Available");
    /// assert_eq!(PlacementStrategy::RoundRobin.as_str(), "Round Robin");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            PlacementStrategy::MostAvailable => "Most Available",
            PlacementStrategy::LeastAvailable => "Least Available",
            PlacementStrategy::RoundRobin => "Round Robin",
            PlacementStrategy::Manual => "Manual",
        }
    }

    /// Get all available placement strategies
    ///
    /// Returns a vector containing all placement strategy variants,
    /// useful for populating UI selection lists.
    ///
    /// # Returns
    ///
    /// A vector containing all `PlacementStrategy` variants in a logical order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use blixard::tui::types::vm::PlacementStrategy;
    ///
    /// let strategies = PlacementStrategy::all();
    /// assert!(strategies.len() > 0);
    /// assert!(strategies.contains(&PlacementStrategy::MostAvailable));
    /// ```
    pub fn all() -> Vec<PlacementStrategy> {
        vec![
            PlacementStrategy::MostAvailable,
            PlacementStrategy::LeastAvailable,
            PlacementStrategy::RoundRobin,
            PlacementStrategy::Manual,
        ]
    }
}