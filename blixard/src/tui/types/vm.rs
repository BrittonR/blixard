//! VM-related types for the TUI

use blixard_core::types::VmStatus;

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

#[derive(Debug, Clone, PartialEq)]
pub enum VmFilter {
    All,
    Running,
    Stopped,
    Failed,
    ByNode(u64),
    ByName(String),
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementStrategy {
    MostAvailable,
    LeastAvailable,
    RoundRobin,
    Manual,
}

impl PlacementStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlacementStrategy::MostAvailable => "Most Available",
            PlacementStrategy::LeastAvailable => "Least Available",
            PlacementStrategy::RoundRobin => "Round Robin",
            PlacementStrategy::Manual => "Manual",
        }
    }

    pub fn all() -> Vec<PlacementStrategy> {
        vec![
            PlacementStrategy::MostAvailable,
            PlacementStrategy::LeastAvailable,
            PlacementStrategy::RoundRobin,
            PlacementStrategy::Manual,
        ]
    }
}