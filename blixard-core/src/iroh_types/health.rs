//! Health check request/response types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub healthy: bool,
    pub message: String,
    // Extended fields for enhanced health reporting
    pub status: Option<String>,
    pub timestamp: Option<u64>,
    pub node_id: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub vm_count: Option<u32>,
    pub memory_usage_mb: Option<u64>,
    pub active_connections: Option<u32>,
}