//! Demonstrates dual transport mode with both gRPC and Iroh
//!
//! This example shows how to run services over both transports simultaneously
//! and how clients can connect using either protocol.

use blixard_core::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    types::NodeConfig,
    proto::{
        cluster_service_client::ClusterServiceClient,
        ClusterStatusRequest, HealthCheckRequest,
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
use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting dual transport demo");
    
    // Create node state
    let config = NodeConfig {
        id: 42,
        data_dir: "/tmp/dual-transport-demo".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
    };
    let node = Arc::new(SharedNodeState::new(config));
    
    // Create endpoints
    let grpc_addr: SocketAddr = "127.0.0.1:7042".parse().unwrap();
    let iroh_endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    
    let watch = iroh_endpoint.node_addr();
    let iroh_node_addr = watch.get().unwrap().unwrap();
    info!("Iroh endpoint: {}", iroh_node_addr.node_id);
    
    // Configure dual mode with gradual migration
    let mut strategy = MigrationStrategy::default();
    // Migrate health service to Iroh, keep status on gRPC
    strategy.prefer_iroh_for.insert(ServiceType::Health);
    
    let config = TransportConfig::Dual {
        strategy,
        grpc_config: Default::default(),
        iroh_config: Default::default(),
    };
    
    // Create dual service runner
    let runner = DualServiceRunner::new(
        node.clone(),
        config,
        grpc_addr,
        Some(iroh_endpoint.clone()),
    );
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = runner.serve_non_critical_services().await {
            error!("Server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    info!("Server started in dual mode");
    info!("- gRPC listening on: {}", grpc_addr);
    info!("- Iroh listening on: {}", iroh_node_addr);
    
    // Demonstrate gRPC client
    info!("\n=== Testing gRPC Transport ===");
    
    let channel = Channel::from_shared(format!("http://{}", grpc_addr))?
        .connect()
        .await?;
    
    let mut grpc_client = ClusterServiceClient::new(channel);
    
    // Try health check via gRPC (should fail - migrated to Iroh)
    info!("Testing health check via gRPC (should fail)...");
    match grpc_client.health_check(HealthCheckRequest {}).await {
        Ok(_) => info!("Health check succeeded (unexpected)"),
        Err(e) => info!("Health check failed as expected: {}", e),
    }
    
    // Try cluster status via gRPC (should work)
    info!("Testing cluster status via gRPC...");
    let response = grpc_client
        .get_cluster_status(ClusterStatusRequest {})
        .await?;
    let status = response.into_inner();
    info!("Cluster status via gRPC: {} nodes, term {}", 
          status.nodes.len(), status.term);
    
    // Demonstrate Iroh client
    info!("\n=== Testing Iroh Transport ===");
    
    let client_endpoint = iroh::Endpoint::builder()
        .bind()
        .await?;
    let iroh_client = IrohRpcClient::new(client_endpoint);
    
    // Test health service via Iroh (should work)
    info!("Testing health check via Iroh...");
    let health_client = IrohHealthClient::new(&iroh_client, iroh_node_addr.clone());
    let health_response = health_client.check().await?;
    info!("Health check via Iroh: status={}, node_id={}, uptime={}s", 
          health_response.status, health_response.node_id, health_response.uptime);
    
    // Test ping
    let pong = health_client.ping().await?;
    info!("Ping via Iroh: {}", pong);
    
    // Try status service via Iroh (should fail - not migrated)
    info!("Testing cluster status via Iroh (should fail)...");
    let status_client = IrohStatusClient::new(&iroh_client, iroh_node_addr);
    match status_client.get_cluster_status().await {
        Ok(_) => info!("Cluster status succeeded (unexpected)"),
        Err(e) => info!("Cluster status failed as expected: {}", e),
    }
    
    info!("\n=== Migration Summary ===");
    info!("Health service: ✓ Migrated to Iroh");
    info!("Status service: ✓ Still on gRPC");
    info!("Dual transport mode working correctly!");
    
    // Cleanup
    server_handle.abort();
    
    Ok(())
}