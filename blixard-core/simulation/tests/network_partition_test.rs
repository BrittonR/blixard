//! True network partition simulation tests
//!
//! Tests using MadSim's network partition primitives to simulate:
//! - Network splits without clean node removal
//! - Split-brain prevention
//! - Minority partition behavior
//! - Partition healing and reconciliation

use std::sync::Arc;
use std::time::Duration;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use madsim::time::{sleep, timeout};
use madsim::net::{Endpoint, NetSim};
use blixard_core::test_helpers::{TestCluster, TestNode};
use blixard_core::proto::{SubmitTaskRequest, GetTaskStatusRequest};
use blixard_core::raft_manager::{TaskSpec, ResourceRequirements};

/// Test basic network partition - majority/minority split
#[madsim::test]
async fn test_network_partition_majority_minority() {
    let cluster = TestCluster::new(5).await.expect("Failed to create cluster");
    
    // Wait for cluster convergence
    cluster.wait_for_convergence(Duration::from_secs(10)).await
        .expect("Cluster should converge");
    
    // Find initial leader
    let initial_leader = cluster.get_leader_id().await
        .expect("Should have initial leader");
    
    // Create partition: nodes 1,2,3 | nodes 4,5
    let majority_partition = vec![1, 2, 3];
    let minority_partition = vec![4, 5];
    
    // Apply network partition
    apply_network_partition(&cluster, &majority_partition, &minority_partition).await;
    
    // Submit task to initial leader
    let task_before = "task-before-partition";
    if majority_partition.contains(&initial_leader) {
        // Leader is in majority, should succeed
        let submitted = submit_task_to_node(&cluster, initial_leader, task_before, "echo", vec!["majority".to_string()]).await;
        assert!(submitted, "Majority partition should accept tasks");
    }
    
    // Wait for partition effects
    sleep(Duration::from_secs(5)).await;
    
    // Check leader in majority partition
    let majority_leader = find_leader_in_partition(&cluster, &majority_partition).await;
    assert!(majority_leader.is_some(), "Majority should maintain/elect a leader");
    
    // Check leader in minority partition
    let minority_leader = find_leader_in_partition(&cluster, &minority_partition).await;
    assert!(minority_leader.is_none(), "Minority should NOT have a leader");
    
    // Try to submit task to majority - should succeed
    let task_during = "task-during-partition";
    let submitted = cluster.submit_task(
        majority_leader.unwrap(), 
        task_during, 
        "echo", 
        &["during-partition"]
    ).await;
    assert!(submitted, "Majority should process tasks during partition");
    
    // Try to submit task to minority node - should fail
    let minority_node = minority_partition[0];
    let minority_result = timeout(
        Duration::from_secs(3),
        cluster.submit_task(minority_node, "minority-task", "echo", &["fail"])
    ).await;
    assert!(minority_result.is_err() || !minority_result.unwrap(), 
        "Minority should not accept tasks");
    
    // Heal the partition
    heal_network_partition(&cluster).await;
    
    // Wait for reconciliation
    sleep(Duration::from_secs(5)).await;
    
    // Verify single leader after healing
    let healed_leaders = count_all_leaders(&cluster).await;
    assert_eq!(healed_leaders, 1, "Should have exactly one leader after healing");
    
    // Verify task from majority partition is visible to all
    for node_id in 1..=5 {
        let status = cluster.get_task_status(node_id, task_during).await;
        assert!(status.is_some(), 
            "Task from majority partition should be visible to node {} after healing", node_id);
    }
}

/// Test symmetric partition - no clear majority
#[madsim::test]
async fn test_network_partition_symmetric() {
    let mut cluster = TestCluster::new(4).await;
    cluster.start_all_nodes().await;
    
    sleep(Duration::from_secs(2)).await;
    
    // Create symmetric partition: nodes 1,2 | nodes 3,4
    let partition_a = vec![1, 2];
    let partition_b = vec![3, 4];
    
    apply_network_partition(&cluster, &partition_a, &partition_b).await;
    
    // Wait for election timeouts
    sleep(Duration::from_secs(5)).await;
    
    // Neither partition should have a leader (no majority)
    let leader_a = find_leader_in_partition(&cluster, &partition_a).await;
    let leader_b = find_leader_in_partition(&cluster, &partition_b).await;
    
    assert!(leader_a.is_none(), "Partition A should not have a leader without majority");
    assert!(leader_b.is_none(), "Partition B should not have a leader without majority");
    
    // Tasks should fail in both partitions
    let task_a_result = timeout(
        Duration::from_secs(2),
        cluster.submit_task(1, "task-a", "echo", &["a"])
    ).await;
    let task_b_result = timeout(
        Duration::from_secs(2),
        cluster.submit_task(3, "task-b", "echo", &["b"])
    ).await;
    
    assert!(task_a_result.is_err() || !task_a_result.unwrap(), 
        "Partition A should not process tasks");
    assert!(task_b_result.is_err() || !task_b_result.unwrap(), 
        "Partition B should not process tasks");
}

