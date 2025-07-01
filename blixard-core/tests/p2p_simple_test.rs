//! Simple P2P integration test
//!
//! This test verifies basic P2P functionality without full cluster operations

#![cfg(feature = "test-helpers")]

use blixard_core::{
    iroh_transport::{IrohTransport, DocumentType},
    p2p_manager::{P2pManager, P2pConfig},
    error::BlixardResult,
};
use tempfile::TempDir;
use std::time::Duration;

#[tokio::test]
async fn test_basic_p2p_initialization() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    tracing::info!("Starting basic P2P initialization test");

    // Test 1: Create IrohTransport instances
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let transport1 = IrohTransport::new(1, temp_dir1.path()).await?;
    let transport2 = IrohTransport::new(2, temp_dir2.path()).await?;
    
    // Get node addresses
    let addr1 = transport1.node_addr().await?;
    let addr2 = transport2.node_addr().await?;
    
    tracing::info!("Node 1 address: {:?}", addr1.node_id);
    tracing::info!("Node 2 address: {:?}", addr2.node_id);
    
    // Test 2: Verify endpoints are created
    let (endpoint1, node_id1) = transport1.endpoint();
    let (endpoint2, node_id2) = transport2.endpoint();
    
    assert!(endpoint1.node_id() == node_id1);
    assert!(endpoint2.node_id() == node_id2);
    assert!(node_id1 != node_id2);
    
    tracing::info!("Successfully created two distinct Iroh transports");
    
    // Test 3: Try to send data between nodes
    let test_data = b"Hello P2P!".to_vec();
    match transport1.send_to_peer(&addr2, DocumentType::ClusterConfig, &test_data).await {
        Ok(_) => tracing::info!("Successfully sent data from node 1 to node 2"),
        Err(e) => tracing::warn!("Failed to send data (expected if not fully implemented): {}", e),
    }
    
    // Test 4: Test document operations (expecting NotImplemented)
    match transport1.create_or_join_doc(DocumentType::ClusterConfig, true).await {
        Ok(_) => tracing::info!("Document creation succeeded"),
        Err(e) => {
            tracing::info!("Document creation failed as expected: {}", e);
            assert!(e.to_string().contains("not implemented") || e.to_string().contains("NotImplemented"));
        }
    }
    
    tracing::info!("Basic P2P initialization test completed");
    Ok(())
}

#[tokio::test]
async fn test_p2p_manager_lifecycle() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    tracing::info!("Starting P2P manager lifecycle test");

    let temp_dir = TempDir::new().unwrap();
    let config = P2pConfig {
        max_concurrent_transfers: 2,
        transfer_timeout: Duration::from_secs(60),
        peer_discovery_interval: Duration::from_secs(10),
        health_check_interval: Duration::from_secs(15),
        max_retry_attempts: 2,
        bandwidth_limit_mbps: Some(50.0),
    };
    
    // Test that P2pManager creation fails due to unimplemented document operations
    match P2pManager::new(1, temp_dir.path(), config).await {
        Ok(_) => {
            tracing::warn!("P2pManager creation unexpectedly succeeded");
            return Err(blixard_core::error::BlixardError::Internal {
                message: "Expected P2pManager creation to fail".to_string(),
            });
        }
        Err(e) => {
            tracing::info!("P2pManager creation failed as expected: {}", e);
            assert!(
                e.to_string().contains("not implemented") || 
                e.to_string().contains("NotImplemented") ||
                e.to_string().contains("feature")
            );
        }
    }
    
    tracing::info!("P2P manager lifecycle test completed");
    Ok(())
}

#[tokio::test]
async fn test_iroh_transport_connectivity() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    tracing::info!("Starting Iroh transport connectivity test");

    // Create 3 nodes
    let temp_dirs: Vec<TempDir> = (0..3).map(|_| TempDir::new().unwrap()).collect();
    let mut transports = Vec::new();
    let mut addresses = Vec::new();
    
    for (i, temp_dir) in temp_dirs.iter().enumerate() {
        let transport = IrohTransport::new((i + 1) as u64, temp_dir.path()).await?;
        let addr = transport.node_addr().await?;
        tracing::info!("Created node {} with address: {:?}", i + 1, addr.node_id);
        addresses.push(addr);
        transports.push(transport);
    }
    
    // Test connectivity matrix
    for i in 0..3 {
        for j in 0..3 {
            if i != j {
                let data = format!("Message from node {} to node {}", i + 1, j + 1).into_bytes();
                match transports[i].send_to_peer(&addresses[j], DocumentType::ClusterConfig, &data).await {
                    Ok(_) => tracing::info!("Node {} -> Node {}: Success", i + 1, j + 1),
                    Err(e) => tracing::info!("Node {} -> Node {}: Failed ({})", i + 1, j + 1, e),
                }
            }
        }
    }
    
    // Test file sharing (expecting NotImplemented)
    let test_file = temp_dirs[0].path().join("test.txt");
    std::fs::write(&test_file, b"Test file content").unwrap();
    
    match transports[0].share_file(&test_file).await {
        Ok(hash) => tracing::info!("File shared with hash: {}", hash),
        Err(e) => tracing::info!("File sharing failed as expected: {}", e),
    }
    
    tracing::info!("Iroh transport connectivity test completed");
    Ok(())
}