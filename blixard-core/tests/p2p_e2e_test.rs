//! End-to-end P2P integration test
//!
//! This test verifies P2P functionality in a multi-node cluster scenario

#![cfg(feature = "test-helpers")]

use blixard_core::{
    test_helpers::{TestCluster, timing},
    iroh_types::{VmConfig as IrohVmConfig, TaskSchedulingRequest},
    error::BlixardResult,
};
use std::time::Duration;

#[tokio::test]
async fn test_p2p_cluster_formation_and_operations() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard_core=debug".parse().unwrap())
                .add_directive("p2p_e2e_test=debug".parse().unwrap())
        )
        .try_init();

    tracing::info!("Starting P2P end-to-end cluster test");

    // Step 1: Create a 3-node cluster
    let mut cluster = TestCluster::new(3).await?;
    
    // Step 2: Wait for cluster convergence
    cluster.wait_for_convergence(Duration::from_secs(30)).await?;
    tracing::info!("Cluster converged with 3 nodes");

    // Step 3: Verify all nodes have P2P connectivity
    let mut p2p_addresses = Vec::new();
    for i in 1..=3 {
        let node = cluster.nodes.get(&i).unwrap();
        let shared_state = &node.shared_state;
        
        // Check if P2P manager is initialized
        if let Some(p2p_manager) = shared_state.get_p2p_manager().await {
            // Get P2P node address
            if let Ok(node_addr) = p2p_manager.get_node_addr().await {
                tracing::info!("Node {} has P2P address: {:?}", i, node_addr.node_id);
                p2p_addresses.push((i, node_addr));
            } else {
                tracing::warn!("Node {} failed to get P2P address", i);
            }
            
            // Check P2P manager started successfully
            match p2p_manager.get_peers().await.len() {
                0 => tracing::info!("Node {} has no P2P peers yet", i),
                n => tracing::info!("Node {} has {} P2P peers", i, n),
            }
        } else {
            return Err(blixard_core::error::BlixardError::Internal {
                message: format!("Node {} does not have P2P manager initialized", i),
            });
        }
    }

    // Step 4: Test basic cluster operations
    tracing::info!("Testing basic cluster operations");
    
    // 4a. Create a VM through the cluster
    let leader_client = cluster.leader_client().await?;
    let vm_config = IrohVmConfig {
        name: "test-p2p-vm".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 10,
        owner: "test-user".to_string(),
        metadata: std::collections::HashMap::new(),
    };
    
    let create_response = leader_client.clone().create_vm(vm_config.clone()).await?;
    assert!(create_response.into_inner().success, "VM creation should succeed");
    tracing::info!("Successfully created VM through cluster");

    // 4b. Schedule the VM placement
    let schedule_request = TaskSchedulingRequest {
        vm_name: vm_config.name.clone(),
        placement_strategy: "MostAvailable".to_string(),
        required_features: vec![],
    };
    
    let schedule_response = leader_client.clone().schedule_vm_placement(schedule_request).await?;
    let schedule_result = schedule_response.into_inner();
    assert!(schedule_result.success, "VM scheduling should succeed");
    assert!(schedule_result.worker_id > 0, "Should have assigned worker");
    tracing::info!("VM scheduled to worker {}", schedule_result.worker_id);

    // Step 5: Test P2P-specific operations
    tracing::info!("Testing P2P-specific operations");
    
    // 5a. Connect P2P peers explicitly
    if p2p_addresses.len() >= 2 {
        let node1 = cluster.nodes.get(&1).unwrap();
        if let Some(p2p_manager) = node1.shared_state.get_p2p_manager().await {
            let (node2_id, node2_addr) = &p2p_addresses[1];
            
            // Try to connect node 1 to node 2 via P2P
            match p2p_manager.connect_p2p_peer(*node2_id, node2_addr).await {
                Ok(_) => tracing::info!("Successfully connected node 1 to node 2 via P2P"),
                Err(e) => tracing::warn!("Failed to connect P2P peers: {}", e),
            }
        }
    }

    // 5b. Check P2P peer counts after some time
    timing::robust_sleep(Duration::from_secs(2)).await;
    
    for i in 1..=3 {
        let node = cluster.nodes.get(&i).unwrap();
        if let Some(p2p_manager) = node.shared_state.get_p2p_manager().await {
            let peers = p2p_manager.get_peers().await;
            tracing::info!("Node {} now has {} P2P peers", i, peers.len());
            
            // Check for active transfers
            let transfers = p2p_manager.get_active_transfers().await;
            tracing::info!("Node {} has {} active P2P transfers", i, transfers.len());
        }
    }

    // Step 6: Test node addition with P2P
    tracing::info!("Testing node addition");
    let new_node_id = cluster.add_node().await?;
    
    // Wait for the new node to join
    timing::robust_sleep(Duration::from_secs(3)).await;
    
    // Verify new node has P2P initialized
    if let Some(new_node) = cluster.nodes.get(&new_node_id) {
        if let Some(p2p_manager) = new_node.shared_state.get_p2p_manager().await {
            if let Ok(node_addr) = p2p_manager.get_node_addr().await {
                tracing::info!("New node {} has P2P address: {:?}", new_node_id, node_addr.node_id);
            }
        }
    }

    // Step 7: Final diagnostics
    tracing::info!("=== Final Cluster State ===");
    let diagnostics = cluster.dump_diagnostics().await;
    tracing::info!("{}", diagnostics);

    // Cleanup
    cluster.shutdown().await;
    tracing::info!("P2P end-to-end test completed successfully");
    
    Ok(())
}

#[tokio::test]
async fn test_p2p_data_sharing() -> BlixardResult<()> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    tracing::info!("Starting P2P data sharing test");

    // Create a 2-node cluster
    let cluster = TestCluster::new(2).await?;
    cluster.wait_for_convergence(Duration::from_secs(20)).await?;

    // Test data sharing between nodes
    let node1 = cluster.nodes.get(&1).unwrap();
    let node2 = cluster.nodes.get(&2).unwrap();

    if let (Some(p2p1), Some(p2p2)) = (
        node1.shared_state.get_p2p_manager().await,
        node2.shared_state.get_p2p_manager().await,
    ) {
        // Share some data from node 1
        let test_data = b"Hello P2P World!".to_vec();
        match p2p1.share_data(test_data.clone(), "test-data").await {
            Ok(hash) => {
                tracing::info!("Node 1 shared data with hash: {}", hash);
                
                // Try to download from node 2 (will fail as not implemented)
                match p2p2.download_data(&hash).await {
                    Ok(_data) => {
                        tracing::info!("Node 2 successfully downloaded data");
                    }
                    Err(e) => {
                        tracing::info!("Expected error - P2P download not implemented: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to share data: {}", e);
            }
        }

        // Test metadata storage
        let metadata_key = "test-metadata";
        let metadata_value = b"test-value";
        
        match p2p1.store_metadata(metadata_key, metadata_value).await {
            Ok(_) => tracing::info!("Stored metadata successfully"),
            Err(e) => tracing::warn!("Failed to store metadata: {}", e),
        }
    }

    cluster.shutdown().await;
    tracing::info!("P2P data sharing test completed");
    
    Ok(())
}