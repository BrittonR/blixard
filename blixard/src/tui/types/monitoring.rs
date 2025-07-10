//! Monitoring-related types for the TUI

use super::node::{NodeStatus, NodeResourceInfo};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub timestamp: Instant,
    pub level: EventLevel,
    pub source: String,
    pub message: String,
    #[allow(dead_code)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventLevel {
    Info,
    Warning,
    Error,
    Critical,
    Debug,
}

impl EventLevel {
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            EventLevel::Info => Color::Cyan,
            EventLevel::Warning => Color::Yellow,
            EventLevel::Error => Color::Red,
            EventLevel::Critical => Color::Magenta,
            EventLevel::Debug => Color::Gray,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            EventLevel::Info => "ℹ️",
            EventLevel::Warning => "⚠️",
            EventLevel::Error => "❌",
            EventLevel::Critical => "🚨",
            EventLevel::Debug => "🔍",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeHealthSnapshot {
    pub timestamp: Instant,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_io_mbps: f32,
    pub network_mbps: f32,
    pub vm_count: u32,
    pub status: NodeStatus,
}

#[derive(Debug, Clone)]
pub struct HealthAlert {
    pub timestamp: Instant,
    pub severity: AlertSeverity,
    pub node_id: Option<u64>,
    pub title: String,
    pub message: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ClusterResourceInfo {
    pub total_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub total_running_vms: u32,
    pub nodes: Vec<NodeResourceInfo>,
}