//! Monitoring-related state management

use crate::tui::types::monitoring::{SystemEvent, NodeHealthSnapshot, HealthAlert, ClusterResourceInfo};
use crate::tui::types::cluster::{ClusterMetrics, ClusterHealth};
use crate::tui::types::ui::{LogEntry, LogStreamConfig};
use ratatui::widgets::ListState;
use std::collections::HashMap;

#[derive(Debug)]
pub struct MonitoringState {
    // Metrics
    pub cluster_metrics: ClusterMetrics,
    pub cluster_health: ClusterHealth,
    pub cluster_resource_info: Option<ClusterResourceInfo>,
    
    // Health tracking
    pub node_health_history: HashMap<u64, Vec<NodeHealthSnapshot>>,
    pub health_alerts: Vec<HealthAlert>,
    
    // Events
    pub recent_events: Vec<SystemEvent>,
    pub max_events: usize,
    
    // Performance history
    pub cpu_history: Vec<f32>,
    pub memory_history: Vec<f32>,
    pub network_history: Vec<f32>,
    pub max_history_points: usize,
    
    // Log streaming
    pub log_stream_config: LogStreamConfig,
    pub log_entries: Vec<LogEntry>,
    pub log_list_state: ListState,
    pub max_log_entries: usize,
}

impl MonitoringState {
    pub fn new() -> Self {
        Self {
            cluster_metrics: ClusterMetrics {
                total_nodes: 0,
                healthy_nodes: 0,
                total_vms: 0,
                running_vms: 0,
                total_cpu: 0,
                used_cpu: 0,
                total_memory: 0,
                used_memory: 0,
                leader_id: None,
                raft_term: 0,
                last_updated: std::time::Instant::now(),
            },
            cluster_health: ClusterHealth::Unknown,
            cluster_resource_info: None,
            node_health_history: HashMap::new(),
            health_alerts: Vec::new(),
            recent_events: Vec::new(),
            max_events: 100,
            cpu_history: Vec::new(),
            memory_history: Vec::new(),
            network_history: Vec::new(),
            max_history_points: 60,
            log_stream_config: LogStreamConfig {
                enabled: false,
                sources: Vec::new(),
                level_filter: crate::tui::types::ui::LogLevel::Info,
                max_buffer_size: 1000,
            },
            log_entries: Vec::new(),
            log_list_state: ListState::default(),
            max_log_entries: 500,
        }
    }

    pub fn add_event(&mut self, event: SystemEvent) {
        self.recent_events.push(event);
        if self.recent_events.len() > self.max_events {
            self.recent_events.remove(0);
        }
    }

    pub fn add_log_entry(&mut self, entry: LogEntry) {
        self.log_entries.push(entry);
        if self.log_entries.len() > self.max_log_entries {
            self.log_entries.remove(0);
        }
    }

    pub fn update_metrics_history(&mut self) {
        // Calculate current metrics
        let cpu_usage = if self.cluster_metrics.total_cpu > 0 {
            (self.cluster_metrics.used_cpu as f32 / self.cluster_metrics.total_cpu as f32) * 100.0
        } else {
            0.0
        };

        let memory_usage = if self.cluster_metrics.total_memory > 0 {
            (self.cluster_metrics.used_memory as f32 / self.cluster_metrics.total_memory as f32) * 100.0
        } else {
            0.0
        };

        // Add to history
        self.cpu_history.push(cpu_usage);
        self.memory_history.push(memory_usage);
        self.network_history.push(0.0); // Placeholder

        // Trim history
        if self.cpu_history.len() > self.max_history_points {
            self.cpu_history.remove(0);
        }
        if self.memory_history.len() > self.max_history_points {
            self.memory_history.remove(0);
        }
        if self.network_history.len() > self.max_history_points {
            self.network_history.remove(0);
        }
    }
}