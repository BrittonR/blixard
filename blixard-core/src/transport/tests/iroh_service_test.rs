//! Tests for Iroh RPC service implementation

use crate::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    proto::{HealthCheckRequest, ClusterStatusRequest},
    transport::{
        iroh_protocol::{serialize_payload, deserialize_payload},
        iroh_service::{IrohService, ServiceRegistry},
        iroh_health_service::IrohHealthService,
        iroh_status_service::IrohStatusService,
    },
};
use bytes::Bytes;
use std::sync::Arc;

#[tokio::test]
async fn test_health_service() -> BlixardResult<()> {
    let node = Arc::new(SharedNodeState::new(1));
    let service = IrohHealthService::new(node);
    
    // Test service name
    assert_eq!(service.name(), "health");
    assert_eq!(service.methods(), vec!["check", "ping"]);
    
    // Test ping method
    let response = service.handle_call("ping", Bytes::new()).await?;
    assert_eq!(&response[..], b"pong");
    
    // Test check method
    let request = HealthCheckRequest {};
    let request_bytes = serialize_payload(&request)?;
    let response_bytes = service.handle_call("check", request_bytes).await?;
    let response: crate::proto::HealthCheckResponse = deserialize_payload(&response_bytes)?;
    
    assert_eq!(response.status, "healthy");
    assert_eq!(response.node_id, 1);
    
    Ok(())
}

#[tokio::test]
async fn test_status_service() -> BlixardResult<()> {
    let node = Arc::new(SharedNodeState::new(2));
    let service = IrohStatusService::new(node);
    
    // Test service name
    assert_eq!(service.name(), "status");
    assert_eq!(service.methods(), vec!["get_cluster_status", "get_raft_status"]);
    
    // Test get_cluster_status method
    let request = ClusterStatusRequest {};
    let request_bytes = serialize_payload(&request)?;
    let response_bytes = service.handle_call("get_cluster_status", request_bytes).await?;
    let response: crate::proto::ClusterStatusResponse = deserialize_payload(&response_bytes)?;
    
    // Should have at least the current node
    assert!(!response.nodes.is_empty());
    
    Ok(())
}

#[test]
fn test_service_registry() {
    let mut registry = ServiceRegistry::new();
    
    // Test empty registry
    assert!(registry.get("health").is_none());
    assert!(registry.list_services().is_empty());
    
    // Register a service
    let node = Arc::new(SharedNodeState::new(1));
    let health_service = IrohHealthService::new(node.clone());
    registry.register(health_service);
    
    // Test registry after registration
    assert!(registry.get("health").is_some());
    assert_eq!(registry.list_services(), vec!["health"]);
    
    // Register another service
    let status_service = IrohStatusService::new(node);
    registry.register(status_service);
    
    // Test multiple services
    assert!(registry.get("status").is_some());
    let services = registry.list_services();
    assert_eq!(services.len(), 2);
    assert!(services.contains(&"health".to_string()));
    assert!(services.contains(&"status".to_string()));
}