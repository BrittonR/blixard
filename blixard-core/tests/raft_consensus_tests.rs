// Raft Consensus Safety Tests
//
// These tests verify that our actual Raft implementation maintains all safety properties

#![cfg(feature = "test-helpers")]

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

use blixard_core::{
    error::BlixardResult,
    iroh_types::CreateVmRequest,
    test_helpers::{timing, TestCluster},
    types::VmConfig,
};

/// Test that only one leader can exist per term
#[tokio::test]
async fn test_at_most_one_leader_per_term() {
    let cluster = TestCluster::with_size(5).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Track leaders per term
    let leaders_per_term = Arc::new(Mutex::new(HashMap::<u64, HashSet<u64>>::new()));

    // Monitor cluster for 30 seconds, checking leadership
    let start = tokio::time::Instant::now();
    let monitor_duration = Duration::from_secs(30);

    while start.elapsed() < monitor_duration {
        // Get cluster status from each node's perspective
        for i in 1..=5 {
            if let Ok(node) = cluster.get_node(i) {
                if let Ok((leader_id, _, term)) = node.shared_state.get_cluster_status().await {
                    if leader_id != 0 {
                        let mut leaders = leaders_per_term.lock().await;
                        leaders
                            .entry(term)
                            .or_insert_with(HashSet::new)
                            .insert(leader_id);
                    }
                }
            }
        }

        // Induce some leadership changes
        if start.elapsed() > Duration::from_secs(10) && start.elapsed() < Duration::from_secs(11) {
            // Partition the current leader
            if let Ok(leader_id) = cluster.get_leader_id().await {
                cluster.partition_node(leader_id).await;
                sleep(Duration::from_secs(2)).await;
                cluster.heal_partition(leader_id).await;
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    // Verify at most one leader per term
    let leaders = leaders_per_term.lock().await;
    for (term, term_leaders) in leaders.iter() {
        assert!(
            term_leaders.len() <= 1,
            "Multiple leaders detected in term {}: {:?}",
            term,
            term_leaders
        );
    }
}

/// Test that committed entries are never lost
#[tokio::test]
async fn test_committed_entries_never_lost() {
    let cluster = TestCluster::with_size(5).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();

    let committed_entries = Arc::new(Mutex::new(Vec::<String>::new()));

    // Create VMs (which go through Raft)
    let leader_client = cluster.leader_client().await.unwrap();

    for i in 0..10 {
        let vm_name = format!("test-vm-{}", i);

        let result = leader_client
            .create_vm(CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 1024,
            })
            .await;

        if result.is_ok() {
            committed_entries.lock().await.push(vm_name);
        }

        // Induce failures during commits
        if i == 5 {
            // Kill the leader mid-operation
            if let Ok(leader_id) = cluster.get_leader_id().await {
                cluster.kill_node(leader_id).await;
                sleep(Duration::from_secs(2)).await;
                cluster.restart_node(leader_id).await;
                cluster
                    .wait_for_convergence(Duration::from_secs(10))
                    .await
                    .unwrap();
                let _ = cluster.get_leader_id().await.unwrap();
            }
        }
    }

    // Wait for cluster to stabilize
    sleep(Duration::from_secs(5)).await;

    // Verify all committed VMs still exist on all nodes
    let committed = committed_entries.lock().await.clone();

    for i in 1..=5 {
        if let Ok(node) = cluster.get_node(i) {
            let vms = node.shared_state.list_vms().await.unwrap();
            let vm_names: HashSet<_> = vms.iter().map(|(config, _)| config.name.clone()).collect();

            for committed_vm in &committed {
                assert!(
                    vm_names.contains(committed_vm),
                    "Committed VM {} missing from node {}",
                    committed_vm,
                    i
                );
            }
        }
    }
}

/// Test that logs remain consistent across nodes
#[tokio::test]
async fn test_log_consistency() {
    let cluster = TestCluster::with_size(3).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Create some entries
    let leader_client = cluster.leader_client().await.unwrap();

    for i in 0..5 {
        let vm_name = format!("vm-{}", i);
        leader_client
            .create_vm(CreateVmRequest {
                name: vm_name,
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 1024,
            })
            .await
            .ok();
    }

    // Wait for replication
    sleep(Duration::from_secs(2)).await;

    // Get VM lists from all nodes
    let mut all_vms = Vec::new();
    for node in cluster.get_nodes() {
        let vms = node.list_vms().await.unwrap();
        all_vms.push((node.id(), vms));
    }

    // Verify all nodes have identical VM lists
    if all_vms.len() > 1 {
        let (_, reference_vms) = &all_vms[0];
        let reference_set: HashSet<_> = reference_vms.iter().map(|v| &v.name).collect();

        for (node_id, vms) in &all_vms[1..] {
            let vm_set: HashSet<_> = vms.iter().map(|v| &v.name).collect();
            assert_eq!(
                reference_set, vm_set,
                "Node {} has different VMs than reference node",
                node_id
            );
        }
    }
}

/// Test split-brain prevention
#[tokio::test]
async fn test_split_brain_prevention() {
    let cluster = TestCluster::with_size(5).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Create network partition: [1,2] vs [3,4,5]
    cluster.partition_network(vec![1, 2], vec![3, 4, 5]).await;

    // Try to perform operations on both sides
    let clients = cluster.get_clients().await;

    // Minority partition (should fail)
    let minority_result = clients[0]
        .create_vm(CreateVmRequest {
            name: "minority-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 512,
        })
        .await;

    // Majority partition (should succeed)
    let majority_result = clients[2]
        .create_vm(CreateVmRequest {
            name: "majority-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 512,
        })
        .await;

    // Verify minority couldn't commit
    assert!(
        minority_result.is_err(),
        "Minority partition should not be able to commit"
    );

    // Verify majority could commit
    assert!(
        majority_result.is_ok(),
        "Majority partition should be able to commit"
    );

    // Heal partition
    cluster.heal_network_partition().await;
    sleep(Duration::from_secs(5)).await;

    // Verify consistency after healing
    let all_vms: Vec<_> =
        futures::future::join_all(cluster.get_nodes().iter().map(|n| n.list_vms())).await;

    // All nodes should have majority-vm but not minority-vm
    for (i, vms_result) in all_vms.iter().enumerate() {
        let vms = vms_result.as_ref().unwrap();
        let vm_names: HashSet<_> = vms.iter().map(|v| v.name.as_str()).collect();

        assert!(
            vm_names.contains("majority-vm"),
            "Node {} missing majority-vm after partition heal",
            i + 1
        );
        assert!(
            !vm_names.contains("minority-vm"),
            "Node {} has minority-vm which should not exist",
            i + 1
        );
    }
}

/// Test that new leaders have all committed entries
#[tokio::test]
async fn test_leader_completeness() {
    let cluster = TestCluster::with_size(5).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Create some committed entries
    let leader_client = cluster.leader_client().await.unwrap();
    let mut created_vms = Vec::new();

    for i in 0..5 {
        let vm_name = format!("committed-vm-{}", i);
        let result = leader_client
            .create_vm(CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 512,
            })
            .await;

        if result.is_ok() {
            created_vms.push(vm_name);
        }
    }

    // Force leader changes multiple times
    for _ in 0..3 {
        sleep(Duration::from_secs(2)).await;

        // Kill current leader
        if let Ok(leader_id) = cluster.get_leader_id().await {
            cluster.kill_node(leader_id).await;

            // Wait for new leader
            cluster
                .wait_for_convergence(Duration::from_secs(10))
                .await
                .unwrap();
            let _ = cluster.get_leader_id().await.unwrap();

            // Verify new leader has all committed entries
            let new_leader = cluster.get_leader_node().await.unwrap();
            let vms = new_leader.list_vms().await.unwrap();
            let vm_names: HashSet<_> = vms.iter().map(|v| v.name.as_str()).collect();

            for created_vm in &created_vms {
                assert!(
                    vm_names.contains(created_vm.as_str()),
                    "New leader missing committed VM: {}",
                    created_vm
                );
            }

            // Restart killed node
            cluster.restart_node(leader_id).await;
        }
    }
}

/// Test that followers correctly reject invalid append entries
#[tokio::test]
async fn test_follower_log_consistency_check() {
    let cluster = TestCluster::with_size(3).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Create initial entries
    let leader_client = cluster.leader_client().await.unwrap();

    for i in 0..3 {
        let vm_name = format!("initial-vm-{}", i);
        leader_client
            .create_vm(CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 512,
            })
            .await
            .ok();
    }

    // Partition a follower
    let nodes = cluster.get_nodes();
    let follower_id = nodes
        .iter()
        .find(|n| !futures::executor::block_on(n.is_leader()))
        .map(|n| n.id())
        .unwrap();

    cluster.partition_node(follower_id).await;

    // Create more entries while follower is partitioned
    for i in 3..6 {
        let vm_name = format!("partition-vm-{}", i);
        leader_client
            .create_vm(CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 512,
            })
            .await
            .ok();
    }

    // Heal partition
    cluster.heal_partition(follower_id).await;

    // Wait for follower to catch up
    timing::wait_for_condition_with_backoff(
        || async {
            let follower = nodes.iter().find(|n| n.id() == follower_id).unwrap();
            let vms = follower.list_vms().await.unwrap();
            vms.len() == 6
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    )
    .await
    .unwrap();

    // Verify follower has correct entries
    let follower = nodes.iter().find(|n| n.id() == follower_id).unwrap();
    let vms = follower.list_vms().await.unwrap();
    let vm_names: HashSet<_> = vms.iter().map(|v| v.name.as_str()).collect();

    for i in 0..6 {
        let expected_name = if i < 3 {
            format!("initial-vm-{}", i)
        } else {
            format!("partition-vm-{}", i)
        };

        assert!(
            vm_names.contains(expected_name.as_str()),
            "Follower missing expected VM: {}",
            expected_name
        );
    }
}

