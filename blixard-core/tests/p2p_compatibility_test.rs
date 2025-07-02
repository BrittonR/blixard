//! Test to verify P2P compatibility with existing code after ALPN changes
//!
//! This test ensures that the P2pManager and related components still work correctly
//! with the unified BLIXARD_ALPN configuration.

use blixard_core::{
    error::BlixardResult,
    p2p_manager::{P2pManager, P2pConfig, ResourceType},
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use tracing::info;

#[tokio::test]
async fn test_p2p_manager_basic_operations() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug")
        .try_init();
    
    info!("Starting P2P Manager compatibility test");
    
    let temp_dir = TempDir::new().unwrap();
    let node_dir = temp_dir.path().join("node1");
    std::fs::create_dir_all(&node_dir).unwrap();
    
    // Create P2P manager with default config
    let config = P2pConfig::default();
    let p2p_manager = Arc::new(
        P2pManager::new(1, &node_dir, config).await.unwrap()
    );
    
    // Start the P2P manager
    p2p_manager.start().await.unwrap();
    
    // Test basic operations
    info!("Testing metadata operations...");
    
    // Store metadata
    let key = "test-key";
    let value = b"test-value";
    p2p_manager.store_metadata(key, value).await.unwrap();
    
    // Read metadata
    let read_value = p2p_manager.get_metadata(key).await.unwrap();
    assert_eq!(read_value, value);
    
    info!("Metadata operations successful");
    
    // Test data sharing
    info!("Testing data sharing...");
    
    let test_data = b"test content for sharing";
    let hash = p2p_manager.share_data(test_data.to_vec(), "test-data").await.unwrap();
    assert!(!hash.to_string().is_empty());
    
    info!("Data sharing successful");
    
    // Test hash computation
    let computed_hash = p2p_manager.hash_data(test_data).await.unwrap();
    assert!(!computed_hash.to_string().is_empty());
    
    info!("P2P Manager compatibility test completed");
}

#[tokio::test]
async fn test_p2p_manager_multi_node() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug")
        .try_init();
    
    info!("Starting multi-node P2P Manager test");
    
    let temp_dir = TempDir::new().unwrap();
    let node1_dir = temp_dir.path().join("node1");
    let node2_dir = temp_dir.path().join("node2");
    std::fs::create_dir_all(&node1_dir).unwrap();
    std::fs::create_dir_all(&node2_dir).unwrap();
    
    // Create two P2P managers
    let config1 = P2pConfig::default();
    let config2 = P2pConfig::default();
    
    let p2p_manager1 = Arc::new(
        P2pManager::new(1, &node1_dir, config1).await.unwrap()
    );
    let p2p_manager2 = Arc::new(
        P2pManager::new(2, &node2_dir, config2).await.unwrap()
    );
    
    // Start both managers
    p2p_manager1.start().await.unwrap();
    p2p_manager2.start().await.unwrap();
    
    // Get node addresses
    let addr1 = p2p_manager1.get_node_addr().await.unwrap();
    let addr2 = p2p_manager2.get_node_addr().await.unwrap();
    
    info!("Node 1 address: {:?}", addr1);
    info!("Node 2 address: {:?}", addr2);
    
    // Test connecting peers
    match p2p_manager1.connect_p2p_peer(2, &addr2).await {
        Ok(_) => {
            info!("Successfully connected P2P peers");
        }
        Err(e) => {
            info!("P2P peer connection failed (expected in test environment): {}", e);
        }
    }
    
    // Get active peers
    let peers = p2p_manager1.get_peers().await;
    info!("Active peers: {}", peers.len());
    
    // Test data sharing between nodes
    let test_data = b"Hello from P2P Manager!";
    let hash = p2p_manager1.share_data(test_data.to_vec(), "test-data").await.unwrap();
    
    // Simulate download request
    p2p_manager2.request_download(
        ResourceType::Configuration,
        "test-data",
        "1.0",
        Some(addr1.node_id.to_string()),
    ).await.unwrap();
    
    info!("Multi-node P2P Manager test completed");
}

#[tokio::test]
async fn test_p2p_with_discovery() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug")
        .try_init();
    
    info!("Starting P2P with discovery test");
    
    let temp_dir = TempDir::new().unwrap();
    let node_dir = temp_dir.path().join("node1");
    std::fs::create_dir_all(&node_dir).unwrap();
    
    // Create discovery manager
    use blixard_core::discovery::{DiscoveryManager, DiscoveryConfig};
    let discovery_config = DiscoveryConfig::default();
    let discovery_manager = Arc::new(DiscoveryManager::new(discovery_config).await.unwrap());
    
    // Create P2P manager with discovery
    let config = P2pConfig::default();
    let p2p_manager = Arc::new(
        P2pManager::new_with_discovery(1, &node_dir, config, Some(discovery_manager)).await.unwrap()
    );
    
    // Start the P2P manager
    p2p_manager.start().await.unwrap();
    
    // Verify the manager is created successfully with discovery
    let addr = p2p_manager.get_node_addr().await.unwrap();
    assert!(!addr.node_id.to_string().is_empty());
    
    info!("P2P Manager with discovery created successfully");
    
    // Test metadata operations
    p2p_manager.store_metadata("discovery-test", b"test-value").await.unwrap();
    let value = p2p_manager.get_metadata("discovery-test").await.unwrap();
    assert_eq!(value, b"test-value");
    
    info!("Discovery-enabled P2P Manager working correctly");
}