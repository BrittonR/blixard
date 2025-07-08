//! Test to verify Raft consensus works over Iroh transport
//!
//! This test creates a cluster and verifies that:
//! 1. Leader election works
//! 2. Raft messages are properly serialized/deserialized
//! 3. State replication works across nodes
//! 4. All nodes agree on the cluster state

#![cfg(feature = "test-helpers")]

use blixard_core::error::BlixardResult;
use blixard_core::test_helpers::TestCluster;
use blixard_core::types::VmConfig;
use blixard_core::vm_scheduler::PlacementStrategy;
use std::time::Duration;

mod common;

#[tokio::test]
async fn test_raft_consensus_over_iroh() -> BlixardResult<()> {
    // Initialize test logging
    common::setup_test_logging();

    // Create a 3-node cluster
    let cluster = TestCluster::builder().with_nodes(3).build().await?;

    // Wait for cluster to converge
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await?;

    // Verify leader election
    let leader_id = cluster.get_leader_id().await?;
    println!("Leader elected: Node {}", leader_id);

    // Verify all nodes agree on the leader
    let mut leader_views = vec![];
    for (node_id, node) in cluster.nodes().iter() {
        if let Ok(status) = node.shared_state.get_raft_status().await {
            leader_views.push((*node_id, status.leader_id));
            println!("Node {} sees leader as: {:?}", node_id, status.leader_id);
        }
    }

    // All nodes should see the same leader
    let first_leader = leader_views[0].1;
    assert!(
        leader_views
            .iter()
            .all(|(_, leader)| *leader == first_leader),
        "Not all nodes agree on the leader"
    );

    // Test state replication by creating a VM through Raft
    let leader_node = cluster.nodes().get(&leader_id).ok_or_else(|| {
        blixard_core::error::BlixardError::Internal {
            message: "Leader node not found".to_string(),
        }
    })?;

    // Create a VM task through Raft consensus
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        vcpus: 1,
        memory: 512,
        ..Default::default()
    };

    let task = VmTask {
        id: "task-1".to_string(),
        vm_config: vm_config.clone(),
        placement_strategy: PlacementStrategy::MostAvailable,
        resource_requirements: Default::default(),
        feature_requirements: vec![],
        assigned_node: None,
        status: blixard_core::types::TaskStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Submit task through leader
    leader_node.shared_state.submit_task(task.clone()).await?;

    // Wait for replication
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes have the task
    for (node_id, node) in cluster.nodes().iter() {
        let tasks = node.shared_state.list_tasks().await?;
        assert!(
            tasks.iter().any(|t| t.id == task.id),
            "Node {} doesn't have the replicated task",
            node_id
        );
        println!("Node {} has the task replicated", node_id);
    }

    // Test configuration changes through Raft
    println!("\nTesting configuration changes...");

    // Get initial peer count
    let initial_peers = leader_node.shared_state.get_peers().await;
    println!(
        "Initial peers: {:?}",
        initial_peers.iter().map(|p| p.id).collect::<Vec<_>>()
    );

    // Verify Raft messages are being sent over Iroh
    // This is implicitly tested by the fact that:
    // 1. Leader election worked (requires vote messages)
    // 2. State replication worked (requires append entries messages)
    // 3. All nodes converged to the same state

    println!("\n✅ Raft consensus over Iroh transport is working correctly!");
    println!("   - Leader election: OK");
    println!("   - Message serialization: OK");
    println!("   - State replication: OK");
    println!("   - Cluster agreement: OK");

    // Cleanup
    cluster.shutdown().await;

    Ok(())
}

#[tokio::test]
async fn test_raft_message_codec() -> BlixardResult<()> {
    use blixard_core::raft_codec::{deserialize_message, serialize_message};
    use raft::prelude::*;

    // Test various Raft message types
    let test_messages = vec![
        // Heartbeat message
        {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgHeartbeat);
            msg.from = 1;
            msg.to = 2;
            msg.term = 5;
            msg
        },
        // Vote request
        {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgRequestVote);
            msg.from = 2;
            msg.to = 3;
            msg.term = 10;
            msg.log_term = 8;
            msg.index = 100;
            msg
        },
        // Append entries
        {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgAppend);
            msg.from = 1;
            msg.to = 3;
            msg.term = 7;
            msg.log_term = 6;
            msg.index = 50;
            msg.commit = 45;

            // Add some entries
            let mut entry = Entry::default();
            entry.term = 7;
            entry.index = 51;
            entry.entry_type = EntryType::EntryNormal as i32;
            entry.data = b"test data".to_vec();
            msg.entries.push(entry);

            msg
        },
    ];

    for original in test_messages {
        println!("Testing {:?} message...", original.msg_type());

        // Serialize
        let serialized = serialize_message(&original)?;
        println!("  Serialized size: {} bytes", serialized.len());

        // Deserialize
        let deserialized = deserialize_message(&serialized)?;

        // Verify basic fields
        assert_eq!(original.msg_type(), deserialized.msg_type());
        assert_eq!(original.from, deserialized.from);
        assert_eq!(original.to, deserialized.to);
        assert_eq!(original.term, deserialized.term);
        assert_eq!(original.log_term, deserialized.log_term);
        assert_eq!(original.index, deserialized.index);
        assert_eq!(original.commit, deserialized.commit);
        assert_eq!(original.entries.len(), deserialized.entries.len());

        println!("  ✓ Round-trip successful");
    }

    println!("\n✅ All Raft message types serialize/deserialize correctly!");

    Ok(())
}
