//! Test three-node cluster formation
#![cfg(feature = "test-helpers")]

use blixard_core::metrics_otel;
use blixard_core::test_helpers::TestNode;
use std::time::Duration;

#[tokio::test]
async fn test_three_node_cluster_join() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=info")
        .try_init();

    // Initialize metrics
    metrics_otel::init_noop();

    println!("=== Creating three-node cluster ===");

    // Create bootstrap node
    let node1 = TestNode::start(1, 9001)
        .await
        .expect("Failed to create node 1");
    println!("Created bootstrap node 1 at {}", node1.addr);

    // Wait for bootstrap node to be ready
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Create node 2 that joins node 1
    let node2 = TestNode::start_with_join(2, 9002, Some(node1.addr))
        .await
        .expect("Failed to create node 2");
    println!(
        "Created node 2 at {} and joined via {}",
        node2.addr, node1.addr
    );

    // Wait a bit for node 2 to fully join
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create node 3 that joins via node 1
    let node3 = TestNode::start_with_join(3, 9003, Some(node1.addr))
        .await
        .expect("Failed to create node 3");
    println!(
        "Created node 3 at {} and joined via {}",
        node3.addr, node1.addr
    );

    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check voters from all nodes
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
    let voters3 = node3
        .shared_state
        .get_current_voters()
        .await
        .expect("Should get voters from node 3");

    println!("Node 1 sees voters: {:?}", voters1);
    println!("Node 2 sees voters: {:?}", voters2);
    println!("Node 3 sees voters: {:?}", voters3);

    // All nodes should see all three voters
    assert_eq!(voters1.len(), 3, "Node 1 should see 3 voters");
    assert_eq!(voters2.len(), 3, "Node 2 should see 3 voters");
    assert_eq!(voters3.len(), 3, "Node 3 should see 3 voters");

    // Check that all nodes see the same set of voters
    let mut sorted_voters1 = voters1.clone();
    let mut sorted_voters2 = voters2.clone();
    let mut sorted_voters3 = voters3.clone();
    sorted_voters1.sort();
    sorted_voters2.sort();
    sorted_voters3.sort();

    assert_eq!(
        sorted_voters1,
        vec![1, 2, 3],
        "Node 1 should see voters [1, 2, 3]"
    );
    assert_eq!(
        sorted_voters2,
        vec![1, 2, 3],
        "Node 2 should see voters [1, 2, 3]"
    );
    assert_eq!(
        sorted_voters3,
        vec![1, 2, 3],
        "Node 3 should see voters [1, 2, 3]"
    );

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
    let raft_status3 = node3
        .shared_state
        .get_raft_status()
        .await
        .expect("Should get raft status from node 3");

    println!(
        "Node 1 Raft status: is_leader={}, leader_id={:?}, term={}",
        raft_status1.is_leader, raft_status1.leader_id, raft_status1.term
    );
    println!(
        "Node 2 Raft status: is_leader={}, leader_id={:?}, term={}",
        raft_status2.is_leader, raft_status2.leader_id, raft_status2.term
    );
    println!(
        "Node 3 Raft status: is_leader={}, leader_id={:?}, term={}",
        raft_status3.is_leader, raft_status3.leader_id, raft_status3.term
    );

    println!("Three-node cluster test passed!");
}
