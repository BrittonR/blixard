#![cfg(feature = "test-helpers")]

use blixard_core::{
    test_helpers::TestCluster,
    types::{VmCommand, VmConfig, VmStatus},
    vm_scheduler::PlacementStrategy,
};
use std::time::Duration;
use tokio::time::sleep;

mod common;

/// Test basic VM lifecycle through distributed consensus
#[tokio::test]
async fn test_distributed_vm_lifecycle() {
    // Create a 3-node cluster
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");

    // Wait for cluster to stabilize
    sleep(Duration::from_secs(2)).await;

    // Get the first node (which should be leader in a fresh cluster)
    let node = cluster.get_node(1).expect("Failed to get node 1");

    // Create a VM using Raft consensus
    let mut vm_config = common::test_vm_config("test-vm-1");
    vm_config.vcpus = 2;
    vm_config.memory = 1024;

    let command = VmCommand::Create {
        config: vm_config.clone(),
        node_id: node.shared_state.get_id(),
    };

    node.shared_state
        .create_vm_through_raft(command)
        .await
        .expect("Failed to create VM");

    // Wait a bit for VM to be created
    sleep(Duration::from_secs(1)).await;

    // Start the VM
    let command = VmCommand::Start {
        name: vm_config.name.clone(),
    };

    node.shared_state
        .send_vm_operation_through_raft(command)
        .await
        .expect("Failed to start VM");

    // Wait for VM to start
    sleep(Duration::from_secs(1)).await;

    // Check VM status through different nodes
    for node_id in 1..=3 {
        let test_node = cluster
            .get_node(node_id)
            .unwrap_or_else(|_| panic!("Failed to get node {}", node_id));
        let vm_manager = test_node
            .shared_state
            .get_vm_manager()
            .await
            .expect("VM manager not initialized");

        let status = vm_manager
            .get_vm_status(&vm_config.name)
            .await
            .expect("Failed to get VM status");

        assert!(status.is_some(), "VM should exist on node {}", node_id);
        let (config, vm_status) = status.unwrap();
        assert_eq!(
            config.name, vm_config.name,
            "VM name should match on node {}",
            node_id
        );

        // The VM should be running or at least starting
        assert!(
            vm_status == VmStatus::Starting || vm_status == VmStatus::Running,
            "VM should be starting or running on node {}, but was {:?}",
            node_id,
            vm_status
        );
    }

    // Stop the VM
    let command = VmCommand::Stop {
        name: vm_config.name.clone(),
    };

    node.shared_state
        .send_vm_operation_through_raft(command)
        .await
        .expect("Failed to stop VM");

    cluster.shutdown().await;
}

/// Test VM scheduling and placement across nodes
#[tokio::test]
async fn test_distributed_vm_scheduling() {
    // Create a 3-node cluster
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");

    // Wait for cluster to stabilize
    sleep(Duration::from_secs(2)).await;

    let node = cluster.get_node(1).expect("Failed to get node 1");
    let vm_manager = node
        .shared_state
        .get_vm_manager()
        .await
        .expect("VM manager not initialized");

    // Create VMs with automatic scheduling
    let mut placement_decisions = Vec::new();

    for i in 1..=5 {
        let mut vm_config = common::test_vm_config(&format!("scheduled-vm-{}", i));
        vm_config.vcpus = 1;
        vm_config.memory = 512;

        // Use the scheduler to determine placement
        let placement = vm_manager
            .create_vm_with_scheduling(vm_config, PlacementStrategy::RoundRobin)
            .await
            .expect("Failed to schedule VM");

        println!(
            "VM {} scheduled on node {}: {}",
            i, placement.selected_node_id, placement.reason
        );

        placement_decisions.push(placement.selected_node_id);
    }

    // Verify VMs are distributed across nodes
    // Count VMs per node
    let mut vms_per_node = std::collections::HashMap::new();
    for node_id in placement_decisions {
        *vms_per_node.entry(node_id).or_insert(0) += 1;
    }

    // With round-robin and 5 VMs across 3 nodes, distribution should be 2-2-1
    assert_eq!(
        vms_per_node.len(),
        3,
        "VMs should be distributed across all 3 nodes"
    );
    for (node_id, count) in &vms_per_node {
        println!("Node {} has {} VMs", node_id, count);
        assert!(
            *count >= 1 && *count <= 2,
            "Each node should have 1-2 VMs with round-robin"
        );
    }

    cluster.shutdown().await;
}

/// Test concurrent VM operations
#[tokio::test]
async fn test_concurrent_vm_operations() {
    // Create a 3-node cluster
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");

    // Wait for cluster to stabilize
    sleep(Duration::from_secs(2)).await;

    // Create VMs concurrently from different nodes
    let mut handles = Vec::new();

    for i in 1..=3 {
        let node = cluster
            .get_node(i)
            .unwrap_or_else(|_| panic!("Failed to get node {}", i));
        let shared_state = node.shared_state.clone();

        let handle = tokio::spawn(async move {
            let mut vm_config = common::test_vm_config(&format!("concurrent-vm-{}", i));
            vm_config.vcpus = 1;
            vm_config.memory = 512;

            let command = VmCommand::Create {
                config: vm_config,
                node_id: i as u64,
            };

            shared_state.create_vm_through_raft(command).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut successes = 0;
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => successes += 1,
            Ok(Err(e)) => println!("VM creation failed: {:?}", e),
            Err(e) => println!("Task failed: {:?}", e),
        }
    }

    assert_eq!(successes, 3, "All concurrent VM creations should succeed");

    // Verify VMs were created
    let node = cluster.get_node(1).expect("Failed to get node 1");
    let vm_manager = node
        .shared_state
        .get_vm_manager()
        .await
        .expect("VM manager not initialized");

    // Check that each VM exists
    for i in 1..=3 {
        let vm_name = format!("concurrent-vm-{}", i);
        let status = vm_manager
            .get_vm_status(&vm_name)
            .await
            .expect("Failed to get VM status");
        assert!(status.is_some(), "VM {} should exist", i);
    }

    cluster.shutdown().await;
}
