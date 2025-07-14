//! Minimal Iroh node test - bypasses all broken subsystems
//!
//! This example creates a minimal node with just Iroh transport to verify
//! the P2P communication layer works before fixing all compilation errors.

use blixard_core::{
    error::BlixardResult,
    iroh_types::{HealthCheckRequest, HealthCheckResponse},
    node_shared::SharedNodeState,
    transport::{
        iroh_health_service::IrohHealthService,
        iroh_service::{IrohRpcClient, IrohRpcServer},
    },
    types::NodeConfig,
};
use iroh::{Endpoint, NodeAddr, SecretKey};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,iroh=debug")
        .init();

    println!("=== Minimal Iroh Node Test ===\n");

    // Create minimal node configuration
    let node_id = 1;
    let bind_addr: SocketAddr = "127.0.0.1:7001".parse().unwrap();

    let config = NodeConfig {
        id: node_id,
        bind_addr,
        data_dir: format!("/tmp/minimal-iroh-node-{}", node_id),
        vm_backend: "mock".to_string(),
        join_addr: None,
        use_tailscale: false,
        transport_config: Some(Default::default()),
        topology: Default::default(),
    };

    // Create shared state (minimal initialization)
    let shared_state = Arc::new(SharedNodeState::new(config.clone()));

    // Create Iroh endpoint directly
    let secret = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create Iroh endpoint: {}", e),
        })?;

    println!("✓ Created Iroh endpoint");
    println!("  Node ID: {}", endpoint.node_id());
    println!("  Bound to: {:?}", endpoint.bound_sockets());

    // Create a simple RPC server
    let server = Arc::new(IrohRpcServer::new(endpoint.clone()));

    // Register health service
    let health_service = IrohHealthService::new(shared_state.clone());
    server.register_service(health_service).await;

    println!("✓ Registered health service");

    // Start the server in background
    let server_handle = {
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server.serve().await {
                tracing::error!("Server error: {}", e);
            }
        })
    };

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    println!("\n📡 Testing local health check...");

    // Create a client and test health check
    let client = IrohRpcClient::new(endpoint.clone());

    // Get our own address
    let our_addr =
        NodeAddr::new(endpoint.node_id()).with_direct_addresses(endpoint.bound_sockets());

    // Test health check
    let request = HealthCheckRequest {};
    match client
        .call::<HealthCheckRequest, HealthCheckResponse>(
            our_addr.clone(),
            "health",
            "check_health",
            request,
        )
        .await
    {
        Ok(response) => {
            println!("✓ Health check successful!");
            if let Some(status) = &response.status {
                println!("  Status: {}", status);
            }
            println!("  Message: {}", response.message);
            if let Some(node_id) = &response.node_id {
                println!("  Node ID: {}", node_id);
            }
        }
        Err(e) => {
            println!("✗ Health check failed: {}", e);
        }
    }

    // Keep running for a bit to allow testing with other nodes
    println!("\n🚀 Node is running. Press Ctrl+C to stop.");
    println!("To test from another node, use node address:");
    println!("  {}", endpoint.node_id());

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await.unwrap();

    println!("\n🛑 Shutting down...");

    // Cleanup
    server_handle.abort();
    endpoint.close().await;

    println!("✅ Shutdown complete");

    Ok(())
}
