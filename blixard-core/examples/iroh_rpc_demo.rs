//! Minimal demo of custom Iroh RPC implementation

use blixard_core::{
    node_shared::SharedNodeState,
    transport::{
        iroh_health_service::{IrohHealthClient, IrohHealthService},
        iroh_service::{IrohRpcClient, IrohRpcServer},
    },
};
use iroh::{Endpoint, NodeAddr};
use std::sync::Arc;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("Starting Iroh RPC demo...");

    // Create server endpoint
    let server_endpoint = Endpoint::builder().bind().await?;

    let server_node_id = server_endpoint.node_id();
    let server_addr = NodeAddr::new(server_node_id);
    println!("Server node ID: {}", server_node_id);

    // Create RPC server
    let server = Arc::new(IrohRpcServer::new(server_endpoint));

    // Create and register health service
    let node_state = Arc::new(SharedNodeState::new(1));
    let health_service = IrohHealthService::new(node_state);
    server.register_service(health_service).await;

    // Start server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            eprintln!("Server error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Create client endpoint
    let client_endpoint = Endpoint::builder().bind().await?;

    println!("Client endpoint created");

    // Create RPC client
    let client = IrohRpcClient::new(client_endpoint);

    // Test health check
    println!("\nTesting health check...");
    let health_client = IrohHealthClient::new(&client, server_addr.clone());

    match health_client.check().await {
        Ok(response) => {
            println!("Health check response:");
            println!("  Status: {}", response.status);
            println!("  Node ID: {}", response.node_id);
            println!("  Uptime: {} seconds", response.uptime);
            println!("  Connected peers: {:?}", response.connected_peers);
        }
        Err(e) => {
            eprintln!("Health check failed: {}", e);
        }
    }

    // Test ping
    println!("\nTesting ping...");
    match health_client.ping().await {
        Ok(response) => {
            println!("Ping response: {}", response);
        }
        Err(e) => {
            eprintln!("Ping failed: {}", e);
        }
    }

    // Cleanup
    server_handle.abort();
    println!("\nDemo completed!");

    Ok(())
}