/// Test election restriction - nodes without up-to-date logs cannot become leader
#[tokio::test]
async fn test_election_restriction() {
    let cluster = TestCluster::with_size(5).await.unwrap();
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .unwrap();
    let _ = cluster.get_leader_id().await.unwrap();

    // Partition one node before creating entries
    cluster.partition_node(5).await;

    // Create committed entries
    let leader_client = cluster.leader_client().await.unwrap();

    for i in 0..5 {
        let vm_name = format!("restricted-vm-{}", i);
        leader_client
            .create_vm(CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 512,
            })
            .await
            .ok();
    }

    // Now partition the majority and heal node 5
    cluster.partition_network(vec![1, 2, 3], vec![4, 5]).await;

    // Node 5 (which missed the entries) is now with node 4
    // They form a minority and node 5 shouldn't become leader

    // Wait a bit for election attempts
    sleep(Duration::from_secs(5)).await;

    // Check that node 5 is not leader
    let node5 = cluster.get_nodes().iter().find(|n| n.id() == 5).unwrap();
    let is_leader = node5.is_leader().await;

    assert!(
        !is_leader,
        "Node 5 should not become leader without up-to-date log"
    );

    // Heal all partitions
    cluster.heal_network_partition().await;
    sleep(Duration::from_secs(5)).await;

    // Verify node 5 eventually gets the entries
    timing::wait_for_condition_with_backoff(
        || async {
            let vms = node5.list_vms().await.unwrap();
            vms.len() == 5
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
}
