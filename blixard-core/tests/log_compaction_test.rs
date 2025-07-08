//! Tests for Raft log compaction functionality
//!
//! Verifies that:
//! - Log compaction triggers after threshold
//! - Old log entries are removed
//! - Snapshots preserve state
//! - Recovery works from compacted logs

#![cfg(feature = "test-helpers")]

use std::time::Duration;

use blixard_core::raft_manager::{ResourceRequirements, TaskSpec};
use blixard_core::storage::RedbRaftStorage;
use blixard_core::test_helpers::{timing, TestCluster};
use redb::ReadableTable;

/// Test basic log compaction after threshold
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_log_compaction_basic() {
    let cluster = TestCluster::new(1).await.expect("Failed to create cluster");

    // Wait for node to be ready
    timing::wait_for_condition_with_backoff(
        || async { cluster.get_node(1).unwrap().shared_state.is_leader().await },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("Node should become leader");

    let node = cluster.get_node(1).unwrap();

    // Submit many tasks to trigger compaction (threshold is 1000)
    for i in 0..1200 {
        let task_id = format!("compact-task-{}", i);
        let task_spec = TaskSpec {
            command: "echo".to_string(),
            args: vec![format!("data-{}", i)],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 60,
        };
        node.node
            .submit_task(&task_id, task_spec)
            .await
            .expect("Should submit task");

        // Small batches to avoid overwhelming
        if i % 100 == 99 {
            timing::robust_sleep(Duration::from_millis(100)).await;
        }
    }

    // Wait for all tasks to be applied and compaction to occur
    timing::robust_sleep(Duration::from_secs(10)).await;

    // Check that compaction occurred by looking at log size
    let db = node
        .shared_state
        .get_database()
        .await
        .expect("Should have database");
    let storage = RedbRaftStorage {
        database: db.clone(),
    };
    let log_entries = count_log_entries(&storage).await;

    // After compaction, we should have fewer than the total entries
    // The exact number depends on compaction timing
    println!("Log entries after compaction: {}", log_entries);
    assert!(
        log_entries < 1200,
        "Log should be compacted, but has {} entries",
        log_entries
    );
}

/// Test recovery after log compaction
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_recovery_after_compaction() {
    let cluster = TestCluster::new(1).await.expect("Failed to create cluster");

    timing::wait_for_condition_with_backoff(
        || async { cluster.get_node(1).unwrap().shared_state.is_leader().await },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("Node should become leader");

    // Submit tasks to trigger compaction
    for i in 0..1500 {
        let task_id = format!("recovery-task-{}", i);
        let task_spec = TaskSpec {
            command: "echo".to_string(),
            args: vec![format!("data-{}", i)],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 60,
        };
        cluster
            .get_node(1)
            .unwrap()
            .node
            .submit_task(&task_id, task_spec)
            .await
            .expect("Should submit task");

        if i % 100 == 99 {
            timing::robust_sleep(Duration::from_millis(100)).await;
        }
    }

    // Wait for compaction
    timing::robust_sleep(Duration::from_secs(10)).await;

    // Get some task status before restart
    let sample_task_id = "recovery-task-500";
    let status_before = cluster
        .get_node(1)
        .unwrap()
        .node
        .get_task_status(sample_task_id)
        .await
        .expect("Should get task status");
    assert!(status_before.is_some(), "Task should exist before restart");

    // Shutdown and recreate the node (simulating restart)
    drop(cluster);
    timing::robust_sleep(Duration::from_secs(1)).await;

    // Create new cluster with same data directory
    let new_cluster = TestCluster::new(1).await.expect("Failed to create cluster");

    // Wait for node to recover
    timing::wait_for_condition_with_backoff(
        || async {
            new_cluster
                .get_node(1)
                .unwrap()
                .shared_state
                .is_leader()
                .await
        },
        Duration::from_secs(10),
        Duration::from_millis(200),
    )
    .await
    .expect("Node should recover and become leader");

    // Verify some tasks were recovered
    let status_after = new_cluster
        .get_node(1)
        .unwrap()
        .node
        .get_task_status(sample_task_id)
        .await
        .expect("Should get task status");
    assert!(
        status_after.is_some(),
        "Task should be recovered after restart"
    );

    // Submit new tasks to verify functionality
    for i in 1500..1600 {
        let task_id = format!("recovery-task-{}", i);
        let task_spec = TaskSpec {
            command: "echo".to_string(),
            args: vec![format!("new-{}", i)],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 60,
        };
        new_cluster
            .get_node(1)
            .unwrap()
            .node
            .submit_task(&task_id, task_spec)
            .await
            .expect("Should submit new task after recovery");
    }

    timing::robust_sleep(Duration::from_secs(2)).await;

    // Verify new tasks completed
    let new_task_status = new_cluster
        .get_node(1)
        .unwrap()
        .node
        .get_task_status("recovery-task-1550")
        .await
        .expect("Should get new task status");
    assert!(
        new_task_status.is_some(),
        "New tasks should complete after recovery"
    );
}

/// Test compaction in multi-node cluster  
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_compaction_multi_node() {
    let cluster = TestCluster::new(3).await.expect("Failed to create cluster");

    // Wait for leader election
    let mut leader_id = 0;
    for _ in 0..100 {
        // Try for 10 seconds
        for i in 1..=3 {
            if let Ok(node) = cluster.get_node(i) {
                if node.shared_state.is_leader().await {
                    leader_id = i;
                    break;
                }
            }
        }
        if leader_id > 0 {
            break;
        }
        timing::robust_sleep(Duration::from_millis(100)).await;
    }

    assert!(leader_id > 0, "Should have found a leader");

    println!("Leader elected: node {}", leader_id);

    // Submit many tasks through the leader
    let leader_node = cluster.get_node(leader_id).unwrap();
    for i in 0..1100 {
        let task_id = format!("multi-task-{}", i);
        let task_spec = TaskSpec {
            command: "echo".to_string(),
            args: vec![format!("data-{}", i)],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 60,
        };
        leader_node
            .node
            .submit_task(&task_id, task_spec)
            .await
            .expect("Should submit task");

        if i % 50 == 49 {
            timing::robust_sleep(Duration::from_millis(50)).await;
        }
    }

    // Wait for replication and compaction
    timing::robust_sleep(Duration::from_secs(15)).await;

    // Check that all nodes have compacted logs
    for node_id in 1..=3 {
        if let Ok(node) = cluster.get_node(node_id) {
            let db = node
                .shared_state
                .get_database()
                .await
                .expect("Should have database");
            let storage = RedbRaftStorage {
                database: db.clone(),
            };
            let log_entries = count_log_entries(&storage).await;

            println!("Node {} has {} log entries", node_id, log_entries);

            // Each node should have compacted (threshold is 1000)
            assert!(
                log_entries < 1100,
                "Node {} should have compacted log, but has {} entries",
                node_id,
                log_entries
            );
        }
    }

    // Verify data consistency - check a sample of tasks
    for i in (0..1100).step_by(100) {
        let task_id = format!("multi-task-{}", i);

        // Check task exists on all nodes
        for node_id in 1..=3 {
            if let Ok(node) = cluster.get_node(node_id) {
                let status = node.node.get_task_status(&task_id).await;
                assert!(
                    status.is_ok() && status.unwrap().is_some(),
                    "Task {} should exist on node {}",
                    task_id,
                    node_id
                );
            }
        }
    }
}

// Helper function to count log entries
async fn count_log_entries(storage: &RedbRaftStorage) -> usize {
    use blixard_core::storage::RAFT_LOG_TABLE;

    let db = storage.database.clone();
    let read_txn = db.begin_read().unwrap();
    let table = read_txn.open_table(RAFT_LOG_TABLE).unwrap();

    table.iter().unwrap().count()
}
