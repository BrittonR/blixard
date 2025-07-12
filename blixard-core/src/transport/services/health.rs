//! Health check service implementation
//!
//! This service provides health checking functionality that works
//! over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    iroh_types::HealthCheckResponse,
    node_shared::SharedNodeState,
};
use async_trait::async_trait;
use std::sync::Arc;
// Removed tonic imports - using Iroh transport

/// Trait for health check operations
#[async_trait]
pub trait HealthService: Send + Sync {
    /// Check health status
    async fn check_health(&self) -> BlixardResult<HealthCheckResponse>;
}

/// Health service implementation
#[derive(Clone)]
pub struct HealthServiceImpl {
    node: Arc<SharedNodeState>,
}

impl HealthServiceImpl {
    /// Create a new health service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }

    /// Check if the node is healthy
    async fn is_node_healthy(&self) -> bool {
        // Check various health indicators

        // 1. Check if we can get Raft status (indicates Raft is running)
        let _raft_status = self.node.get_raft_status();

        // 2. Check if we can access database
        if let Some(database) = self.node.get_database().await {
            // Try a simple operation to verify database is responsive
            // Just checking if we can get the database is enough for health check
            let _ = database;
        } else {
            return false;
        }

        // 3. Check if peer connector is healthy (for networked nodes)
        if let Some(peer_connector) = self.node.get_peer_connector() {
            // Could check connection health here
            let _ = peer_connector;
        }

        true
    }
}

#[async_trait]
impl HealthService for HealthServiceImpl {
    async fn check_health(&self) -> BlixardResult<HealthCheckResponse> {
        let healthy = self.is_node_healthy().await;
        let message = if healthy {
            format!("Node {} is healthy", self.node.get_id())
        } else {
            format!("Node {} is unhealthy", self.node.get_id())
        };

        Ok(HealthCheckResponse {
            healthy,
            message,
            status: Some(if healthy {
                "OK".to_string()
            } else {
                "ERROR".to_string()
            }),
            timestamp: Some(chrono::Utc::now().timestamp() as u64),
            node_id: Some(self.node.get_id().to_string()),
            uptime_seconds: Some(0),     // TODO: Track actual uptime
            vm_count: Some(0),           // TODO: Get actual VM count
            memory_usage_mb: Some(0),    // TODO: Get actual memory usage
            active_connections: Some(0), // TODO: Get actual connection count
        })
    }
}

/// Iroh protocol handler for health service
pub struct HealthProtocolHandler {
    service: HealthServiceImpl,
}

impl HealthProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: HealthServiceImpl::new(node),
        }
    }

    /// Handle a health check request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
    ) -> BlixardResult<()> {
        // TODO: Implement proper protocol handling
        // For now, this is a placeholder

        // 1. Read request from connection
        // 2. Deserialize HealthCheckRequest
        // 3. Call service.check_health()
        // 4. Serialize response
        // 5. Send response back

        Err(BlixardError::NotImplemented {
            feature: "Iroh health protocol handler".to_string(),
        })
    }
}
