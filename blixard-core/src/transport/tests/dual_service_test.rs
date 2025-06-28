//! Tests for dual service runner with both gRPC and Iroh

use crate::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    proto::{
        cluster_service_client::ClusterServiceClient,
        blixard_service_client::BlixardServiceClient,
        ClusterStatusRequest, HealthCheckRequest,
        GetRaftStatusRequest,
    },
    transport::{
        config::{TransportConfig, MigrationStrategy, ServiceType},
        dual_service_runner::DualServiceRunner,
        iroh_service::IrohRpcClient,
        iroh_health_service::IrohHealthClient,
        iroh_status_service::IrohStatusClient,
    },
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::time::sleep;
use tonic::transport::Channel;

#[tokio::test]
async fn test_grpc_only_mode() -> BlixardResult<()> {
    // Create node state
    let node = Arc::new(SharedNodeState::new(1));
    
    // Configure gRPC-only mode
    let config = TransportConfig::Grpc(Default::default());
    let grpc_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    // Create and start dual service runner in gRPC-only mode
    let runner = DualServiceRunner::new(node, config, grpc_addr, None);
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        runner.serve_non_critical_services().await
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Test gRPC client
    let channel = Channel::from_static("http://127.0.0.1:7001")
        .connect()
        .await?;
    
    let mut cluster_client = ClusterServiceClient::new(channel.clone());
    let mut blixard_client = BlixardServiceClient::new(channel);
    
    // Test health check
    let response = cluster_client
        .health_check(HealthCheckRequest {})
        .await?;
    assert_eq!(response.into_inner().status, "healthy");
    
    // Test cluster status
    let response = cluster_client
        .get_cluster_status(ClusterStatusRequest {})
        .await?;
    assert!(!response.into_inner().nodes.is_empty());
    
    // Test Raft status
    let response = blixard_client
        .get_raft_status(GetRaftStatusRequest {})
        .await?;
    assert_eq!(response.into_inner().node_id, 1);
    
    // Cleanup
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_iroh_only_mode() -> BlixardResult<()> {
    // Create node state
    let node = Arc::new(SharedNodeState::new(2));
    
    // Create Iroh endpoint
    let endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    
    // Configure Iroh-only mode
    let config = TransportConfig::Iroh(Default::default());
    let grpc_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    // Create and start dual service runner in Iroh-only mode
    let runner = DualServiceRunner::new(
        node.clone(),
        config,
        grpc_addr,
        Some(endpoint.clone()),
    );
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        runner.serve_non_critical_services().await
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Create Iroh client
    let client_endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    let iroh_client = IrohRpcClient::new(client_endpoint);
    
    // Get node address for connection
    let node_addr = endpoint.node_addr().await?;
    
    // Test health service
    let health_client = IrohHealthClient::new(&iroh_client, node_addr.clone());
    let response = health_client.check().await?;
    assert_eq!(response.status, "healthy");
    assert_eq!(response.node_id, 2);
    
    // Test ping
    let pong = health_client.ping().await?;
    assert_eq!(pong, "pong");
    
    // Test status service
    let status_client = IrohStatusClient::new(&iroh_client, node_addr);
    let cluster_status = status_client.get_cluster_status().await?;
    assert!(!cluster_status.nodes.is_empty());
    
    let raft_status = status_client.get_raft_status().await?;
    assert_eq!(raft_status.node_id, 2);
    
    // Cleanup
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_dual_mode_migration() -> BlixardResult<()> {
    // Create node state
    let node = Arc::new(SharedNodeState::new(3));
    
    // Create endpoints
    let grpc_addr: SocketAddr = "127.0.0.1:7003".parse().unwrap();
    let iroh_endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    
    // Configure dual mode with health on Iroh, status on gRPC
    let mut strategy = MigrationStrategy::default();
    strategy.prefer_iroh_for.insert(ServiceType::Health);
    
    let config = TransportConfig::Dual {
        strategy,
        grpc_config: Default::default(),
        iroh_config: Default::default(),
    };
    
    // Create and start dual service runner
    let runner = DualServiceRunner::new(
        node.clone(),
        config,
        grpc_addr,
        Some(iroh_endpoint.clone()),
    );
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        runner.serve_non_critical_services().await
    });
    
    // Give server time to start
    sleep(Duration::from_millis(200)).await;
    
    // Test gRPC client - health should fail (migrated to Iroh)
    let channel = Channel::from_static("http://127.0.0.1:7003")
        .connect()
        .await?;
    
    let mut cluster_client = ClusterServiceClient::new(channel.clone());
    let health_result = cluster_client
        .health_check(HealthCheckRequest {})
        .await;
    assert!(health_result.is_err());
    
    // Test gRPC client - status should work
    let response = cluster_client
        .get_cluster_status(ClusterStatusRequest {})
        .await?;
    assert!(!response.into_inner().nodes.is_empty());
    
    // Test Iroh client - health should work
    let client_endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    let iroh_client = IrohRpcClient::new(client_endpoint);
    let node_addr = iroh_endpoint.node_addr().await?;
    
    let health_client = IrohHealthClient::new(&iroh_client, node_addr.clone());
    let response = health_client.check().await?;
    assert_eq!(response.status, "healthy");
    
    // Test Iroh client - status should fail (not migrated)
    let status_client = IrohStatusClient::new(&iroh_client, node_addr);
    let status_result = status_client.get_cluster_status().await;
    assert!(status_result.is_err());
    
    // Cleanup
    server_handle.abort();
    
    Ok(())
}