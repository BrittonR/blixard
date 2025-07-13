//! Health check API schemas

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Overall system health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// All systems operational
    Healthy,
    /// Some non-critical issues detected
    Degraded,
    /// Critical issues affecting functionality
    Unhealthy,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Overall health status
    pub status: HealthStatus,
    
    /// Timestamp of health check (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
    
    /// Node ID
    pub node_id: u64,
    
    /// Service uptime in seconds
    pub uptime_seconds: u64,
    
    /// Version information
    pub version: VersionInfo,
    
    /// Individual service health checks
    pub services: Vec<ServiceHealth>,
    
    /// Resource health information
    pub resources: ResourceHealth,
    
    /// Cluster connectivity status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterHealth>,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VersionInfo {
    /// Application version
    pub version: String,
    
    /// Git commit hash
    pub commit: String,
    
    /// Build timestamp (ISO 8601)
    pub build_time: String,
    
    /// Rust version used for build
    pub rust_version: String,
}

/// Individual service health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceHealth {
    /// Service name
    pub name: String,
    
    /// Service health status
    pub status: HealthStatus,
    
    /// Last check timestamp (ISO 8601)
    pub last_check: chrono::DateTime<chrono::Utc>,
    
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    
    /// Service-specific metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
}

/// Resource health information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResourceHealth {
    /// CPU health
    pub cpu: ResourceHealthStatus,
    
    /// Memory health
    pub memory: ResourceHealthStatus,
    
    /// Disk health
    pub disk: ResourceHealthStatus,
    
    /// Network health
    pub network: ResourceHealthStatus,
}

/// Individual resource health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResourceHealthStatus {
    /// Resource health status
    pub status: HealthStatus,
    
    /// Current usage percentage (0.0-100.0)
    pub usage_percent: f64,
    
    /// Available amount (in appropriate units)
    pub available: u64,
    
    /// Total amount (in appropriate units)
    pub total: u64,
    
    /// Resource-specific details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Cluster connectivity health
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClusterHealth {
    /// Cluster connectivity status
    pub status: HealthStatus,
    
    /// Number of connected peers
    pub connected_peers: u32,
    
    /// Total expected peers
    pub expected_peers: u32,
    
    /// Whether this node is the cluster leader
    pub is_leader: bool,
    
    /// Current Raft term
    pub raft_term: u64,
    
    /// Raft log replication health
    pub replication_health: ReplicationHealth,
}

/// Raft replication health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplicationHealth {
    /// Replication status
    pub status: HealthStatus,
    
    /// Current log index
    pub log_index: u64,
    
    /// Number of up-to-date followers
    pub healthy_followers: u32,
    
    /// Number of lagging followers
    pub lagging_followers: u32,
    
    /// Maximum replication lag in log entries
    pub max_lag: u64,
}

/// Readiness check response (simpler than full health check)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadinessResponse {
    /// Whether service is ready to accept requests
    pub ready: bool,
    
    /// Timestamp of readiness check (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
    
    /// Optional message explaining readiness status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Liveness check response (simplest health check)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LivenessResponse {
    /// Whether service is alive
    pub alive: bool,
    
    /// Timestamp of liveness check (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl HealthResponse {
    /// Create a healthy response
    pub fn healthy(node_id: u64, uptime_seconds: u64) -> Self {
        Self {
            status: HealthStatus::Healthy,
            timestamp: chrono::Utc::now(),
            node_id,
            uptime_seconds,
            version: VersionInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                commit: option_env!("GIT_HASH").unwrap_or("unknown").to_string(),
                build_time: option_env!("BUILD_TIME").unwrap_or("unknown").to_string(),
                rust_version: option_env!("RUST_VERSION").unwrap_or("unknown").to_string(),
            },
            services: Vec::new(),
            resources: ResourceHealth {
                cpu: ResourceHealthStatus {
                    status: HealthStatus::Healthy,
                    usage_percent: 0.0,
                    available: 0,
                    total: 0,
                    details: None,
                },
                memory: ResourceHealthStatus {
                    status: HealthStatus::Healthy,
                    usage_percent: 0.0,
                    available: 0,
                    total: 0,
                    details: None,
                },
                disk: ResourceHealthStatus {
                    status: HealthStatus::Healthy,
                    usage_percent: 0.0,
                    available: 0,
                    total: 0,
                    details: None,
                },
                network: ResourceHealthStatus {
                    status: HealthStatus::Healthy,
                    usage_percent: 0.0,
                    available: 0,
                    total: 0,
                    details: None,
                },
            },
            cluster: None,
        }
    }
    
    /// Add service health status
    pub fn with_service(mut self, service: ServiceHealth) -> Self {
        self.services.push(service);
        self
    }
    
    /// Set cluster health status
    pub fn with_cluster(mut self, cluster: ClusterHealth) -> Self {
        self.cluster = Some(cluster);
        self
    }
    
    /// Calculate overall health status based on services and resources
    pub fn calculate_overall_status(&mut self) {
        let mut has_unhealthy = false;
        let mut has_degraded = false;
        
        // Check service health
        for service in &self.services {
            match service.status {
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Degraded => has_degraded = true,
                HealthStatus::Healthy => {}
            }
        }
        
        // Check resource health
        let resource_statuses = [
            &self.resources.cpu.status,
            &self.resources.memory.status,
            &self.resources.disk.status,
            &self.resources.network.status,
        ];
        
        for status in resource_statuses {
            match status {
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Degraded => has_degraded = true,
                HealthStatus::Healthy => {}
            }
        }
        
        // Check cluster health
        if let Some(ref cluster) = self.cluster {
            match cluster.status {
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Degraded => has_degraded = true,
                HealthStatus::Healthy => {}
            }
        }
        
        // Set overall status
        self.status = if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }
}