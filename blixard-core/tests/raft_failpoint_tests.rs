//! Raft failpoint injection tests
//!
//! These tests use failpoints to inject various failures and verify
//! that the Raft implementation handles them correctly.

#![cfg(all(feature = "test-helpers", feature = "failpoints"))]

use std::collections::HashMap;
use std::time::Duration;

use blixard_core::{
    error::BlixardError,
    fail_point, fail_point_action,
    failpoints::{self, scenarios},
    test_helpers::{timing, TestCluster},
    types::{CreateVmRequest, VmConfig},
};

/// Test Raft leader election under network failures
#[tokio::test]
async fn test_leader_election_with_network_failures() {
    failpoints::init();

    let cluster = TestCluster::with_size(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let initial_leader = cluster.get_leader_id().await.unwrap();

    // Configure failpoint to drop messages from the leader
    scenarios::fail_with_probability("raft::send_message", 0.5);

    // Kill the leader
    cluster.kill_node(initial_leader).await;

    // Despite 50% message loss, cluster should elect new leader
    let result = timing::wait_for_condition_with_backoff(
        || async {
            match cluster.get_leader_id().await {
                Ok(leader) => leader != initial_leader && leader != 0,
                Err(_) => false,
            }
        },
        Duration::from_secs(30),
        Duration::from_millis(100),
    )
    .await;

    scenarios::disable("raft::send_message");

    assert!(
        result.is_ok(),
        "Failed to elect new leader with network failures"
    );

    // Restart the old leader
    cluster.restart_node(initial_leader).await;

    // Verify cluster converges
    timing::robust_sleep(Duration::from_secs(5)).await;

    let final_leader = cluster.get_leader_id().await.unwrap();
    assert_ne!(final_leader, 0, "Cluster should have a leader");
}

/// Test storage failures during log replication
#[tokio::test]
async fn test_storage_failures_during_replication() {
    failpoints::init();

    let cluster = TestCluster::with_size(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Configure storage to fail occasionally
    scenarios::fail_with_error(
        "storage::append_entries",
        BlixardError::Storage {
            operation: "append_entries".to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Simulated disk failure",
            )),
        },
    );

    scenarios::fail_with_probability("storage::append_entries", 0.2);

    // Try to create VMs
    let leader_client = cluster.get_leader_client().await.unwrap();
    let mut created = 0;
    let mut failed = 0;

    for i in 0..10 {
        let result = leader_client
            .create_vm(CreateVmRequest {
                name: format!("storage-test-vm-{}", i),
                config: VmConfig {
                    name: format!("storage-test-vm-{}", i),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await;

        match result {
            Ok(_) => created += 1,
            Err(_) => failed += 1,
        }
    }

    scenarios::disable("storage::append_entries");

    // Despite storage failures, some operations should succeed
    assert!(
        created > 0,
        "No VMs were created despite only 20% failure rate"
    );
    assert!(
        failed > 0,
        "No failures detected, failpoint may not be working"
    );

    // Verify consistency across nodes
    timing::robust_sleep(Duration::from_secs(3)).await;

    let mut vm_counts = vec![];
    for node in cluster.get_nodes() {
        let vms = node.list_vms().await.unwrap();
        vm_counts.push(vms.len());
    }

    // All nodes should have the same number of VMs
    assert!(
        vm_counts.windows(2).all(|w| w[0] == w[1]),
        "Nodes have inconsistent VM counts: {:?}",
        vm_counts
    );
}

/// Test failpoints in the Raft state machine
#[tokio::test]
async fn test_state_machine_apply_failures() {
    failpoints::init();

    let cluster = TestCluster::with_size(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Fail state machine applies on node 2
    scenarios::fail_after_n("raft::apply_entry::node2", 3);

    let leader_client = cluster.get_leader_client().await.unwrap();

    // Create 5 VMs
    for i in 0..5 {
        leader_client
            .create_vm(CreateVmRequest {
                name: format!("apply-test-vm-{}", i),
                config: VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await
            .ok();
    }

    // Wait for replication
    timing::robust_sleep(Duration::from_secs(3)).await;

    // Node 2 should have missed applying the 4th entry
    let nodes = cluster.get_nodes();
    let node2_vms = nodes[1].list_vms().await.unwrap();

    // This depends on implementation - if apply failures are handled gracefully,
    // the node might recover. If not, it might be behind.
    println!("Node 2 has {} VMs after apply failure", node2_vms.len());

    scenarios::disable_all();
}

/// Test leader step-down scenarios
#[tokio::test]
async fn test_leader_step_down_failpoints() {
    failpoints::init();

    let cluster = TestCluster::with_size(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let leader_id = cluster.get_leader_id().await.unwrap();

    // Configure leader to step down after receiving 5 requests
    scenarios::fail_after_n("raft::propose::leader_check", 5);

    let leader_client = cluster.get_leader_client().await.unwrap();

    // Send requests that will trigger step-down
    for i in 0..7 {
        let result = leader_client
            .create_vm(CreateVmRequest {
                name: format!("step-down-vm-{}", i),
                config: VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await;

        // After 5th request, leader should step down
        if i >= 5 {
            // Expect failures as leader steps down
            if result.is_ok() {
                println!("Request {} succeeded despite expected step-down", i);
            }
        }
    }

    // Wait for new leader election
    timing::robust_sleep(Duration::from_secs(5)).await;

    let new_leader = cluster.get_leader_id().await.unwrap();
    assert_ne!(new_leader, leader_id, "Should have elected a new leader");

    scenarios::disable_all();
}

/// Test snapshot failures
#[tokio::test]
async fn test_snapshot_failures() {
    failpoints::init();

    let cluster = TestCluster::with_size(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Create enough entries to trigger snapshot
    let leader_client = cluster.get_leader_client().await.unwrap();

    for i in 0..20 {
        leader_client
            .create_vm(CreateVmRequest {
                name: format!("snapshot-vm-{}", i),
                config: VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await
            .ok();
    }

    // Configure snapshot creation to fail
    scenarios::fail_with_error(
        "storage::create_snapshot",
        BlixardError::Storage {
            operation: "create_snapshot".to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Snapshot creation failed",
            )),
        },
    );

    // Trigger snapshot by adding a new node
    let new_node = TestCluster::create_node(4).await;
    cluster.add_node(new_node).await.ok();

    // Despite snapshot failures, the node should eventually catch up
    // through normal log replication
    timing::robust_sleep(Duration::from_secs(10)).await;

    scenarios::disable("storage::create_snapshot");

    // Verify new node has the data
    let nodes = cluster.get_nodes();
    let new_node_vms = nodes.last().unwrap().list_vms().await.unwrap();

    assert!(
        new_node_vms.len() >= 15,
        "New node should have caught up despite snapshot failures, has {} VMs",
        new_node_vms.len()
    );
}

/// Test concurrent failpoints affecting multiple components
#[tokio::test]
async fn test_chaos_engineering_scenario() {
    failpoints::init();

    let cluster = TestCluster::with_size(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Enable multiple failpoints to simulate chaos
    let failpoints = vec![
        ("raft::send_message", "10%return"),     // 10% message loss
        ("storage::append_entries", "5%return"), // 5% storage failures
        ("raft::apply_entry", "2%return"),       // 2% apply failures
        ("network::connect", "15%delay(100)"),   // 15% connection delays
    ];

    for (name, config) in &failpoints {
        fail::cfg(name, config).unwrap();
    }

    // Run workload under chaos
    let leader_client = cluster.get_leader_client().await.unwrap();
    let mut successes = 0;
    let mut failures = 0;

    for i in 0..50 {
        let result = leader_client
            .create_vm(CreateVmRequest {
                name: format!("chaos-vm-{}", i),
                config: VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await;

        match result {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }

        // Add some node failures
        if i == 10 {
            cluster.kill_node(2).await;
        }
        if i == 20 {
            cluster.restart_node(2).await;
            cluster.kill_node(3).await;
        }
        if i == 30 {
            cluster.restart_node(3).await;
        }
    }

    // Disable all failpoints
    scenarios::disable_all();

    println!("Chaos test: {} successes, {} failures", successes, failures);

    // System should still make progress under chaos
    assert!(
        successes > 25,
        "Too few successes under chaos: {}",
        successes
    );

    // Wait for stabilization
    timing::robust_sleep(Duration::from_secs(10)).await;

    // Verify eventual consistency
    let mut vm_counts = vec![];
    for node in cluster.get_nodes() {
        if let Ok(vms) = node.list_vms().await {
            vm_counts.push(vms.len());
        }
    }

    // All running nodes should converge to same state
    let running_counts: Vec<_> = vm_counts.into_iter().filter(|&c| c > 0).collect();
    if running_counts.len() > 1 {
        let first = running_counts[0];
        assert!(
            running_counts
                .iter()
                .all(|&c| (c as i32 - first as i32).abs() <= 5),
            "Nodes diverged too much under chaos: {:?}",
            running_counts
        );
    }
}

/// Test recovery after cascading failures
#[tokio::test]
async fn test_cascading_failure_recovery() {
    failpoints::init();

    let cluster = TestCluster::with_size(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Create initial state
    let leader_client = cluster.get_leader_client().await.unwrap();
    for i in 0..5 {
        leader_client
            .create_vm(CreateVmRequest {
                name: format!("initial-vm-{}", i),
                config: VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 1024,
                    vcpus: 1,
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            })
            .await
            .ok();
    }

    // Simulate cascading failures
    scenarios::pause("raft::heartbeat", Duration::from_secs(5));

    // This will cause election storms
    timing::robust_sleep(Duration::from_secs(2)).await;

    // Add storage failures during recovery
    scenarios::fail_with_probability("storage::read_state", 0.3);

    // Wait for things to get chaotic
    timing::robust_sleep(Duration::from_secs(5)).await;

    // Clear all failures
    scenarios::disable_all();

    // System should recover
    cluster
        .wait_for_leader(Duration::from_secs(30))
        .await
        .expect("Cluster failed to recover from cascading failures");

    // Verify data integrity after recovery
    timing::robust_sleep(Duration::from_secs(5)).await;

    for node in cluster.get_nodes() {
        let vms = node.list_vms().await.unwrap();
        assert_eq!(
            vms.len(),
            5,
            "Node {} lost data during cascading failure",
            node.id()
        );
    }
}
