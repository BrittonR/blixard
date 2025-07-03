//! Simple test to debug join cluster functionality
#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers::TestNode;
use blixard_core::metrics_otel;
use std::time::Duration;

#[tokio::test]
async fn test_simple_node_join() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,iroh=debug")
        .try_init();
    
    // Initialize metrics
    metrics_otel::init_noop();
    
    println!("Starting simple join test");
    
    // Create bootstrap node
    let node1 = TestNode::start(1, 9001).await.expect("Failed to create node 1");
    println!("Created bootstrap node 1 at {}", node1.addr);
    
    // Wait for bootstrap node to be ready
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Check node 1 peers (should be empty)
    let peers1_before = node1.shared_state.get_peers().await;
    println!("Node 1 peers before join: {:?}", peers1_before);
    
    // Create node 2 that joins node 1
    let mut node2 = TestNode::start(2, 9002).await.expect("Failed to create node 2");
    println!("Created node 2 at {}", node2.addr);
    
    // Explicitly join the cluster
    println!("Node 2 joining cluster via {}", node1.addr);
    let join_result = node2.node.join_cluster(Some(node1.addr)).await;
    match &join_result {
        Ok(_) => println!("Join succeeded!"),
        Err(e) => println!("Join failed: {:?}", e),
    }
    assert!(join_result.is_ok(), "Join should succeed: {:?}", join_result.err());
    
    // Wait for propagation
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check peers
    let peers1 = node1.shared_state.get_peers().await;
    let peers2 = node2.shared_state.get_peers().await;
    
    println!("Node 1 peers after join: {:?}", peers1);
    println!("Node 2 peers after join: {:?}", peers2);
    
    // Verify they see each other
    assert!(!peers1.is_empty(), "Node 1 should have peers");
    assert!(!peers2.is_empty(), "Node 2 should have peers");
    
    // Check P2P addresses
    for peer in &peers1 {
        println!("Node 1 sees peer {}: p2p_addresses={:?}, p2p_node_id={:?}", 
                 peer.id, peer.p2p_addresses, peer.p2p_node_id);
        assert!(!peer.p2p_addresses.is_empty(), "Peer should have P2P addresses");
    }
    
    for peer in &peers2 {
        println!("Node 2 sees peer {}: p2p_addresses={:?}, p2p_node_id={:?}", 
                 peer.id, peer.p2p_addresses, peer.p2p_node_id);
        assert!(!peer.p2p_addresses.is_empty(), "Peer should have P2P addresses");
    }
    
    println!("Simple join test passed!");
}