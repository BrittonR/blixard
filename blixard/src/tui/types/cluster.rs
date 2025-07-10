//! Cluster-related types for the TUI

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    pub id: u64,
    pub address: String,
    pub state: String,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub leader_id: u64,
    pub term: u64,
    pub node_count: usize,
    pub current_node_id: u64,
    pub current_node_state: String,
    pub nodes: Vec<ClusterNodeInfo>,
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum ClusterHealth {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

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

#[derive(Debug, Clone)]
pub struct DiscoveredCluster {
    pub id: String,
    pub leader_address: String,
    pub node_count: u32,
    pub discovered_at: Instant,
    pub is_compatible: bool,
}