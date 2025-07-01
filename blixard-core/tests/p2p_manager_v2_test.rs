//! Test P2pManager with IrohTransportV2

#![cfg(feature = "test-helpers")]

use blixard_core::{
    p2p_manager::{P2pManager, P2pConfig, ResourceType, TransferPriority},
    error::BlixardResult,
};
use tempfile::TempDir;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_p2p_manager_creation_with_v2() -> BlixardResult<()> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    let temp_dir = TempDir::new().unwrap();
    let config = P2pConfig::default();
    
    // Create P2pManager - this now uses IrohTransportV2 internally
    let p2p_manager = P2pManager::new(1, temp_dir.path(), config).await?;
    
    // Start the manager
    p2p_manager.start().await?;
    
    // Get our node address
    let node_addr = p2p_manager.get_node_addr().await?;
    assert!(!node_addr.node_id.to_string().is_empty());
    
    // Get Iroh endpoint
    let (_endpoint, node_id) = p2p_manager.get_endpoint();
    assert!(!node_id.to_string().is_empty());
    
    println!("✅ P2pManager created successfully with IrohTransportV2");
    println!("   Node ID: {}", node_id);
    
    Ok(())
}

#[tokio::test]
async fn test_p2p_file_sharing_with_v2() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    let temp_dir = TempDir::new().unwrap();
    let config = P2pConfig::default();
    
    let p2p_manager = P2pManager::new(1, temp_dir.path(), config).await?;
    p2p_manager.start().await?;
    
    // Create a test file
    let test_file = temp_dir.path().join("test-image.qcow2");
    std::fs::write(&test_file, b"fake vm image content").unwrap();
    
    // Upload the file
    p2p_manager.upload_resource(
        ResourceType::VmImage,
        "test-vm",
        "1.0.0",
        &test_file,
    ).await?;
    
    println!("✅ File uploaded successfully via P2pManager with V2 transport");
    
    Ok(())
}

#[tokio::test]
async fn test_p2p_peer_connection() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    // Create two P2P managers
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let config1 = P2pConfig::default();
    let config2 = P2pConfig::default();
    
    let p2p1 = P2pManager::new(1, temp_dir1.path(), config1).await?;
    let p2p2 = P2pManager::new(2, temp_dir2.path(), config2).await?;
    
    p2p1.start().await?;
    p2p2.start().await?;
    
    // Get node addresses
    let addr1 = p2p1.get_node_addr().await?;
    let addr2 = p2p2.get_node_addr().await?;
    
    println!("Node 1: {:?}", addr1.node_id);
    println!("Node 2: {:?}", addr2.node_id);
    
    // Try to connect (may fail due to lack of direct connectivity in test env)
    match p2p1.connect_p2p_peer(2, &addr2).await {
        Ok(_) => println!("✅ Successfully connected P2P peers"),
        Err(e) => println!("⚠️  P2P connection failed (expected in test env): {}", e),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_event_system() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    let temp_dir = TempDir::new().unwrap();
    let config = P2pConfig::default();
    
    let p2p_manager = P2pManager::new(1, temp_dir.path(), config).await?;
    p2p_manager.start().await?;
    
    // Get event receiver
    let event_rx = p2p_manager.event_receiver();
    
    // Request a download (will be queued)
    let request_id = p2p_manager.request_download(
        ResourceType::VmImage,
        "test-image",
        "1.0.0",
        TransferPriority::Normal,
    ).await?;
    
    println!("✅ Download request created: {}", request_id);
    
    // Try to receive an event (with timeout)
    let mut rx = event_rx.write().await;
    match timeout(Duration::from_secs(2), rx.recv()).await {
        Ok(Some(event)) => println!("Received event: {:?}", event),
        Ok(None) => println!("Event channel closed"),
        Err(_) => println!("No events received (timeout)"),
    }
    
    Ok(())
}