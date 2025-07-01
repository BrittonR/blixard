//! Simple Iroh cluster test - validates basic multi-node communication

use blixard_core::error::BlixardResult;
use blixard_core::transport::iroh_service::{IrohRpcServer, IrohRpcClient};
use blixard_core::transport::iroh_health_service::IrohHealthService;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::NodeConfig;
use blixard_core::transport::config::{TransportConfig, IrohConfig};
use blixard_core::iroh_types::{HealthCheckRequest, HealthCheckResponse};
use iroh::{Endpoint, SecretKey, NodeAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Simple Iroh Cluster Test ===\n");
    
    // Create two nodes
    let mut node1 = create_node(1).await?;
    let mut node2 = create_node(2).await?;
    
    println!("✓ Created two nodes with Iroh endpoints");
    
    // Start servers
    let server1_handle = tokio::spawn(node1.server.clone().serve());
    let server2_handle = tokio::spawn(node2.server.clone().serve());
    
    // Give servers time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test direct connection from node2 to node1
    println!("\n📡 Testing direct connection...");
    
    // Create client from node2 to connect to node1
    let client = IrohRpcClient::new(node2.endpoint.clone());
    
    // Get node1's address with socket addresses
    let addrs = node1.endpoint.bound_sockets();
    let node1_addr = NodeAddr::new(node1.endpoint.node_id())
        .with_direct_addresses(addrs);
    
    println!("Node 1 ID: {}", node1.endpoint.node_id());
    println!("Node 1 addr: {:?}", node1_addr.direct_addresses());
    
    // Test health check via RPC
    let request = HealthCheckRequest {};
    match client.call::<HealthCheckRequest, HealthCheckResponse>(
        node1_addr.clone(),
        "health",
        "check_health",
        request,
    ).await {
        Ok(response) => {
            println!("✓ Health check successful!");
            println!("  Status: {}", response.status);
            println!("  Message: {}", response.message);
        }
        Err(e) => {
            println!("✗ Health check failed: {}", e);
        }
    }
    
    // Cleanup - stop server tasks
    server1_handle.abort();
    server2_handle.abort();
    
    // Close endpoints
    node1.endpoint.close().await;
    node2.endpoint.close().await;
    
    println!("\n✅ Test complete!");
    
    Ok(())
}

struct TestNode {
    _config: NodeConfig,
    endpoint: Endpoint,
    server: Arc<IrohRpcServer>,
    _shared_state: Arc<SharedNodeState>,
}

async fn create_node(id: u64) -> BlixardResult<TestNode> {
    // Create node config
    let config = NodeConfig {
        id,
        bind_addr: format!("127.0.0.1:{}", 7000 + id).parse().unwrap(),
        data_dir: format!("/tmp/iroh-test-node-{}", id),
        vm_backend: "test".to_string(),
        join_addr: None,
        use_tailscale: false,
        transport_config: Some(TransportConfig::Iroh(IrohConfig::default())),
    };
    
    // Create Iroh endpoint
    let secret = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint: {}", e)
        })?;
    
    println!("Node {} endpoint: {}", id, endpoint.node_id());
    
    // Create shared state
    let shared_state = Arc::new(SharedNodeState::new(config.clone()));
    
    // Create server
    let server = Arc::new(IrohRpcServer::new(endpoint.clone()));
    
    // Register health service
    let health_service = IrohHealthService::new(shared_state.clone());
    server.register_service(health_service).await;
    
    Ok(TestNode {
        _config: config,
        endpoint,
        server,
        _shared_state: shared_state,
    })
}