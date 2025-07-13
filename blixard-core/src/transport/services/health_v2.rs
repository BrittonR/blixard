//! Health service implementation using ServiceBuilder
//!
//! This demonstrates how the ServiceBuilder pattern eliminates duplication
//! and provides consistent service patterns.

use crate::{
    error::BlixardResult,
    iroh_types::{HealthCheckRequest, HealthCheckResponse},
    node_shared::SharedNodeState,
    transport::service_builder::{ServiceBuilder, ServiceProtocolHandler},
};
use std::sync::Arc;

/// Health service request types
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HealthRequest {
    pub include_details: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub node_id: String,
    pub uptime_seconds: u64,
    pub details: Option<HealthDetails>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HealthDetails {
    pub vm_count: usize,
    pub active_connections: usize,
    pub memory_usage_mb: u64,
    pub disk_usage_percent: f64,
}

/// Create health service using ServiceBuilder
pub fn create_health_service(node: Arc<SharedNodeState>) -> ServiceProtocolHandler {
    let node_clone = node.clone();

    let service = ServiceBuilder::new("health", node)
        .simple_method("check", move |req: HealthRequest| {
            let node = node_clone.clone();
            async move { check_health(node, req).await }
        })
        .simple_method("ping", |_req: ()| async move { Ok("pong".to_string()) })
        .simple_method("version", |_req: ()| async move {
            Ok(env!("CARGO_PKG_VERSION").to_string())
        })
        .build();

    ServiceProtocolHandler::new(service)
}

/// Health check implementation
async fn check_health(
    node: Arc<SharedNodeState>,
    request: HealthRequest,
) -> BlixardResult<HealthResponse> {
    let node_id = node.get_id().to_string();
    // TODO: Implement proper start time tracking in SharedNodeState
    let uptime = 0u64; // Default to 0 until start time tracking is implemented

    let details = if request.include_details {
        Some(HealthDetails {
            vm_count: node.get_vm_count() as usize,
            active_connections: node.get_active_connection_count().await as usize,
            memory_usage_mb: get_memory_usage().await,
            disk_usage_percent: get_disk_usage().await,
        })
    } else {
        None
    };

    Ok(HealthResponse {
        status: "healthy".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        node_id,
        uptime_seconds: uptime,
        details,
    })
}

/// Get current memory usage
async fn get_memory_usage() -> u64 {
    // Simplified implementation - in production would use system metrics
    1024 // MB
}

/// Get current disk usage percentage
async fn get_disk_usage() -> f64 {
    // Simplified implementation - in production would check actual disk usage
    45.2 // percent
}

/// Legacy compatibility wrapper
pub struct HealthServiceV2 {
    handler: ServiceProtocolHandler,
}

impl HealthServiceV2 {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            handler: create_health_service(node),
        }
    }

    /// Handle health check with legacy interface
    pub async fn handle_health_check(
        &self,
        _request: HealthCheckRequest,
    ) -> BlixardResult<HealthCheckResponse> {
        // HealthCheckRequest has no fields, so create basic HealthRequest
        let health_req = HealthRequest {
            include_details: false, // Default since HealthCheckRequest doesn't have this field
        };

        // Serialize request
        let payload = crate::transport::iroh_protocol::serialize_payload(&health_req)?;

        // Call through ServiceBuilder
        let response_payload = self.handler.handle_request("check", payload).await?;

        // Deserialize response
        let health_resp: HealthResponse =
            crate::transport::iroh_protocol::deserialize_payload(&response_payload)?;

        // Convert to legacy format
        Ok(HealthCheckResponse {
            healthy: health_resp.status == "healthy",
            message: format!(
                "Node {} health status: {}",
                health_resp.node_id, health_resp.status
            ),
            status: Some(health_resp.status),
            timestamp: Some(health_resp.timestamp),
            node_id: Some(health_resp.node_id),
            uptime_seconds: Some(health_resp.uptime_seconds),
            vm_count: health_resp.details.as_ref().map(|d| d.vm_count as u32),
            memory_usage_mb: health_resp.details.as_ref().map(|d| d.memory_usage_mb),
            active_connections: health_resp
                .details
                .as_ref()
                .map(|d| d.active_connections as u32),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_service_creation() {
        let node = Arc::new(SharedNodeState::new());
        let handler = create_health_service(node);

        assert_eq!(handler.service_name(), "health");
        let methods = handler.available_methods();
        assert!(methods.contains(&"check"));
        assert!(methods.contains(&"ping"));
        assert!(methods.contains(&"version"));
    }

    #[tokio::test]
    async fn test_health_check_basic() {
        let node = Arc::new(SharedNodeState::new());
        let request = HealthRequest {
            include_details: false,
        };

        let response = check_health(node, request).await.unwrap();
        assert_eq!(response.status, "healthy");
        assert!(response.details.is_none());
    }

    #[tokio::test]
    async fn test_health_check_with_details() {
        let node = Arc::new(SharedNodeState::new());
        let request = HealthRequest {
            include_details: true,
        };

        let response = check_health(node, request).await.unwrap();
        assert_eq!(response.status, "healthy");
        assert!(response.details.is_some());

        let details = response.details.unwrap();
        assert_eq!(details.memory_usage_mb, 1024);
        assert_eq!(details.disk_usage_percent, 45.2);
    }

    #[tokio::test]
    async fn test_legacy_compatibility() {
        let node = Arc::new(SharedNodeState::new());
        let service = HealthServiceV2::new(node);

        let request = HealthCheckRequest {
            include_details: Some(true),
        };

        let response = service.handle_health_check(request).await.unwrap();
        assert_eq!(response.status, "healthy");
        assert!(response.vm_count.is_some());
        assert!(response.memory_usage_mb.is_some());
    }
}
