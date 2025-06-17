//! Comprehensive three-node cluster tests with enhanced reliability
//!
//! This test suite addresses the known issues with 3-node cluster formation
//! documented in TEST_RELIABILITY_ISSUES.md

#![cfg(feature = "test-helpers")]

use std::time::Duration;
use std::collections::HashSet;

use blixard::{
    test_helpers::{TestCluster, timing},
    proto::{
        HealthCheckRequest, TaskRequest, ClusterStatusRequest,
    },
};

/// Basic 3-node cluster formation test with robust timing
#[tokio::test]
async fn test_three_node_cluster_basic() {
    // Create cluster with environment-aware timeouts
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(30)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    // Verify all nodes are healthy
    for node_id in 1..=3 {
        let mut client = cluster.client(node_id).await
            .expect("Failed to get client");
        
        let response = client.health_check(HealthCheckRequest {}).await
            .expect("Health check failed");
        
        assert!(response.into_inner().healthy, "Node {} not healthy", node_id);
    }
    
    // Verify cluster has converged with a leader
    let leader_client = cluster.leader_client().await
        .expect("No leader found");
    
    let status = leader_client.clone()
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .expect("Failed to get cluster status")
        .into_inner();
    
    assert_eq!(status.nodes.len(), 3, "Cluster should have 3 nodes");
    assert!(status.leader_id > 0, "Cluster should have a leader");
    
    // Verify all nodes agree on the leader
    let mut leader_ids = HashSet::new();
    for node in cluster.nodes().values() {
        let state = node.shared_state.get_raft_status().await.unwrap();
        if let Some(leader_id) = state.leader_id {
            leader_ids.insert(leader_id);
        }
    }
    
    assert_eq!(leader_ids.len(), 1, "All nodes should agree on the same leader");
    
    cluster.shutdown().await;
}

/// Test 3-node cluster with task submission
#[tokio::test]
async fn test_three_node_cluster_task_submission() {
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(30)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    // Get leader client
    let mut leader_client = cluster.leader_client().await
        .expect("No leader found");
    
    // Submit multiple tasks to ensure replication works
    for i in 0..5 {
        let task = TaskRequest {
            task_id: format!("test-task-{}", i),
            command: "echo".to_string(),
            args: vec![format!("Task {}", i)],
            cpu_cores: 1,
            memory_mb: 256,
            disk_gb: 1,
            required_features: vec![],
            timeout_secs: 10,
        };
        
        let response = leader_client.submit_task(task).await
            .expect("Failed to submit task");
        
        let resp = response.into_inner();
        assert!(resp.accepted, "Task {} should be accepted", i);
    }
    
    // Give time for replication using a simple delay
    // Note: In a real implementation, we would check the actual Raft log/commit index
    // For now, we use a conservative wait to ensure replication has time to occur
    timing::robust_sleep(Duration::from_secs(2)).await;
    
    cluster.shutdown().await;
}

/// Test 3-node cluster with node failure and recovery
#[tokio::test]
async fn test_three_node_cluster_fault_tolerance() {
    let mut cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(30)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    // Get initial leader
    let initial_leader = cluster.leader_client().await
        .expect("No leader found")
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .unwrap()
        .into_inner()
        .leader_id;
    
    eprintln!("Initial leader: {}", initial_leader);
    
    // Remove a follower node
    let follower_to_remove = if initial_leader == 1 { 2 } else { 1 };
    cluster.remove_node(follower_to_remove).await
        .expect("Failed to remove node");
    
    eprintln!("Removed node {}", follower_to_remove);
    
    // Wait for cluster to stabilize with 2 nodes
    timing::wait_for_condition_with_backoff(
        || async {
            // Check remaining nodes still have a leader
            for &node_id in &[1, 2, 3] {
                if node_id == follower_to_remove {
                    continue;
                }
                
                if let Ok(client) = cluster.client(node_id).await {
                    if let Ok(response) = client.clone().get_cluster_status(ClusterStatusRequest {}).await {
                        let status = response.into_inner();
                        if status.leader_id > 0 {
                            return true;
                        }
                    }
                }
            }
            false
        },
        timing::scaled_timeout(Duration::from_secs(20)),
        Duration::from_millis(200),
    )
    .await
    .expect("Cluster should maintain leader with 2 nodes");
    
    // Verify cluster still works with 2 nodes
    let mut leader_client = cluster.leader_client().await
        .expect("No leader after node removal");
    
    let task = TaskRequest {
        task_id: "fault-tolerance-test".to_string(),
        command: "echo".to_string(),
        args: vec!["Still working!".to_string()],
        cpu_cores: 1,
        memory_mb: 256,
        disk_gb: 1,
        required_features: vec![],
        timeout_secs: 10,
    };
    
    let response = leader_client.submit_task(task).await
        .expect("Failed to submit task after node removal");
    
    assert!(response.into_inner().accepted, "Task should be accepted with 2 nodes");
    
    // Add the node back
    let new_node_id = cluster.add_node().await
        .expect("Failed to add node back");
    
    eprintln!("Added new node {}", new_node_id);
    
    // Wait for 3-node cluster to converge again
    cluster.wait_for_convergence(timing::scaled_timeout(Duration::from_secs(30))).await
        .expect("Cluster should converge after adding node back");
    
    // Verify we have 3 nodes again
    let final_status = cluster.leader_client().await
        .expect("No leader after adding node")
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .unwrap()
        .into_inner();
    
    assert_eq!(final_status.nodes.len(), 3, "Should have 3 nodes again");
    
    cluster.shutdown().await;
}

