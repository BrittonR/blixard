//! Debug test for multi-node clustering issues
#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers::{TestCluster, timing};
use std::time::Duration;

#[tokio::test]
async fn test_simple_two_node_join() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    // Start with a single node
    let mut cluster = TestCluster::builder()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create single node cluster");
    
    println!("Created single node cluster");
    
    // Verify node 1 is the leader
    {
        let node1 = cluster.get_node(1).expect("Node 1 should exist");
        let status1 = node1.shared_state.get_raft_status().await.expect("Should get raft status");
        println!("Node 1 status: {:?}", status1);
        assert!(status1.is_leader, "Node 1 should be leader");
    }
    
    // Add a second node
    println!("Adding node 2...");
    let node2_result = cluster.add_node().await;
    
    match node2_result {
        Ok(_) => {
            println!("Node 2 added successfully");
            
            // Wait for convergence
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Check both nodes' status
            let node2 = cluster.get_node(2).expect("Node 2 should exist");
            let status2 = node2.shared_state.get_raft_status().await.expect("Should get raft status");
            println!("Node 2 status: {:?}", status2);
            
            // Verify cluster state
            let node1 = cluster.get_node(1).expect("Node 1 should exist");
            let (leader_id, node_ids, _term) = node1.shared_state.get_cluster_status().await.expect("Should get cluster status");
            println!("Cluster status - Leader: {}, Nodes: {:?}", leader_id, node_ids);
            assert_eq!(node_ids.len(), 2, "Should have 2 nodes");
        }
        Err(e) => {
            println!("Failed to add node 2: {:?}", e);
            panic!("Node join failed: {:?}", e);
        }
    }
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_node_joins() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    // Start with a single node
    let mut cluster = TestCluster::builder()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create single node cluster");
    
    println!("Created single node cluster");
    
    // Note: TestCluster doesn't support concurrent modification
    // We'll add nodes sequentially instead
    let mut results = Vec::new();
    
    for i in 0..2 {
        println!("Attempting to add node {}...", i + 2);
        let result = cluster.add_node().await;
        results.push((i + 2, result));
    }
    
    // Process results
    let mut successes = 0;
    let mut failures = 0;
    
    for (node_num, result) in results {
        match result {
            Ok(node_id) => {
                println!("Node {} (id {}) joined successfully", node_num, node_id);
                successes += 1;
            }
            Err(e) => {
                println!("Node {} failed to join: {:?}", node_num, e);
                failures += 1;
            }
        }
    }
    
    println!("Join results: {} successes, {} failures", successes, failures);
    
    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Check final cluster state
    let node1 = cluster.get_node(1).expect("Node 1 should exist");
    let (leader_id, node_ids, _term) = node1.shared_state.get_cluster_status().await.expect("Should get cluster status");
    println!("Final cluster status - Leader: {}, Nodes: {:?}", leader_id, node_ids);
    println!("Final node count: {}", node_ids.len());
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_join_with_network_delays() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    // This test would require network simulation capabilities
    // For now, just test basic join with artificial delays
    
    let mut cluster = TestCluster::builder()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create single node cluster");
    
    // Add artificial delay before join
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let join_result = cluster.add_node().await;
    println!("Join result after delay: {:?}", join_result);
    
    if join_result.is_ok() {
        // Check if nodes can communicate
        let node1 = cluster.get_node(1).expect("Node 1 should exist");
        let node2 = cluster.get_node(2).expect("Node 2 should exist");
        
        // Try a simple operation from node 2
        let vm_list = node2.node.list_vms().await;
        println!("VM list from node 2: {:?}", vm_list);
    }
    
    cluster.shutdown().await;
}