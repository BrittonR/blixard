//! Integration test for Iroh as default transport
//!
//! This test verifies that multi-node clusters work correctly
//! when Iroh is the default transport.

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardResult,
    test_helpers::{TestCluster, TestNode},
    transport::config::{IrohConfig, TransportConfig},
};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_single_node_with_iroh_default() -> BlixardResult<()> {
    // Create a single node - should use Iroh by default
    let test_node = TestNode::new(1).await?;

    // Verify the node uses Iroh transport
    let config = &test_node.node().shared().config;
    assert!(config.transport_config.is_some());

    match &config.transport_config {
        Some(TransportConfig::Iroh(_)) => {
            // Expected - Iroh is the default
        }
        _ => panic!(
            "Expected Iroh transport but got {:?}",
            config.transport_config
        ),
    }

    // Start the node
    test_node.start().await?;

    // Verify node is running
    assert!(test_node.is_running().await);

    // Stop the node
    test_node.stop().await?;

    Ok(())
}

#[tokio::test]
async fn test_three_node_cluster_with_iroh() -> BlixardResult<()> {
    // Create a 3-node cluster - all should use Iroh by default
    let mut cluster = TestCluster::new(3).await?;

    // Verify all nodes use Iroh transport
    for i in 1..=3 {
        let node = cluster.get_node(i).expect("Node should exist");
        let config = &node.node().shared().config;

        match &config.transport_config {
            Some(TransportConfig::Iroh(_)) => {
                // Expected
            }
            _ => panic!(
                "Node {} expected Iroh transport but got {:?}",
                i, config.transport_config
            ),
        }
    }

    // Wait for cluster to form
    let formed = timeout(Duration::from_secs(10), async {
        loop {
            if cluster.is_cluster_formed().await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    assert!(formed.is_ok(), "Cluster should form within 10 seconds");

    // Verify cluster status
    let (leader_id, members, _term) = cluster.get_node(1).unwrap().get_cluster_status().await?;

    assert!(leader_id > 0, "Should have a leader");
    assert_eq!(members.len(), 3, "Should have 3 members");

    // Test Raft consensus by creating a VM
    let vm_name = "test-vm-iroh";
    cluster.create_vm_on_leader(vm_name, 1, 1024).await?;

    // Verify VM exists on all nodes
    for i in 1..=3 {
        let node = cluster.get_node(i).unwrap();
        let vms = node.list_vms().await?;

        let vm_exists = vms.iter().any(|(config, _)| config.name == vm_name);
        assert!(vm_exists, "VM should exist on node {}", i);
    }

    Ok(())
}

#[tokio::test]
async fn test_node_join_with_iroh() -> BlixardResult<()> {
    // Start with a 2-node cluster
    let mut cluster = TestCluster::new(2).await?;

    // Wait for initial cluster to form
    timeout(Duration::from_secs(10), async {
        while !cluster.is_cluster_formed().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Initial cluster should form");

    // Add a third node - should also use Iroh
    cluster.add_node(3).await?;

    // Verify new node uses Iroh
    let new_node = cluster.get_node(3).expect("Node 3 should exist");
    let config = &new_node.node().shared().config;

    match &config.transport_config {
        Some(TransportConfig::Iroh(_)) => {
            // Expected
        }
        _ => panic!(
            "New node expected Iroh transport but got {:?}",
            config.transport_config
        ),
    }

    // Wait for new node to join
    timeout(Duration::from_secs(10), async {
        loop {
            let (_leader, members, _) = cluster
                .get_node(1)
                .unwrap()
                .get_cluster_status()
                .await
                .unwrap();

            if members.len() == 3 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Node should join cluster");

    // Verify all nodes see 3 members
    for i in 1..=3 {
        let node = cluster.get_node(i).unwrap();
        let (_leader, members, _) = node.get_cluster_status().await?;
        assert_eq!(members.len(), 3, "Node {} should see 3 members", i);
    }

    Ok(())
}

#[tokio::test]
async fn test_explicit_iroh_config() -> BlixardResult<()> {
    // Create a node with explicit Iroh config
    let iroh_config = IrohConfig {
        enabled: true,
        home_relay: "https://custom.relay.example".to_string(),
        discovery_port: 12345,
        alpn_protocols: vec!["custom/1".to_string()],
    };

    let mut test_node = TestNode::new(1).await?;
    test_node.node_mut().shared().config.transport_config =
        Some(TransportConfig::Iroh(iroh_config.clone()));

    // Verify config is preserved
    match &test_node.node().shared().config.transport_config {
        Some(TransportConfig::Iroh(config)) => {
            assert_eq!(config.home_relay, "https://custom.relay.example");
            assert_eq!(config.discovery_port, 12345);
            assert_eq!(config.alpn_protocols, vec!["custom/1".to_string()]);
        }
        _ => panic!("Expected custom Iroh config"),
    }

    Ok(())
}

#[tokio::test]
async fn test_raft_messages_over_iroh() -> BlixardResult<()> {
    // Create a cluster and verify Raft messages flow
    let mut cluster = TestCluster::new(3).await?;

    // Wait for cluster formation
    timeout(Duration::from_secs(10), async {
        while !cluster.is_cluster_formed().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Cluster should form");

    // Find the leader
    let leader_id = cluster.find_leader().await.expect("Should have a leader");

    // Submit multiple proposals to exercise Raft
    for i in 0..5 {
        let vm_name = format!("raft-test-vm-{}", i);
        cluster
            .create_vm_on_node(leader_id, &vm_name, 1, 512)
            .await?;
    }

    // Verify all VMs are replicated
    for node_id in 1..=3 {
        let node = cluster.get_node(node_id).unwrap();
        let vms = node.list_vms().await?;

        assert_eq!(vms.len(), 5, "Node {} should have 5 VMs", node_id);
    }

    Ok(())
}