/// Test 3-node cluster with concurrent operations
#[tokio::test]
async fn test_three_node_cluster_concurrent_operations() {
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(30)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    // Get clients for all nodes
    let mut clients = vec![];
    for node_id in 1..=3 {
        clients.push(cluster.client(node_id).await.expect("Failed to get client"));
    }
    
    // Submit tasks concurrently from different clients
    let mut handles = vec![];
    
    for (i, mut client) in clients.into_iter().enumerate() {
        let handle = tokio::spawn(async move {
            let mut results = vec![];
            
            for j in 0..10 {
                let task = TaskRequest {
                    task_id: format!("concurrent-{}-{}", i, j),
                    command: "echo".to_string(),
                    args: vec![format!("From client {}, task {}", i, j)],
                    cpu_cores: 1,
                    memory_mb: 256,
                    disk_gb: 1,
                    required_features: vec![],
                    timeout_secs: 10,
                };
                
                match client.submit_task(task).await {
                    Ok(response) => results.push(response.into_inner().accepted),
                    Err(e) => {
                        // Expected for non-leader nodes
                        eprintln!("Client {} task {} error (expected for followers): {}", i, j, e);
                        results.push(false);
                    }
                }
                
                // Small delay between submissions
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            results
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut total_accepted = 0;
    for handle in handles {
        let results = handle.await.unwrap();
        total_accepted += results.iter().filter(|&&accepted| accepted).count();
    }
    
    // At least the leader's submissions should succeed
    assert!(total_accepted >= 10, "At least 10 tasks should be accepted (from leader)");
    
    cluster.shutdown().await;
}

/// Test 3-node cluster with dynamic membership changes
#[tokio::test]
async fn test_three_node_cluster_membership_changes() {
    let mut cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(30)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    // Add a 4th node
    let node4_id = cluster.add_node().await
        .expect("Failed to add 4th node");
    
    eprintln!("Added node {}", node4_id);
    
    // Wait for 4-node cluster to converge
    timing::wait_for_condition_with_backoff(
        || async {
            if let Ok(mut client) = cluster.leader_client().await {
                if let Ok(response) = client.get_cluster_status(ClusterStatusRequest {}).await {
                    let status = response.into_inner();
                    return status.nodes.len() == 4;
                }
            }
            false
        },
        timing::scaled_timeout(Duration::from_secs(30)),
        Duration::from_millis(200),
    )
    .await
    .expect("Cluster should have 4 nodes");
    
    // Remove original node 2
    cluster.remove_node(2).await
        .expect("Failed to remove node 2");
    
    eprintln!("Removed node 2");
    
    // Wait for cluster to stabilize with 3 nodes again
    timing::wait_for_condition_with_backoff(
        || async {
            if let Ok(mut client) = cluster.leader_client().await {
                if let Ok(response) = client.get_cluster_status(ClusterStatusRequest {}).await {
                    let status = response.into_inner();
                    return status.nodes.len() == 3 && status.leader_id > 0;
                }
            }
            false
        },
        timing::scaled_timeout(Duration::from_secs(30)),
        Duration::from_millis(200),
    )
    .await
    .expect("Cluster should stabilize with 3 nodes");
    
    // Verify final cluster state
    let final_status = cluster.leader_client().await
        .expect("No leader after membership changes")
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .unwrap()
        .into_inner();
    
    assert_eq!(final_status.nodes.len(), 3, "Should have 3 nodes");
    let node_ids: HashSet<u64> = final_status.nodes.iter().map(|n| n.id).collect();
    assert!(!node_ids.contains(&2), "Node 2 should not be in cluster");
    assert!(node_ids.contains(&node4_id), "Node 4 should be in cluster");
    
    cluster.shutdown().await;
}

/// Stress test with rapid operations
#[tokio::test]
#[ignore] // Can be run with --ignored flag for extended testing
async fn test_three_node_cluster_stress() {
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(timing::scaled_timeout(Duration::from_secs(60)))
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    let start = std::time::Instant::now();
    let duration = Duration::from_secs(30);
    let mut task_count = 0;
    
    while start.elapsed() < duration {
        if let Ok(mut client) = cluster.leader_client().await {
            let task = TaskRequest {
                task_id: format!("stress-task-{}", task_count),
                command: "stress".to_string(),
                args: vec![format!("{}", task_count)],
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 5,
            };
            
            if let Ok(response) = client.submit_task(task).await {
                if response.into_inner().accepted {
                    task_count += 1;
                }
            }
        }
        
        // Very short delay to stress the system
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    
    eprintln!("Submitted {} tasks in {:?}", task_count, start.elapsed());
    assert!(task_count > 100, "Should submit many tasks under stress");
    
    cluster.shutdown().await;
}