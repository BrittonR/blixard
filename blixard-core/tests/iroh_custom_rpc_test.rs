//! Integration tests for custom Iroh RPC implementation
#![cfg(all(test, feature = "test-helpers"))]

use blixard_core::{
    node_shared::SharedNodeState,
    proto::{HealthCheckRequest, HealthCheckResponse},
    transport::{
        iroh_health_service::{IrohHealthService, IrohHealthClient},
        iroh_service::{IrohRpcServer, IrohRpcClient},
    },
};
use std::sync::Arc;
use iroh::{Endpoint, NodeAddr};

#[tokio::test]
async fn test_basic_iroh_rpc() {
    // Create server endpoint
    let server_endpoint = Endpoint::builder()
        .bind()
        .await
        .expect("Failed to create server endpoint");
    
    let server_node_id = server_endpoint.node_id();
    let server_addr = NodeAddr::new(server_node_id);
    
    // Create RPC server
    let server = Arc::new(IrohRpcServer::new(server_endpoint));
    
    // Create and register health service
    let node_state = Arc::new(SharedNodeState::new(1));
    let health_service = IrohHealthService::new(node_state);
    server.register_service(health_service).await;
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        server.serve().await
    });
    
    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client endpoint
    let client_endpoint = Endpoint::builder()
        .bind()
        .await
        .expect("Failed to create client endpoint");
    
    // Create RPC client
    let client = IrohRpcClient::new(client_endpoint);
    
    // Test health check
    let health_client = IrohHealthClient::new(&client, server_addr.clone());
    let response = health_client.check().await.expect("Health check failed");
    
    assert_eq!(response.status, "healthy");
    assert_eq!(response.node_id, 1);
    assert_eq!(response.uptime, 0); // Just started
    
    // Test ping
    let pong = health_client.ping().await.expect("Ping failed");
    assert_eq!(pong, "pong");
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_iroh_rpc_error_handling() {
    // Create endpoints
    let server_endpoint = Endpoint::builder()
        .bind()
        .await
        .expect("Failed to create server endpoint");
    
    let server_node_id = server_endpoint.node_id();
    let server_addr = NodeAddr::new(server_node_id);
    
    // Create RPC server (but don't register any services)
    let server = Arc::new(IrohRpcServer::new(server_endpoint));
    
    // Start server
    let server_handle = tokio::spawn(async move {
        server.serve().await
    });
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client
    let client_endpoint = Endpoint::builder()
        .bind()
        .await
        .expect("Failed to create client endpoint");
    
    let client = IrohRpcClient::new(client_endpoint);
    
    // Try to call non-existent service
    let result: Result<HealthCheckResponse, _> = client
        .call(server_addr, "nonexistent", "method", HealthCheckRequest {})
        .await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Service 'nonexistent'"));
    
    // Cleanup
    server_handle.abort();
}

#[test]
fn test_protocol_basics() {
    use blixard_core::transport::iroh_protocol::*;
    
    // Test request ID generation
    let id1 = generate_request_id();
    let id2 = generate_request_id();
    assert_ne!(id1, id2);
    
    // Test message header
    let header = MessageHeader::new(MessageType::Request, 100, id1);
    assert_eq!(header.version, 1);
    assert_eq!(header.msg_type, MessageType::Request);
    assert_eq!(header.payload_len, 100);
    
    // Test serialization
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct TestRequest {
        value: String,
    }
    
    let req = TestRequest {
        value: "test".to_string(),
    };
    
    let bytes = serialize_payload(&req).unwrap();
    let decoded: TestRequest = deserialize_payload(&bytes).unwrap();
    assert_eq!(decoded, req);
}