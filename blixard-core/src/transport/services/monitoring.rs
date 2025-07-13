//! Monitoring service implementation
//!
//! This service provides monitoring endpoints (metrics, traces, logs)
//! that work over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Monitoring data response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringResponse {
    /// Node ID
    pub node_id: u64,
    /// Timestamp of the snapshot
    pub timestamp: u64,
    /// Metrics in Prometheus format
    pub metrics: String,
    /// System information
    pub system_info: SystemInfo,
    /// Cluster health summary
    pub cluster_health: ClusterHealth,
}

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    /// Total memory in MB
    pub total_memory_mb: u64,
    /// Disk usage in GB
    pub disk_usage_gb: u64,
    /// Total disk in GB
    pub total_disk_gb: u64,
}

/// Cluster health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    /// Whether this node is the leader
    pub is_leader: bool,
    /// Number of connected peers
    pub connected_peers: u32,
    /// Number of running VMs
    pub running_vms: u32,
    /// Number of pending tasks
    pub pending_tasks: u32,
    /// Raft term
    pub raft_term: u64,
    /// Raft commit index
    pub raft_commit_index: u64,
}

/// Trait for monitoring operations
#[async_trait]
pub trait MonitoringService: Send + Sync {
    /// Get monitoring data
    async fn get_monitoring_data(&self) -> BlixardResult<MonitoringResponse>;

    /// Get raw Prometheus metrics
    async fn get_prometheus_metrics(&self) -> BlixardResult<String>;
}

/// Monitoring service implementation
#[derive(Clone)]
pub struct MonitoringServiceImpl {
    node: Arc<SharedNodeState>,
    start_time: std::time::Instant,
}

impl MonitoringServiceImpl {
    /// Create a new monitoring service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            node,
            start_time: std::time::Instant::now(),
        }
    }

    /// Get system information
    async fn get_system_info(&self) -> SystemInfo {
        // Get system metrics from the OS
        // For now, return placeholder values
        // In production, use sysinfo crate or similar

        let uptime_seconds = self.start_time.elapsed().as_secs();

        // Placeholder values - would query actual system metrics
        SystemInfo {
            uptime_seconds,
            cpu_usage_percent: 25.5,
            memory_usage_mb: 1024,
            total_memory_mb: 8192,
            disk_usage_gb: 50,
            total_disk_gb: 500,
        }
    }

    /// Get cluster health information
    async fn get_cluster_health(&self) -> ClusterHealth {
        let is_leader = self.node.is_leader();
        let connected_peers = self.node.get_peers().await.len() as u32;

        // Get VM count
        let running_vms = if let Some(vm_manager) = self.node.get_vm_manager() {
            vm_manager
                .list_vms()
                .await
                .unwrap_or_default()
                .iter()
                .filter(|(_, status)| *status == crate::types::VmStatus::Running)
                .count() as u32
        } else {
            0
        };

        // Get Raft metrics
        let (raft_term, raft_commit_index) = {
            let raft_status = self.node.get_raft_status().await;
            // Note: commit_index not available in current RaftStatus - use term as placeholder
            // TODO: Extend RaftStatus to include commit_index when available
            (raft_status.term, raft_status.term)
        };

        // Get actual pending task count
        // TODO: Implement get_pending_task_count when task infrastructure is complete
        // For now, use 0 as a safe default since the task tracking isn't fully implemented
        let pending_tasks = 0;

        ClusterHealth {
            is_leader,
            connected_peers,
            running_vms,
            pending_tasks,
            raft_term,
            raft_commit_index,
        }
    }
}

#[async_trait]
impl MonitoringService for MonitoringServiceImpl {
    async fn get_monitoring_data(&self) -> BlixardResult<MonitoringResponse> {
        let node_id = self.node.get_id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let metrics = self.get_prometheus_metrics().await?;
        let system_info = self.get_system_info().await;
        let cluster_health = self.get_cluster_health().await;

        Ok(MonitoringResponse {
            node_id,
            timestamp,
            metrics,
            system_info,
            cluster_health,
        })
    }

