//! Simple test to debug join cluster functionality
#![cfg(feature = "test-helpers")]

use blixard_core::metrics_otel;
use blixard_core::test_helpers::TestNode;
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
    let node1 = TestNode::start(1, 9001)
        .await
        .expect("Failed to create node 1");
    println!("Created bootstrap node 1 at {}", node1.addr);

    // Wait for bootstrap node to be ready
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check node 1 peers (should be empty)
    let peers1_before = node1.shared_state.get_peers().await;
    println!("Node 1 peers before join: {:?}", peers1_before);

    // Create node 2 that joins node 1
    // IMPORTANT: Use start_with_join to prevent node 2 from bootstrapping as its own cluster
    let node2 = TestNode::start_with_join(2, 9002, Some(node1.addr))
        .await
        .expect("Failed to create node 2");
    println!(
        "Created node 2 at {} and joined cluster via {}",
        node2.addr, node1.addr
    );

    // Wait for propagation and leader election
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check the authoritative voter list from Raft
    let voters1 = node1
        .shared_state
        .get_current_voters()
        .await
        .expect("Should get voters from node 1");
    let voters2 = node2
        .shared_state
        .get_current_voters()
        .await
        .expect("Should get voters from node 2");

    println!("Node 1 voters after join: {:?}", voters1);
    println!("Node 2 voters after join: {:?}", voters2);

    // Also check the local peer cache for debugging
    let peers1 = node1.shared_state.get_peers().await;
    let peers2 = node2.shared_state.get_peers().await;

    println!("Node 1 peers (local cache) after join: {:?}", peers1);
    println!("Node 2 peers (local cache) after join: {:?}", peers2);

    // Check Raft status
    let raft_status1 = node1
        .shared_state
        .get_raft_status()
        .await
        .expect("Should get raft status from node 1");
    let raft_status2 = node2
        .shared_state
        .get_raft_status()
        .await
        .expect("Should get raft status from node 2");

    println!(
        "Node 1 Raft status: is_leader={}, leader_id={:?}, term={}",
        raft_status1.is_leader, raft_status1.leader_id, raft_status1.term
    );
    println!(
        "Node 2 Raft status: is_leader={}, leader_id={:?}, term={}",
        raft_status2.is_leader, raft_status2.leader_id, raft_status2.term
    );

    // Verify they see each other in the voter list
    assert!(
        voters1.len() >= 2,
        "Node 1 should see at least 2 voters, got: {:?}",
        voters1
    );
    assert!(
        voters2.len() >= 2,
        "Node 2 should see at least 2 voters, got: {:?}",
        voters2
    );
    assert!(
        voters1.contains(&1),
        "Node 1 voter list should contain node 1"
    );
    assert!(
        voters1.contains(&2),
        "Node 1 voter list should contain node 2"
    );
    assert!(
        voters2.contains(&1),
        "Node 2 voter list should contain node 1"
    );
    assert!(
        voters2.contains(&2),
        "Node 2 voter list should contain node 2"
    );

    // Check P2P addresses
    for peer in &peers1 {
        println!(
            "Node 1 sees peer {}: p2p_addresses={:?}, p2p_node_id={:?}",
            peer.id, peer.p2p_addresses, peer.p2p_node_id
        );
        assert!(
            !peer.p2p_addresses.is_empty(),
            "Peer should have P2P addresses"
        );
    }

    for peer in &peers2 {
        println!(
            "Node 2 sees peer {}: p2p_addresses={:?}, p2p_node_id={:?}",
            peer.id, peer.p2p_addresses, peer.p2p_node_id
        );
        assert!(
            !peer.p2p_addresses.is_empty(),
            "Peer should have P2P addresses"
        );
    }

    println!("Simple join test passed!");
}
