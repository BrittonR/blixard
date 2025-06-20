//! Network Partition Tests for Distributed Storage
//!
//! This module tests the behavior of the distributed storage layer during network partitions.
//! It simulates various partition scenarios and verifies:
//! - Data consistency during partitions
//! - Split-brain prevention
//! - Data reconciliation after partition healing
//! - Leader election behavior during partitions

#![cfg(feature = "test-helpers")]

use std::time::Duration;
use std::collections::{HashMap, HashSet};
use blixard::{
    proto::{
        CreateVmRequest, ListVmsRequest, ClusterStatusRequest, LeaveRequest,
    },
    test_helpers::{TestCluster, wait_for_condition},
    error::BlixardError,
};
use tokio::time::{sleep, timeout};
use tracing::{info, warn};

/// Helper to wait for a specific leader with retries
async fn wait_for_leader(cluster: &TestCluster, expected_leader: Option<u64>, timeout_duration: Duration) -> Result<u64, String> {
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout_duration {
        if let Some(leader) = get_cluster_leader(cluster).await {
            if expected_leader.is_none() || expected_leader == Some(leader) {
                return Ok(leader);
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    
    Err(format!("No leader found within {:?}", timeout_duration))
}

/// Wait for nodes to recognize no leader (during partition)
async fn wait_for_no_leader(node_ids: &[u64], cluster: &TestCluster, timeout_duration: Duration) -> Result<(), String> {
    wait_for_condition(
        || async {
            for &node_id in node_ids {
                if let Ok(client) = cluster.client(node_id).await {
                    if let Ok(response) = client.clone().get_cluster_status(ClusterStatusRequest {}).await {
                        let status = response.into_inner();
                        if status.leader_id > 0 {
                            return false; // Still has a leader
                        }
                    }
                }
            }
            true // All nodes have no leader
        },
        timeout_duration,
        Duration::from_millis(100),
    )
    .await
    .map_err(|e| format!("Failed to wait for no leader: {:?}", e))
}

/// Simulate a network partition by removing nodes from the cluster
/// Returns the removed TestNodes that can be re-added later
async fn create_partition(
    cluster: &mut TestCluster,
    nodes_to_isolate: &[u64],
) -> Result<HashMap<u64, blixard::test_helpers::TestNode>, BlixardError> {
    let mut isolated_nodes = HashMap::new();
    
    // Remove nodes from cluster to simulate partition
    for &node_id in nodes_to_isolate {
        info!("Isolating node {} from cluster", node_id);
        
        // First, get the node reference to save it
        if let Some(node) = cluster.nodes_mut().remove(&node_id) {
            isolated_nodes.insert(node_id, node);
        }
        
        // Send leave request from remaining nodes to remove the isolated node
        if let Ok(leader_client) = cluster.leader_client().await {
            let leave_request = LeaveRequest { node_id };
            let _ = leader_client.clone().leave_cluster(leave_request).await;
        }
    }
    
    // Wait a bit for Raft to process the configuration changes
    sleep(Duration::from_millis(500)).await;
    
    info!("Created partition isolating nodes: {:?}", nodes_to_isolate);
    Ok(isolated_nodes)
}

/// Heal a partition by re-adding isolated nodes
async fn heal_partition(
    cluster: &mut TestCluster,
    isolated_nodes: HashMap<u64, blixard::test_helpers::TestNode>,
) -> Result<(), BlixardError> {
    info!("Healing partition, re-adding {} nodes", isolated_nodes.len());
    
    // Re-add the isolated nodes back to the cluster
    for (node_id, node) in isolated_nodes {
        cluster.nodes_mut().insert(node_id, node);
    }
    
    // Wait for cluster to stabilize
    cluster.wait_for_convergence(Duration::from_secs(10)).await?;
    
    info!("Partition healed, cluster converged");
    Ok(())
}

/// Test basic partition detection and handling
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_basic_network_partition() {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 5-node cluster
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Get initial leader and verify cluster state
    let initial_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Should have a leader");
    info!("Initial leader: {}", initial_leader);
    
    // Verify all nodes agree on the leader
    let initial_node_count = verify_cluster_agreement(&cluster).await;
    assert_eq!(initial_node_count, 5, "All 5 nodes should agree on cluster state");
    
    // Create VMs before partition
    let mut leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    let vm_count = 3;
    
    for i in 0..vm_count {
        let vm_name = format!("partition-test-vm-{}", i);
        let create_request = CreateVmRequest {
            name: vm_name.clone(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        };
        
        let response = leader_client
            .create_vm(create_request)
            .await
            .expect("Failed to create VM");
        
        assert!(response.into_inner().success, "VM {} creation should succeed", i);
    }
    
    // Wait for replication to all nodes
    wait_for_condition(
        || async {
            let vm_counts = get_vm_count_all_nodes(&cluster).await;
            vm_counts.values().all(|&count| count == vm_count)
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("All nodes should replicate VMs");
    
    // Record pre-partition state
    let pre_partition_vms = get_vm_count_all_nodes(&cluster).await;
    assert_eq!(pre_partition_vms.len(), 5, "Should get VM count from all 5 nodes");
    for (node_id, count) in &pre_partition_vms {
        assert_eq!(*count, vm_count, "Node {} should see {} VMs before partition", node_id, vm_count);
    }
    
    // Simulate network partition: isolate nodes 4,5 (minority)
    info!("Creating network partition - isolating nodes 4,5");
    let minority_nodes = vec![4, 5];
    let isolated = create_partition(&mut cluster, &minority_nodes)
        .await
        .expect("Failed to create partition");
    
    assert_eq!(isolated.len(), 2, "Should have isolated 2 nodes");
    
    // Verify majority partition (nodes 1,2,3) maintains a leader
    let majority_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Majority should maintain/elect a leader");
    
    assert!(majority_leader <= 3, "Leader {} should be from majority partition", majority_leader);
    info!("Majority partition leader: {}", majority_leader);
    
    // Try to create VM in majority partition - should succeed
    leader_client = cluster.leader_client().await.expect("Should get leader client from majority");
    let majority_vm = CreateVmRequest {
        name: "majority-partition-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory_mb: 512,
    };
    
    let majority_response = leader_client
        .create_vm(majority_vm)
        .await
        .expect("Should connect to majority leader");
    
    assert!(majority_response.into_inner().success, "Majority should accept writes");
    
    // Verify majority nodes see the new VM
    wait_for_condition(
        || async {
            let remaining_nodes: Vec<u64> = cluster.nodes().keys().cloned().collect();
            for node_id in &remaining_nodes {
                if let Ok(client) = cluster.client(*node_id).await {
                    if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                        if response.into_inner().vms.len() != vm_count + 1 {
                            return false;
                        }
                    }
                }
            }
            true
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("Majority nodes should see new VM");
    
    // Try to access isolated nodes - should fail or show no leader
    for (node_id, isolated_node) in &isolated {
        match isolated_node.client().await {
            Ok(mut client) => {
                // Node is running but isolated
                match client.get_cluster_status(ClusterStatusRequest {}).await {
                    Ok(response) => {
                        let status = response.into_inner();
                        // Isolated nodes might still think there's a leader initially
                        // but won't be able to make progress
                        info!("Isolated node {} sees leader: {}", node_id, status.leader_id);
                    }
                    Err(e) => {
                        info!("Isolated node {} cannot get status: {}", node_id, e);
                    }
                }
                
                // Try to create VM on isolated node - should fail
                let isolated_vm = CreateVmRequest {
                    name: format!("isolated-vm-{}", node_id),
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 256,
                };
                
                let isolated_result = client.create_vm(isolated_vm).await;
                assert!(isolated_result.is_err() || !isolated_result.unwrap().into_inner().success,
                    "Isolated node {} should not accept writes", node_id);
            }
            Err(e) => {
                info!("Cannot connect to isolated node {}: {}", node_id, e);
            }
        }
    }
    
    // Heal the partition
    info!("Healing network partition");
    heal_partition(&mut cluster, isolated).await.expect("Failed to heal partition");
    
    // Verify all nodes converge to same state
    let post_heal_leader = wait_for_leader(&cluster, None, Duration::from_secs(10))
        .await
        .expect("Should have leader after healing");
    info!("Post-heal leader: {}", post_heal_leader);
    
    // All nodes should eventually see the same VMs (including the one created during partition)
    wait_for_condition(
        || async {
            let vm_counts = get_vm_count_all_nodes(&cluster).await;
            let expected_count = vm_count + 1; // Original VMs + 1 created during partition
            vm_counts.len() == 5 && vm_counts.values().all(|&count| count == expected_count)
        },
        Duration::from_secs(10),
        Duration::from_millis(200),
    )
    .await
    .expect("All nodes should converge to same VM count after healing");
    
    // Final verification
    let final_agreement = verify_cluster_agreement(&cluster).await;
    assert_eq!(final_agreement, 5, "All 5 nodes should agree on final state");
    
    cluster.shutdown().await;
}

/// Test split-brain prevention during network partition
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_split_brain_prevention() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Get initial leader
    let initial_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Should have initial leader");
    info!("Initial leader: {}", initial_leader);
    
    // Create initial data
    let mut leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    let vm_name = "split-brain-test-vm";
    let create_request = CreateVmRequest {
        name: vm_name.to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory_mb: 512,
    };
    
    let response = leader_client
        .create_vm(create_request)
        .await
        .expect("Failed to create VM");
    assert!(response.into_inner().success, "Initial VM creation should succeed");
    
    // Wait for replication
    wait_for_condition(
        || async {
            let vm_counts = get_vm_count_all_nodes(&cluster).await;
            vm_counts.len() == 5 && vm_counts.values().all(|&count| count == 1)
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("All nodes should see initial VM");
    
    // Simulate partition into two groups based on initial leader location
    let (majority_nodes, minority_nodes) = if initial_leader <= 3 {
        // Leader in first group - isolate nodes 4,5
        (vec![1, 2, 3], vec![4, 5])
    } else {
        // Leader in second group - isolate nodes 1,2
        (vec![3, 4, 5], vec![1, 2])
    };
    
    info!("Creating partition - majority: {:?}, minority: {:?}", majority_nodes, minority_nodes);
    let isolated = create_partition(&mut cluster, &minority_nodes)
        .await
        .expect("Failed to create partition");
    
    // Verify majority still has a leader
    let majority_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Majority should maintain leader");
    assert!(majority_nodes.contains(&majority_leader), 
        "Leader {} should be in majority partition", majority_leader);
    
    // Try to write to majority partition - should succeed
    leader_client = cluster.leader_client().await.expect("Should get majority leader client");
    let majority_vm = CreateVmRequest {
        name: "majority-write-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 1,
        memory_mb: 256,
    };
    
    let majority_write = leader_client
        .create_vm(majority_vm)
        .await
        .expect("Should connect to majority");
    assert!(majority_write.into_inner().success, "Majority should accept writes");
    
    // Verify minority nodes cannot make progress (no split-brain)
    let mut minority_write_attempts = 0;
    let mut minority_write_successes = 0;
    
    for (node_id, isolated_node) in &isolated {
        match isolated_node.client().await {
            Ok(mut client) => {
                // Check if isolated node thinks it has a leader
                let status_result = client.get_cluster_status(ClusterStatusRequest {}).await;
                match status_result {
                    Ok(response) => {
                        let status = response.into_inner();
                        info!("Isolated node {} sees leader: {}, term: {}", 
                            node_id, status.leader_id, status.term);
                        
                        // In proper Raft, isolated nodes should step down
                        // For this test, we verify they can't make progress
                        if status.leader_id == *node_id {
                            warn!("Isolated node {} thinks it's leader - testing write rejection", node_id);
                        }
                    }
                    Err(e) => {
                        info!("Isolated node {} has no status: {}", node_id, e);
                    }
                }
                
                // Try to write - should fail
                let minority_vm = CreateVmRequest {
                    name: format!("minority-write-vm-{}", node_id),
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 128,
                };
                
                minority_write_attempts += 1;
                match client.create_vm(minority_vm).await {
                    Ok(response) => {
                        if response.into_inner().success {
                            minority_write_successes += 1;
                            panic!("SPLIT-BRAIN: Isolated node {} accepted write!", node_id);
                        }
                    }
                    Err(e) => {
                        info!("Isolated node {} correctly rejected write: {}", node_id, e);
                    }
                }
            }
            Err(e) => {
                info!("Cannot reach isolated node {}: {}", node_id, e);
            }
        }
    }
    
    assert_eq!(minority_write_successes, 0, 
        "No minority node should accept writes (prevents split-brain)");
    assert!(minority_write_attempts > 0, 
        "Should have attempted writes to minority nodes");
    
    // Verify majority partition has exactly 2 VMs (initial + new)
    let majority_vm_count = {
        let mut count = 0;
        for node_id in &majority_nodes {
            if let Ok(client) = cluster.client(*node_id).await {
                if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                    count = response.into_inner().vms.len();
                    break;
                }
            }
        }
        count
    };
    
    assert_eq!(majority_vm_count, 2, "Majority should have 2 VMs (initial + new write)");
    
    info!("Split-brain prevention verified - minority nodes rejected all writes");
    
    // Clean up
    for (_, node) in isolated {
        node.shutdown().await;
    }
    cluster.shutdown().await;
}

/// Test data reconciliation after partition healing
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_partition_healing_reconciliation() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Get initial leader
    let initial_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Should have initial leader");
    info!("Initial leader: {} ", initial_leader);
    
    // Create initial state
    let mut leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    let initial_vm_count = 5;
    
    // Create VMs before partition
    for i in 0..initial_vm_count {
        let vm_name = format!("reconcile-vm-{}", i);
        let create_request = CreateVmRequest {
            name: vm_name,
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        };
        
        let response = leader_client
            .create_vm(create_request)
            .await
            .expect("Failed to create VM");
        assert!(response.into_inner().success, "VM {} should be created", i);
    }
    
    // Wait for all nodes to see initial VMs
    wait_for_condition(
        || async {
            let vm_counts = get_vm_count_all_nodes(&cluster).await;
            vm_counts.len() == 5 && vm_counts.values().all(|&count| count == initial_vm_count)
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("All nodes should see initial VMs");
    
    // Record state before partition
    let pre_partition_vms = get_vm_count_all_nodes(&cluster).await;
    info!("Pre-partition VM counts: {:?}", pre_partition_vms);
    
    // Create partition - isolate nodes 4,5 (minority)
    let minority_nodes = vec![4, 5];
    info!("Creating partition, isolating nodes {:?}", minority_nodes);
    let isolated = create_partition(&mut cluster, &minority_nodes)
        .await
        .expect("Failed to create partition");
    
    // Verify majority can still make progress
    let majority_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Majority should have leader");
    assert!(majority_leader <= 3, "Leader should be in majority partition");
    
    // Create additional VMs in majority partition
    let partition_vm_count = 3;
    leader_client = cluster.leader_client().await.expect("Should get majority leader");
    
    for i in 0..partition_vm_count {
        let vm_name = format!("partition-vm-{}", i);
        let create_request = CreateVmRequest {
            name: vm_name,
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory_mb: 512,
        };
        
        let response = leader_client
            .create_vm(create_request)
            .await
            .expect("Majority should accept writes");
        assert!(response.into_inner().success, "Partition VM {} should be created", i);
    }
    
    // Verify majority nodes see new VMs
    wait_for_condition(
        || async {
            let expected_count = initial_vm_count + partition_vm_count;
            for node_id in 1..=3 {
                if let Ok(client) = cluster.client(node_id).await {
                    if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                        if response.into_inner().vms.len() != expected_count {
                            return false;
                        }
                    }
                }
            }
            true
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("Majority nodes should see new VMs");
    
    // Verify isolated nodes still have old state
    for (node_id, isolated_node) in &isolated {
        if let Ok(mut client) = isolated_node.client().await {
            if let Ok(response) = client.list_vms(ListVmsRequest {}).await {
                let vm_count = response.into_inner().vms.len();
                assert_eq!(vm_count, initial_vm_count,
                    "Isolated node {} should still have old VM count", node_id);
            }
        }
    }
    
    info!("Healing partition - re-adding isolated nodes");
    
    // Heal partition
    heal_partition(&mut cluster, isolated).await.expect("Failed to heal partition");
    
    // Wait for all nodes to converge to same state
    let final_expected_count = initial_vm_count + partition_vm_count;
    wait_for_condition(
        || async {
            let vm_counts = get_vm_count_all_nodes(&cluster).await;
            if vm_counts.len() != 5 {
                return false;
            }
            
            // All nodes should see the same count
            let all_same = vm_counts.values().all(|&count| count == final_expected_count);
            if !all_same {
                info!("Current VM counts during reconciliation: {:?}", vm_counts);
            }
            all_same
        },
        Duration::from_secs(20), // Give more time for reconciliation
        Duration::from_millis(500),
    )
    .await
    .expect("All nodes should reconcile to same VM count after healing");
    
    // Final verification
    let final_vm_counts = get_vm_count_all_nodes(&cluster).await;
    info!("Final VM counts after reconciliation: {:?}", final_vm_counts);
    
    assert_eq!(final_vm_counts.len(), 5, "Should get counts from all 5 nodes");
    for (node_id, count) in final_vm_counts {
        assert_eq!(count, final_expected_count,
            "Node {} should have {} VMs after reconciliation", node_id, final_expected_count);
    }
    
    // Verify all nodes agree on leader
    let final_agreement = verify_cluster_agreement(&cluster).await;
    assert_eq!(final_agreement, 5, "All nodes should agree on leader after healing");
    
    cluster.shutdown().await;
}

/// Test leader election behavior during partition
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_partition_leader_election() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let initial_leader = wait_for_leader(&cluster, None, Duration::from_secs(5))
        .await
        .expect("Should have initial leader");
    info!("Initial leader: {}", initial_leader);
    
    // Determine partition groups to ensure leader is in minority
    let (minority_with_leader, majority_without_leader) = if initial_leader <= 2 {
        // Leader is node 1 or 2, put them in minority
        (vec![1, 2], vec![3, 4, 5])
    } else {
        // Leader is node 3, 4, or 5, isolate with one other node
        if initial_leader == 3 {
            (vec![3, 4], vec![1, 2, 5])
        } else {
            (vec![4, 5], vec![1, 2, 3])
        }
    };
    
    info!("Creating partition - leader {} will be in minority {:?}", 
        initial_leader, minority_with_leader);
    
    // Create some data before partition
    let mut leader_client = cluster.leader_client().await.expect("Get leader client");
    let pre_partition_vm = CreateVmRequest {
        name: "pre-partition-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 1,
        memory_mb: 256,
    };
    let response = leader_client.create_vm(pre_partition_vm).await.expect("Create VM");
    assert!(response.into_inner().success, "Should create VM before partition");
    
    // Create partition with leader in minority
    let isolated = create_partition(&mut cluster, &minority_with_leader)
        .await
        .expect("Failed to create partition");
    
    // Verify minority partition (with old leader) cannot operate
    for (node_id, isolated_node) in &isolated {
        if *node_id == initial_leader {
            match isolated_node.client().await {
                Ok(mut client) => {
                    // Old leader should step down when it loses quorum
                    match timeout(
                        Duration::from_secs(2),
                        client.get_cluster_status(ClusterStatusRequest {})
                    ).await {
                        Ok(Ok(response)) => {
                            let status = response.into_inner();
                            info!("Isolated former leader {} status: leader_id={}, term={}", 
                                node_id, status.leader_id, status.term);
                            // Should either have no leader or not be leader anymore
                            if status.leader_id == *node_id {
                                warn!("Isolated leader {} still thinks it's leader - checking write capability", node_id);
                            }
                        }
                        _ => {
                            info!("Isolated leader {} not responding to status requests", node_id);
                        }
                    }
                    
                    // Verify it cannot accept writes
                    let test_vm = CreateVmRequest {
                        name: "isolated-leader-vm".to_string(),
                        config_path: "/tmp/test.nix".to_string(),
                        vcpus: 1,
                        memory_mb: 128,
                    };
                    
                    let write_result = client.create_vm(test_vm).await;
                    assert!(write_result.is_err() || !write_result.unwrap().into_inner().success,
                        "Isolated former leader should not accept writes");
                }
                Err(e) => {
                    info!("Cannot reach isolated leader {}: {}", node_id, e);
                }
            }
        }
    }
    
    // Verify majority partition elects new leader
    let new_leader = wait_for_leader(&cluster, None, Duration::from_secs(10))
        .await
        .expect("Majority should elect new leader");
    
    assert!(majority_without_leader.contains(&new_leader),
        "New leader {} should be from majority partition", new_leader);
    assert_ne!(new_leader, initial_leader,
        "New leader should be different from initial leader");
    
    info!("Majority elected new leader: {} (was {})", new_leader, initial_leader);
    
    // Verify new leader accepts writes
    leader_client = cluster.leader_client().await.expect("Get new leader client");
    let new_leader_vm = CreateVmRequest {
        name: "new-leader-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory_mb: 512,
    };
    
    let response = leader_client.create_vm(new_leader_vm).await
        .expect("New leader should accept request");
    assert!(response.into_inner().success, "New leader should accept writes");
    
    // Heal partition
    info!("Healing partition");
    heal_partition(&mut cluster, isolated).await.expect("Failed to heal partition");
    
    // Verify single leader emerges after healing
    let post_heal_leader = wait_for_leader(&cluster, None, Duration::from_secs(10))
        .await
        .expect("Should have leader after healing");
    
    // Verify all nodes agree on the leader
    let agreement = verify_cluster_agreement(&cluster).await;
    assert_eq!(agreement, 5, "All 5 nodes should agree on leader after healing");
    
    // The final leader might be the new leader or a re-elected one
    info!("Final leader after healing: {} (initial: {}, during partition: {})",
        post_heal_leader, initial_leader, new_leader);
    
    // Verify final state consistency
    let final_vm_counts = get_vm_count_all_nodes(&cluster).await;
    let expected_vms = 2; // pre-partition + new-leader VMs
    
    assert_eq!(final_vm_counts.len(), 5, "Should get counts from all nodes");
    for (node_id, count) in final_vm_counts {
        assert_eq!(count, expected_vms,
            "Node {} should see {} VMs after healing", node_id, expected_vms);
    }
    
    cluster.shutdown().await;
}

// Helper functions

/// Get the current leader of the cluster
async fn get_cluster_leader(cluster: &TestCluster) -> Option<u64> {
    for (node_id, _) in cluster.nodes() {
        if let Ok(client) = cluster.client(*node_id).await {
            if let Ok(response) = client.clone().get_cluster_status(ClusterStatusRequest {}).await {
                let status = response.into_inner();
                if status.leader_id > 0 {
                    return Some(status.leader_id);
                }
            }
        }
    }
    None
}

/// Get VM count from all nodes
async fn get_vm_count_all_nodes(cluster: &TestCluster) -> HashMap<u64, usize> {
    let mut counts = HashMap::new();
    
    for (node_id, _) in cluster.nodes() {
        if let Ok(client) = cluster.client(*node_id).await {
            if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                counts.insert(*node_id, response.into_inner().vms.len());
            } else {
                counts.insert(*node_id, 0);
            }
        } else {
            counts.insert(*node_id, 0);
        }
    }
    
    counts
}

/// Verify that all nodes in the cluster agree on the leader
async fn verify_cluster_agreement(cluster: &TestCluster) -> usize {
    let mut leaders = HashSet::new();
    let mut responding_nodes = 0;
    
    for (node_id, _) in cluster.nodes() {
        if let Ok(client) = cluster.client(*node_id).await {
            if let Ok(response) = client.clone().get_cluster_status(ClusterStatusRequest {}).await {
                let status = response.into_inner();
                leaders.insert(status.leader_id);
                responding_nodes += 1;
            }
        }
    }
    
    // All nodes should agree on the same leader
    if leaders.len() == 1 && !leaders.contains(&0) {
        responding_nodes
    } else {
        0
    }
}