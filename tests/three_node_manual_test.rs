//! Manual 3-node cluster test that works around known reliability issues
//!
//! This test demonstrates a more reliable approach to 3-node cluster testing
//! by manually managing node creation and join operations.

#![cfg(feature = "test-helpers")]

use std::time::Duration;
use blixard::{
    test_helpers::TestNode,
    proto::HealthCheckRequest,
};


#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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
    let mut retries = 0;
    let mut is_leader = false;
    loop {
        let status = node1.shared_state.get_raft_status().await.unwrap();
        if status.is_leader {
            is_leader = true;
            break;
        }
        retries += 1;
        if retries > 100 {
            eprintln!("Warning: Node 1 did not become leader in time, continuing anyway");
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    if is_leader {
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
    
    eprintln!("Created node 2 at {} (will auto-join)", node2.addr);
    
    // Wait for node 2 to be added to cluster
    let mut retries = 0;
    loop {
        let (_, peers, _) = node1.shared_state.get_cluster_status().await.unwrap();
        if peers.len() == 2 {
            break;
        }
        retries += 1;
        if retries > 50 {
            panic!("Node 2 failed to join cluster after {} retries", retries);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
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
    
    eprintln!("Created node 3 at {} (will auto-join)", node3.addr);
    
    // Wait for node 3 to be added to cluster
    let mut retries = 0;
    loop {
        let (_, peers, _) = node1.shared_state.get_cluster_status().await.unwrap();
        if peers.len() == 3 {
            break;
        }
        retries += 1;
        if retries > 50 {
            panic!("Node 3 failed to join cluster after {} retries", retries);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
    // Trigger log replication again
    let _ = client1.health_check(HealthCheckRequest {}).await;
    
    // Step 4: Wait for all nodes to converge on cluster state
    let mut retries = 0;
    loop {
        let mut all_see_three = true;
        for node in [&node1, &node2, &node3] {
            let (_, peers, _) = node.shared_state.get_cluster_status().await.unwrap();
            if peers.len() != 3 {
                all_see_three = false;
                break;
            }
        }
        if all_see_three {
            break;
        }
        retries += 1;
        if retries > 50 {
            panic!("Not all nodes converged to 3-node cluster after {} retries", retries);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
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
    
    // Check all nodes agree on leader - wait for convergence
    let mut retries = 0;
    let mut final_leader_id = None;
    loop {
        let mut leader_ids = vec![];
        let mut all_know_leader = true;
        
        for (id, node) in [(1, &node1), (2, &node2), (3, &node3)] {
            let status = node.shared_state.get_raft_status().await.unwrap();
            eprintln!("Node {} state: {}, leader: {:?}", id, status.state, status.leader_id);
            match status.leader_id {
                Some(leader_id) => leader_ids.push(leader_id),
                None => all_know_leader = false,
            }
        }
        
        // Check if all nodes know a leader and agree
        if all_know_leader && !leader_ids.is_empty() && 
           leader_ids.iter().all(|&id| id == leader_ids[0]) {
            final_leader_id = Some(leader_ids[0]);
            break;
        }
        
        retries += 1;
        if retries > 50 {
            panic!("Nodes did not converge on leader after {} retries. Leader IDs: {:?}", 
                   retries, leader_ids);
        }
        
        // Trigger log replication to help convergence
        if retries % 10 == 5 {
            let _ = client1.health_check(HealthCheckRequest {}).await;
        }
        
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
    assert!(final_leader_id.is_some(), "No leader found");
    eprintln!("✓ All nodes agree on leader: {}", final_leader_id.unwrap());
    
    // Clean up
    node3.shutdown().await;
    node2.shutdown().await;
    node1.shutdown().await;
    
    eprintln!("✓ Test completed successfully");
}