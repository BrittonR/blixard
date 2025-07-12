//! Comprehensive test for three-node cluster formation
//!
//! This test verifies that all the fixes for cluster formation work correctly:
//! - P2P connections are established properly
//! - Raft messages don't fail due to channel issues
//! - Configuration changes propagate P2P info correctly
//! - Worker registration happens after leader identification
//! - All nodes can communicate and reach consensus

#![cfg(feature = "test-helpers")]

use blixard_core::iroh_types::{CreateVmRequest, VmConfig};
use blixard_core::test_helpers::{timing, TestCluster};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::test]
async fn test_three_node_cluster_formation_comprehensive() {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,raft=info")
        .try_init();

    println!("=== Starting comprehensive three-node cluster test ===");

    // Create a three-node cluster
    let mut cluster = TestCluster::new(3)
        .await
        .expect("Failed to create three-node cluster");

    println!("Created cluster with 3 nodes");

    // Wait for cluster to converge (leader election, peer discovery)
    cluster
        .wait_for_convergence(Duration::from_secs(30))
        .await
        .expect("Cluster failed to converge");

    println!("Cluster converged successfully");

    // Verify all nodes see each other
    for node_id in 1..=3 {
        let client = cluster
            .client(node_id)
            .await
            .expect(&format!("Failed to get client for node {}", node_id));

        let (leader_id, nodes, _term) = client.get_cluster_status().await.expect(&format!(
            "Failed to get cluster status from node {}",
            node_id
        ));

        println!(
            "Node {} sees leader: {}, nodes: {:?}",
            node_id,
            leader_id,
            nodes.len()
        );

        // Verify this node sees all 3 nodes
        assert_eq!(
            nodes.len(),
            3,
            "Node {} should see 3 nodes in cluster",
            node_id
        );

        // Verify all nodes have P2P info
        for node_info in &nodes {
            assert!(
                !node_info.p2p_node_id.is_empty(),
                "Node {} should have P2P node ID",
                node_info.id
            );
            assert!(
                !node_info.p2p_addresses.is_empty() || !node_info.p2p_relay_url.is_empty(),
                "Node {} should have P2P addresses or relay URL",
                node_info.id
            );
        }
    }

    println!("All nodes see each other with P2P info ✓");

    // Test that we can perform operations through the leader
    let leader_client = cluster
        .leader_client()
        .await
        .expect("Failed to get leader client");

    // Create a VM to test that operations work
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        memory: 512 * 1024 * 1024, // 512MB
        vcpus: 1,
        disk_size: 1024 * 1024 * 1024, // 1GB
        image: "test-image".to_string(),
        network_interfaces: vec![],
        metadata: Some(HashMap::new()),
    };

    let create_response = leader_client
        .create_vm(vm_config)
        .await
        .expect("Failed to create VM through leader");

    println!(
        "Successfully created VM through leader: {:?}",
        create_response
    );

    // Give some time for replication
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify VM is visible from all nodes
    for node_id in 1..=3 {
        let client = cluster
            .client(node_id)
            .await
            .expect(&format!("Failed to get client for node {}", node_id));

        let vms = client
            .list_vms()
            .await
            .expect(&format!("Failed to list VMs from node {}", node_id));

        assert_eq!(vms.len(), 1, "Node {} should see 1 VM", node_id);
        assert_eq!(vms[0].name, "test-vm", "VM name should match");
    }

    println!("VM replicated to all nodes ✓");

    // Test adding a fourth node to verify join still works
    println!("\n=== Testing dynamic node addition ===");

    let new_node_id = cluster.add_node().await.expect("Failed to add fourth node");

    println!("Added node {} to cluster", new_node_id);

    // Wait for new topology to stabilize
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify new node sees the cluster
    let new_client = cluster
        .client(new_node_id)
        .await
        .expect("Failed to get client for new node");

    let (_leader_id, nodes, _term) = new_client
        .get_cluster_status()
        .await
        .expect("Failed to get cluster status from new node");

    assert_eq!(nodes.len(), 4, "New node should see 4 nodes in cluster");

    // Verify new node has P2P info for all peers
    for node_info in &nodes {
        if node_info.id != new_node_id {
            assert!(
                !node_info.p2p_node_id.is_empty(),
                "New node should have P2P info for node {}",
                node_info.id
            );
        }
    }

    println!("New node successfully joined and has P2P info for all peers ✓");

    // Verify new node sees the VM (tests state replication)
    let vms = new_client
        .list_vms()
        .await
        .expect("Failed to list VMs from new node");

    assert_eq!(vms.len(), 1, "New node should see 1 VM");
    assert_eq!(vms[0].name, "test-vm", "VM name should match on new node");

    println!("New node received replicated state ✓");

    // Test removing a node
    println!("\n=== Testing node removal ===");

    cluster
        .remove_node(new_node_id)
        .await
        .expect("Failed to remove node");

    println!("Removed node {} from cluster", new_node_id);

    // Wait for removal to propagate
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify remaining nodes see correct cluster size
    for node_id in 1..=3 {
        let client = cluster
            .client(node_id)
            .await
            .expect(&format!("Failed to get client for node {}", node_id));

        let (_leader_id, nodes, _term) = client.get_cluster_status().await.expect(&format!(
            "Failed to get cluster status from node {}",
            node_id
        ));

        assert_eq!(
            nodes.len(),
            3,
            "Node {} should see 3 nodes after removal",
            node_id
        );
    }

    println!("Node removal successful, cluster back to 3 nodes ✓");

    println!("\n=== All tests passed! ===");
    println!("✓ Three-node cluster formation");
    println!("✓ P2P info propagation");
    println!("✓ Leader election and convergence");
    println!("✓ Distributed operations (VM creation)");
    println!("✓ State replication");
    println!("✓ Dynamic node addition");
    println!("✓ Dynamic node removal");
}

#[tokio::test]
async fn test_cluster_formation_stress() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=info")
        .try_init();

    println!("=== Starting cluster formation stress test ===");

    // Test rapid cluster formation and teardown
    for i in 1..=3 {
        println!("\nIteration {}/3", i);

        let cluster = TestCluster::with_size(3).await.expect("Failed to create cluster");

        cluster
            .wait_for_convergence(Duration::from_secs(30))
            .await
            .expect("Cluster failed to converge");

        // Verify basic operation
        let leader_client = cluster
            .leader_client()
            .await
            .expect("Failed to get leader client");

        let (_leader_id, nodes, _term) = leader_client
            .get_cluster_status()
            .await
            .expect("Failed to get cluster status");

        assert_eq!(nodes.len(), 3, "Should have 3 nodes");

        println!("Iteration {} successful", i);

        // Cluster will be dropped here, cleaning up resources
    }

    println!("\n=== Stress test passed! ===");
}