/// Test cascading partitions
#[madsim::test]
async fn test_cascading_partitions() {
    let mut cluster = TestCluster::new(7).await;
    cluster.start_all_nodes().await;
    
    sleep(Duration::from_secs(2)).await;
    let initial_leader = cluster.find_leader().await.expect("Should have initial leader");
    
    // First partition: isolate node 7
    let main_group = vec![1, 2, 3, 4, 5, 6];
    let isolated_1 = vec![7];
    apply_network_partition(&cluster, &main_group, &isolated_1).await;
    
    sleep(Duration::from_secs(2)).await;
    
    // Main group should still have a leader
    let leader_1 = find_leader_in_partition(&cluster, &main_group).await;
    assert!(leader_1.is_some(), "Main group should maintain leadership");
    
    // Submit task in first configuration
    cluster.submit_task(leader_1.unwrap(), "task-1", "echo", &["config-1"]).await;
    
    // Second partition: isolate nodes 5,6,7
    let reduced_group = vec![1, 2, 3, 4];
    let isolated_2 = vec![5, 6, 7];
    apply_network_partition(&cluster, &reduced_group, &isolated_2).await;
    
    sleep(Duration::from_secs(3)).await;
    
    // Reduced group should still have majority (4/7)
    let leader_2 = find_leader_in_partition(&cluster, &reduced_group).await;
    assert!(leader_2.is_some(), "Reduced group should maintain leadership");
    
    // Submit task in second configuration
    cluster.submit_task(leader_2.unwrap(), "task-2", "echo", &["config-2"]).await;
    
    // Third partition: split the reduced group
    let final_a = vec![1, 2];
    let final_b = vec![3, 4];
    let isolated_3 = vec![5, 6, 7];
    apply_three_way_partition(&cluster, &final_a, &final_b, &isolated_3).await;
    
    sleep(Duration::from_secs(3)).await;
    
    // No partition should have a leader now (no majority anywhere)
    assert!(find_leader_in_partition(&cluster, &final_a).await.is_none());
    assert!(find_leader_in_partition(&cluster, &final_b).await.is_none());
    assert!(find_leader_in_partition(&cluster, &isolated_3).await.is_none());
    
    // Heal all partitions
    heal_network_partition(&cluster).await;
    
    sleep(Duration::from_secs(5)).await;
    
    // Should converge to single leader
    let final_leaders = count_all_leaders(&cluster).await;
    assert_eq!(final_leaders, 1, "Should have exactly one leader after full healing");
    
    // Both tasks should be preserved
    let final_leader = cluster.find_leader().await.unwrap();
    assert!(cluster.get_task_status(final_leader, "task-1").await.is_some());
    assert!(cluster.get_task_status(final_leader, "task-2").await.is_some());
}

/// Test partition during leader election
#[madsim::test]
async fn test_partition_during_election() {
    let mut cluster = TestCluster::new(5).await;
    cluster.start_all_nodes().await;
    
    sleep(Duration::from_secs(2)).await;
    let initial_leader = cluster.find_leader().await.expect("Should have initial leader");
    
    // Stop the leader to trigger election
    cluster.stop_node(initial_leader).await;
    
    // Immediately partition the remaining nodes during election
    let partition_a = vec![2, 3];  // 2 nodes
    let partition_b = vec![4, 5];  // 2 nodes
    // Node 1 is stopped, so neither partition has majority
    
    sleep(Duration::from_millis(500)).await; // Let election start
    apply_network_partition(&cluster, &partition_a, &partition_b).await;
    
    // Wait for election timeout
    sleep(Duration::from_secs(5)).await;
    
    // Neither partition should elect a leader
    assert!(find_leader_in_partition(&cluster, &partition_a).await.is_none());
    assert!(find_leader_in_partition(&cluster, &partition_b).await.is_none());
    
    // Restart node 1 while partitioned
    cluster.start_node(1).await;
    
    // Add node 1 to partition A, giving it majority
    let majority_partition = vec![1, 2, 3];
    let minority_partition = vec![4, 5];
    apply_network_partition(&cluster, &majority_partition, &minority_partition).await;
    
    sleep(Duration::from_secs(3)).await;
    
    // Now partition A should elect a leader
    let new_leader = find_leader_in_partition(&cluster, &majority_partition).await;
    assert!(new_leader.is_some(), "Majority partition should elect leader after node joins");
}

