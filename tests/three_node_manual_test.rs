//! Manual 3-node cluster test that works around known reliability issues
//!
//! This test demonstrates a more reliable approach to 3-node cluster testing
//! by manually managing node creation and join operations.

#![cfg(feature = "test-helpers")]

use std::time::Duration;
use blixard::{
    test_helpers::{TestNode, timing},
    proto::HealthCheckRequest,
};

#[tokio::test]
async fn test_three_node_cluster_manual_approach() {
    // Step 1: Create bootstrap node
    let node1 = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node 1");
    
    eprintln!("Created bootstrap node 1 at {}", node1.addr);
    
    // Wait for bootstrap node to elect itself as leader
    let leader_elected = timing::wait_for_condition_with_backoff(
        || async {
            let status = node1.shared_state.get_raft_status().await.unwrap();
            status.is_leader
        },
        timing::scaled_timeout(Duration::from_secs(10)),
        Duration::from_millis(100),
    )
    .await;
    
    if leader_elected.is_err() {
        eprintln!("Warning: Node 1 did not become leader in time, continuing anyway");
    } else {
        eprintln!("✓ Node 1 is leader");
    }
    
    // Step 2: Create and join node 2
    let node2 = TestNode::builder()
        .with_id(2)
        .with_auto_port()
        .with_join_addr(Some(node1.addr))
        .build()
        .await
        .expect("Failed to create node 2");
    
    eprintln!("Created node 2 at {}", node2.addr);
    
    // Send join request
    node2.node.send_join_request().await
        .expect("Failed to send join request from node 2");
    
    // Wait for node 2 to be added
    timing::robust_sleep(Duration::from_secs(3)).await;
    
    // Trigger log replication with a health check
    let mut client1 = node1.client().await.unwrap();
    let _ = client1.health_check(HealthCheckRequest {}).await;
    
    // Step 3: Create and join node 3
    let node3 = TestNode::builder()
        .with_id(3)
        .with_auto_port()
        .with_join_addr(Some(node1.addr))
        .build()
        .await
        .expect("Failed to create node 3");
    
    eprintln!("Created node 3 at {}", node3.addr);
    
    // Send join request
    node3.node.send_join_request().await
        .expect("Failed to send join request from node 3");
    
    // Wait for node 3 to be added
    timing::robust_sleep(Duration::from_secs(3)).await;
    
    // Trigger log replication again
    let _ = client1.health_check(HealthCheckRequest {}).await;
    
    // Step 4: Verify cluster state
    timing::robust_sleep(Duration::from_secs(2)).await;
    
    // Check all nodes agree on cluster size
    let mut cluster_sizes = vec![];
    for (id, node) in [(1, &node1), (2, &node2), (3, &node3)] {
        let (_, peers, _) = node.shared_state.get_cluster_status().await.unwrap();
        eprintln!("Node {} sees {} peers", id, peers.len());
        cluster_sizes.push(peers.len());
    }
    
    // All nodes should see 3 peers
    assert!(cluster_sizes.iter().all(|&size| size == 3), 
            "Not all nodes see 3 peers: {:?}", cluster_sizes);
    
    // Check all nodes agree on leader
    let mut leader_ids = vec![];
    for (id, node) in [(1, &node1), (2, &node2), (3, &node3)] {
        let status = node.shared_state.get_raft_status().await.unwrap();
        eprintln!("Node {} state: {}, leader: {:?}", id, status.state, status.leader_id);
        if let Some(leader_id) = status.leader_id {
            leader_ids.push(leader_id);
        }
    }
    
    // All nodes should agree on the same leader
    assert!(!leader_ids.is_empty(), "No leader found");
    assert!(leader_ids.iter().all(|&id| id == leader_ids[0]), 
            "Nodes disagree on leader: {:?}", leader_ids);
    
    eprintln!("✓ All nodes agree on leader: {}", leader_ids[0]);
    
    // Clean up
    node3.shutdown().await;
    node2.shutdown().await;
    node1.shutdown().await;
    
    eprintln!("✓ Test completed successfully");
}