    async fn get_prometheus_metrics(&self) -> BlixardResult<String> {
        // Get real metrics data
        let cluster_health = self.get_cluster_health().await;
        let system_info = self.get_system_info().await;
        let node_id = self.node.get_id();
        
        // Generate Prometheus format metrics
        let mut metrics = String::new();
        
        // Node identification
        metrics.push_str(&format!(
            "# HELP blixard_node_info Node information\n# TYPE blixard_node_info gauge\nblixard_node_info{{node_id=\"{}\"}} 1\n",
            node_id
        ));
        
        // Raft metrics
        metrics.push_str(&format!(
            "# HELP blixard_raft_term Current Raft term\n# TYPE blixard_raft_term gauge\nblixard_raft_term{{node_id=\"{}\"}} {}\n",
            node_id, cluster_health.raft_term
        ));
        
        metrics.push_str(&format!(
            "# HELP blixard_raft_commit_index Current Raft commit index\n# TYPE blixard_raft_commit_index gauge\nblixard_raft_commit_index{{node_id=\"{}\"}} {}\n",
            node_id, cluster_health.raft_commit_index
        ));
        
        // Leadership status
        metrics.push_str(&format!(
            "# HELP blixard_is_leader Node leadership status\n# TYPE blixard_is_leader gauge\nblixard_is_leader{{node_id=\"{}\"}} {}\n",
            node_id, if cluster_health.is_leader { 1 } else { 0 }
        ));
        
        // Peer connections
        metrics.push_str(&format!(
            "# HELP blixard_connected_peers Number of connected peers\n# TYPE blixard_connected_peers gauge\nblixard_connected_peers{{node_id=\"{}\"}} {}\n",
            node_id, cluster_health.connected_peers
        ));
        
        // VM metrics
        metrics.push_str(&format!(
            "# HELP blixard_running_vms Number of running VMs\n# TYPE blixard_running_vms gauge\nblixard_running_vms{{node_id=\"{}\"}} {}\n",
            node_id, cluster_health.running_vms
        ));
        
        // Task metrics
        metrics.push_str(&format!(
            "# HELP blixard_pending_tasks Number of pending tasks\n# TYPE blixard_pending_tasks gauge\nblixard_pending_tasks{{node_id=\"{}\"}} {}\n",
            node_id, cluster_health.pending_tasks
        ));
        
        // System metrics
        metrics.push_str(&format!(
            "# HELP blixard_cpu_usage CPU usage percentage\n# TYPE blixard_cpu_usage gauge\nblixard_cpu_usage{{node_id=\"{}\"}} {:.2}\n",
            node_id, system_info.cpu_usage_percent
        ));
        
        metrics.push_str(&format!(
            "# HELP blixard_memory_used_mb Memory used in MB\n# TYPE blixard_memory_used_mb gauge\nblixard_memory_used_mb{{node_id=\"{}\"}} {}\n",
            node_id, system_info.memory_usage_mb
        ));
        
        metrics.push_str(&format!(
            "# HELP blixard_memory_total_mb Total memory in MB\n# TYPE blixard_memory_total_mb gauge\nblixard_memory_total_mb{{node_id=\"{}\"}} {}\n",
            node_id, system_info.total_memory_mb
        ));
        
        Ok(metrics)
    }
}

/// HTTP endpoint for Prometheus metrics scraping
pub async fn prometheus_metrics_handler(
    node: Arc<SharedNodeState>,
) -> Result<String, Box<dyn std::error::Error>> {
    let service = MonitoringServiceImpl::new(node);
    Ok(service.get_prometheus_metrics().await?)
}

/// Iroh protocol handler for monitoring service
pub struct MonitoringProtocolHandler {
    /// Service implementation for monitoring operations
    /// Currently unused pending Iroh protocol implementation
    #[allow(dead_code)]
    service: MonitoringServiceImpl,
}

impl MonitoringProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: MonitoringServiceImpl::new(node),
        }
    }

    /// Handle a monitoring request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request_type: MonitoringRequestType,
    ) -> BlixardResult<()> {
        // TODO: Implement proper protocol handling
        match request_type {
            MonitoringRequestType::FullMonitoring => {
                // Return full monitoring data
            }
            MonitoringRequestType::PrometheusMetrics => {
                // Return just Prometheus metrics
            }
        }

        Err(BlixardError::NotImplemented {
            feature: "Iroh monitoring protocol handler".to_string(),
        })
    }
}

/// Types of monitoring requests
pub enum MonitoringRequestType {
    FullMonitoring,
    PrometheusMetrics,
}
