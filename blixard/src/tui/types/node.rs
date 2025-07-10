//! Node-related types for the TUI

use std::time::Instant;

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

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Healthy,
    Warning,
    Critical,
    #[allow(dead_code)]
    Offline,
}

impl NodeStatus {
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

#[derive(Debug, Clone, PartialEq)]
pub enum NodeRole {
    Leader,
    Follower,
    #[allow(dead_code)]
    Candidate,
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeFilter {
    All,
    Healthy,
    Unhealthy,
    Leader,
    Followers,
}

#[derive(Debug, Clone)]
pub struct NodeTemplate {
    pub name: String,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub disk_gb: u32,
    pub features: Vec<String>,
    pub location: Option<String>,
}

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