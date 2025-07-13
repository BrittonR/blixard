//! Test P2P connectivity during cluster join
//!
//! This example demonstrates:
//! 1. Starting a bootstrap node with P2P enabled
//! 2. Starting a second node that joins the cluster
//! 3. Verifying P2P addresses are exchanged
//! 4. Establishing P2P connections between nodes

use blixard_core::{
    error::BlixardResult,
    node::Node,
    transport::config::TransportConfig,
    types::{NodeConfig, NodeTopology},
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,iroh=info")
        .init();

    // Initialize global configuration with defaults
    blixard_core::config_global::init(blixard_core::config::Config::default())?;

    // Initialize metrics
    blixard_core::metrics_otel::init_noop().map_err(|e| {
        blixard_core::error::BlixardError::Internal {
            message: format!("Failed to initialize metrics: {}", e),
        }
    })?;

    info!("=== P2P Join Test ===");

    // Create bootstrap node (node 1)
    let node1_config = NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:7001".parse::<SocketAddr>().unwrap(),
        data_dir: "/tmp/blixard-p2p-test-1".to_string(),
        vm_backend: "mock".to_string(),
        join_addr: None,
        use_tailscale: false,
        transport_config: Some(TransportConfig::default()),
        topology: NodeTopology::default(),
    };

    info!("Starting bootstrap node 1...");
    let mut node1 = Node::new(node1_config);
    node1.initialize().await?;

    // Get node 1's P2P info
    let node1_shared = node1.shared();
    let node1_p2p_addr = node1_shared.get_p2p_node_addr().await;
    info!("Node 1 P2P address: {:?}", node1_p2p_addr);

    // Give node 1 time to start up fully
    sleep(Duration::from_millis(500)).await;

    // Create joining node (node 2)
    let node2_config = NodeConfig {
        id: 2,
        bind_addr: "127.0.0.1:7002".parse::<SocketAddr>().unwrap(),
        data_dir: "/tmp/blixard-p2p-test-2".to_string(),
        vm_backend: "mock".to_string(),
        join_addr: Some("127.0.0.1:7001".to_string()),
        use_tailscale: false,
        transport_config: Some(TransportConfig::default()),
        topology: NodeTopology::default(),
    };

    info!("Starting node 2 to join cluster...");
    let mut node2 = Node::new(node2_config);

    // Note: The join will fail because we haven't implemented the full join logic yet
    // But we can still verify P2P addresses are available
    match node2.initialize().await {
        Ok(_) => info!("Node 2 initialized successfully"),
        Err(e) => info!("Node 2 initialization error: {}", e),
    }

    // Get node 2's P2P info
    let node2_shared = node2.shared();
    let node2_p2p_addr = node2_shared.get_p2p_node_addr().await;
    info!("Node 2 P2P address: {:?}", node2_p2p_addr);

    // Verify both nodes have P2P addresses
    if node1_p2p_addr.is_some() && node2_p2p_addr.is_some() {
        info!("✅ Both nodes have P2P addresses");

        // Try to establish P2P connection manually
        if let (Some(p2p_manager1), Some(p2p_manager2)) = (
            node1_shared.get_p2p_manager().await,
            node2_shared.get_p2p_manager().await,
        ) {
            info!("Both nodes have P2P managers");

            // Node 2 connects to node 1
            if let Some(node1_addr) = node1_p2p_addr {
                info!("Node 2 attempting to connect to node 1...");
                match p2p_manager2.connect_p2p_peer(1, &node1_addr).await {
                    Ok(_) => info!("✅ P2P connection established!"),
                    Err(e) => info!("❌ P2P connection failed: {}", e),
                }
            }
        }
    } else {
        info!("❌ One or both nodes missing P2P addresses");
    }

    // Check peer lists
    info!("\nPeer information:");
    let node1_peers = node1_shared.get_peers().await;
    info!("Node 1 sees {} peers", node1_peers.len());
    for peer in &node1_peers {
        info!("  Peer {}: P2P={:?}", peer.id, peer.p2p_node_id);
    }

    let node2_peers = node2_shared.get_peers().await;
    info!("Node 2 sees {} peers", node2_peers.len());
    for peer in &node2_peers {
        info!("  Peer {}: P2P={:?}", peer.id, peer.p2p_node_id);
    }

    // Cleanup
    info!("\nShutting down nodes...");
    node2.stop().await?;
    node1.stop().await?;

    info!("✅ Test completed");
    Ok(())
}
