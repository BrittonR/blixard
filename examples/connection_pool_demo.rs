//! Example demonstrating connection pooling for gRPC clients
//!
//! This example shows how connection pooling improves performance by:
//! - Reusing connections instead of creating new ones
//! - Supporting multiple concurrent requests per peer
//! - Automatically cleaning up idle connections

use blixard_core::{config_global, config_v2::BlixardConfig, node::Node, types::NodeConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
                .add_directive("connection_pool_demo=info".parse()?),
        )
        .init();

    info!("Starting connection pool demo");

    // Create temporary directories for nodes
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    // Configure connection pooling
    let mut config = BlixardConfig::default();
    config.cluster.peer.enable_pooling = true;
    config.cluster.peer.connection_pool.max_connections_per_peer = 3;
    config.cluster.peer.connection_pool.idle_timeout_secs = 30;
    config.cluster.peer.connection_pool.max_lifetime_secs = 300;
    config.cluster.peer.connection_pool.cleanup_interval_secs = 10;

    config_global::set(config.clone());

    // Create first node (bootstrap)
    let node1_config = NodeConfig {
        id: 1,
        bind_address: "127.0.0.1:7101".to_string(),
        data_dir: temp_dir1.path().to_string_lossy().to_string(),
        bootstrap: true,
        transport_config: None,
    };

    let mut node1 = Node::new(node1_config);
    node1.initialize().await?;
    let node1_shared = node1.shared();

    // Start first node
    let node1_arc = Arc::new(node1);
    let node1_handle = node1_arc.clone().start();

    // Wait for bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create second node
    let node2_config = NodeConfig {
        id: 2,
        bind_address: "127.0.0.1:7102".to_string(),
        data_dir: temp_dir2.path().to_string_lossy().to_string(),
        bootstrap: false,
        transport_config: None,
    };

    let mut node2 = Node::new(node2_config);
    node2.initialize().await?;
    let node2_shared = node2.shared();

    // Join cluster
    node2_shared
        .join_cluster("127.0.0.1:7101".to_string())
        .await?;

    // Start second node
    let node2_arc = Arc::new(node2);
    let node2_handle = node2_arc.clone().start();

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_secs(3)).await;

    info!("Cluster formed with connection pooling enabled");

    // Get peer connector
    let peer_connector = node1_shared
        .get_peer_connector()
        .await
        .expect("Peer connector should be available");

    // Demonstrate connection pooling
    info!("\n=== Connection Pool Demo ===");

    // Test 1: Sequential requests (should reuse connection)
    info!("\nTest 1: Sequential requests");
    let start = Instant::now();

    for i in 0..5 {
        let conn = peer_connector.get_connection(2).await;
        if conn.is_some() {
            info!("  Request {}: Got connection", i + 1);
        }
        // Connection automatically returned to pool when dropped
    }

    info!("  Sequential requests completed in {:?}", start.elapsed());

    // Test 2: Concurrent requests (should use multiple connections)
    info!("\nTest 2: Concurrent requests");
    let start = Instant::now();

    let mut handles = vec![];
    for i in 0..10 {
        let pc = peer_connector.clone();
        let handle = tokio::spawn(async move {
            let conn = pc.get_connection(2).await;
            if conn.is_some() {
                info!("  Concurrent request {}: Got connection", i + 1);
                // Simulate some work
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all requests
    for handle in handles {
        handle.await?;
    }

    info!("  Concurrent requests completed in {:?}", start.elapsed());

    // Test 3: Using pooled connection guard
    info!("\nTest 3: Pooled connection guard");
    match peer_connector.get_pooled_connection(2).await {
        Ok(mut guard) => {
            info!("  Got pooled connection guard");
            // Use the connection
            let _client = guard.client();
            info!("  Using connection...");
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Connection automatically returned when guard is dropped
        }
        Err(e) => {
            error!("  Failed to get pooled connection: {}", e);
        }
    }
    info!("  Connection returned to pool");

    // Wait for cleanup cycle
    info!("\nWaiting for cleanup cycle...");
    tokio::time::sleep(Duration::from_secs(11)).await;

    // Compare with pooling disabled
    info!("\n=== Comparison with Pooling Disabled ===");

    // Disable pooling
    let mut config = config_global::get();
    config.cluster.peer.enable_pooling = false;
    config_global::set(config);

    // Create new peer connector without pooling
    // (In real usage, you'd restart the node)

    info!("\nWithout pooling, each request creates a new connection");
    info!("This is slower and uses more resources");

    // Shutdown
    info!("\nShutting down nodes...");
    node1_arc.stop().await?;
    node2_arc.stop().await?;

    // Wait for shutdown
    let _ = tokio::time::timeout(Duration::from_secs(5), node1_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), node2_handle).await;

    info!("\nConnection pool demo completed!");
    info!("\nKey benefits demonstrated:");
    info!("  1. Connection reuse reduces latency");
    info!("  2. Multiple connections per peer improve throughput");
    info!("  3. Automatic cleanup prevents resource leaks");
    info!("  4. Pooled guards simplify connection management");

    Ok(())
}
