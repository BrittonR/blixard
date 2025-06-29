//! Tests for Iroh RPC service implementation

use crate::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    types::NodeConfig,
    transport::{
        iroh_protocol::{serialize_payload, deserialize_payload},
        iroh_service::{IrohService, ServiceRegistry},
        iroh_health_service::{IrohHealthService, HealthCheckRequest, HealthCheckResponse},
        iroh_status_service::{IrohStatusService, ClusterStatusRequest, ClusterStatusResponse},
    },
};
use bytes::Bytes;
use std::sync::Arc;

#[tokio::test]
async fn test_health_service() -> BlixardResult<()> {
    let config = NodeConfig {
        id: 1,
        data_dir: "/tmp/test".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
    };
    let node = Arc::new(SharedNodeState::new(config));
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
    let response: HealthCheckResponse = deserialize_payload(&response_bytes)?;
    
    assert!(response.healthy);
    assert!(response.message.contains("Node 1"));
    
    Ok(())
}

#[tokio::test]
async fn test_status_service() -> BlixardResult<()> {
    let config = NodeConfig {
        id: 2,
        data_dir: "/tmp/test2".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
    };
    let node = Arc::new(SharedNodeState::new(config));
    let service = IrohStatusService::new(node);
    
    // Test service name
    assert_eq!(service.name(), "status");
    assert_eq!(service.methods(), vec!["get_cluster_status", "get_raft_status"]);
    
    // Test get_cluster_status method
    let request = ClusterStatusRequest {};
    let request_bytes = serialize_payload(&request)?;
    let response_bytes = service.handle_call("get_cluster_status", request_bytes).await?;
    let response: ClusterStatusResponse = deserialize_payload(&response_bytes)?;
    
    // Should have at least the current node
    assert!(!response.member_ids.is_empty());
    
    Ok(())
}

#[test]
fn test_service_registry() {
    let mut registry = ServiceRegistry::new();
    
    // Test empty registry
    assert!(registry.get("health").is_none());
    assert!(registry.list_services().is_empty());
    
    // Register a service
    let config = NodeConfig {
        id: 1,
        data_dir: "/tmp/test".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
    };
    let node = Arc::new(SharedNodeState::new(config));
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