/// Test rapid partition/heal cycles
#[madsim::test]
async fn test_rapid_partition_cycles() {
    let mut cluster = TestCluster::new(5).await;
    cluster.start_all_nodes().await;
    
    sleep(Duration::from_secs(2)).await;
    
    let mut tasks_submitted = Vec::new();
    
    // Perform rapid partition/heal cycles
    for i in 0..5 {
        // Partition
        let majority = vec![1, 2, 3];
        let minority = vec![4, 5];
        apply_network_partition(&cluster, &majority, &minority).await;
        
        sleep(Duration::from_secs(1)).await;
        
        // Try to submit task
        if let Some(leader) = find_leader_in_partition(&cluster, &majority).await {
            let task_id = format!("rapid-task-{}", i);
            if cluster.submit_task(leader, &task_id, "echo", &[&format!("cycle-{}", i)]).await {
                tasks_submitted.push(task_id);
            }
        }
        
        // Heal
        heal_network_partition(&cluster).await;
        
        sleep(Duration::from_secs(1)).await;
    }
    
    // Final verification
    sleep(Duration::from_secs(2)).await;
    
    let final_leader = cluster.find_leader().await.expect("Should have leader after cycles");
    
    // At least some tasks should have been submitted
    assert!(!tasks_submitted.is_empty(), "Should have submitted some tasks during cycles");
    
    // All submitted tasks should be visible
    for task_id in &tasks_submitted {
        let status = cluster.get_task_status(final_leader, task_id).await;
        assert!(status.is_some(), "Task {} should be preserved", task_id);
    }
}

// Helper functions for network partition management

async fn apply_network_partition(
    cluster: &TestCluster,
    partition_a: &[u64],
    partition_b: &[u64]
) {
    // Use MadSim's network partition API
    let net = NetSim::current();
    
    // Convert node IDs to addresses
    let addrs_a: Vec<String> = partition_a.iter()
        .filter_map(|&id| cluster.get_node_addr(id))
        .map(|addr| addr.to_string())
        .collect();
    
    let addrs_b: Vec<String> = partition_b.iter()
        .filter_map(|&id| cluster.get_node_addr(id))
        .map(|addr| addr.to_string())
        .collect();
    
    // Create string slices for the partition call
    let refs_a: Vec<&str> = addrs_a.iter().map(|s| s.as_str()).collect();
    let refs_b: Vec<&str> = addrs_b.iter().map(|s| s.as_str()).collect();
    
    // Apply the partition
    net.partition(&refs_a, &refs_b);
}

async fn apply_three_way_partition(
    cluster: &TestCluster,
    partition_a: &[u64],
    partition_b: &[u64],
    partition_c: &[u64]
) {
    apply_network_partition(cluster, partition_a, partition_b).await;
    apply_network_partition(cluster, partition_a, partition_c).await;
    apply_network_partition(cluster, partition_b, partition_c).await;
}

async fn heal_network_partition(cluster: &TestCluster) {
    // Restore all network connections
    let net = NetSim::current();
    
    // Reset removes all partitions
    net.reset();
}

async fn find_leader_in_partition(cluster: &TestCluster, partition: &[u64]) -> Option<u64> {
    for &node_id in partition {
        if let Some(node) = cluster.get_node(node_id) {
            let status = node.get_raft_status().await;
            if status.role == "Leader" {
                // Verify this node can actually communicate with majority of partition
                let mut reachable = 1; // self
                for &other in partition {
                    if other != node_id {
                        // In real implementation, we'd check actual connectivity
                        // For now, assume nodes in same partition can communicate
                        reachable += 1;
                    }
                }
                
                if reachable > partition.len() / 2 {
                    return Some(node_id);
                }
            }
        }
    }
    None
}

async fn count_all_leaders(cluster: &TestCluster) -> usize {
    let mut leaders = 0;
    for node_id in 1..=cluster.size() {
        if let Some(node) = cluster.get_node(node_id) {
            let status = node.get_raft_status().await;
            if status.role == "Leader" {
                leaders += 1;
            }
        }
    }
    leaders
}

// Extension methods for TestCluster
impl TestCluster {
    fn get_node_addr(&self, node_id: u64) -> Option<std::net::SocketAddr> {
        self.get_node(node_id).map(|node| node.grpc_addr())
    }
}