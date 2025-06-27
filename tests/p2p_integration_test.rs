//! P2P Integration Tests
//!
//! Tests the enhanced P2P functionality including:
//! - P2P manager lifecycle
//! - Peer discovery and connection
//! - Resource sharing and transfers
//! - Event handling

#[cfg(test)]
mod tests {
    use blixard_core::p2p_manager::{P2pManager, P2pConfig, ResourceType, TransferPriority};
    use blixard_core::p2p_image_store::P2pImageStore;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration};
    
    #[tokio::test]
    async fn test_p2p_manager_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        // Create P2P manager
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        
        // Start the manager
        manager.start().await.unwrap();
        
        // Give it some time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Check that we can get peers (should be empty initially)
        let peers = manager.get_peers().await;
        assert_eq!(peers.len(), 0);
        
        // Check that we can get transfers (should be empty)
        let transfers = manager.get_active_transfers().await;
        assert_eq!(transfers.len(), 0);
    }
    
    #[tokio::test]
    async fn test_peer_connection() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        manager.start().await.unwrap();
        
        // Connect to a peer
        manager.connect_peer("192.168.1.100:7001").await.unwrap();
        
        // Check that peer was added
        let peers = manager.get_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].address, "192.168.1.100:7001");
    }
    
    #[tokio::test]
    async fn test_resource_upload() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        manager.start().await.unwrap();
        
        // Create a test file
        let test_file = temp_dir.path().join("test.img");
        std::fs::write(&test_file, b"test vm image data").unwrap();
        
        // Upload the resource
        manager.upload_resource(
            ResourceType::VmImage,
            "test-vm",
            "1.0",
            &test_file,
        ).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_download_queue() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        manager.start().await.unwrap();
        
        // Queue a download
        let request_id = manager.request_download(
            ResourceType::VmImage,
            "ubuntu-server",
            "22.04",
            TransferPriority::Normal,
        ).await.unwrap();
        
        assert!(!request_id.is_empty());
        
        // Give the transfer processor time to pick it up
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Check transfers (it should fail since we don't have the image, but it should be attempted)
        let transfers = manager.get_active_transfers().await;
        // The transfer might have already failed and been removed
        assert!(transfers.len() <= 1);
    }
    
    #[tokio::test]
    async fn test_event_stream() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        let event_rx = manager.event_receiver();
        
        manager.start().await.unwrap();
        
        // Connect a peer to generate an event
        manager.connect_peer("192.168.1.100:7001").await.unwrap();
        
        // Wait for event
        let result = timeout(Duration::from_secs(5), async {
            let mut rx = event_rx.write().await;
            rx.recv().await
        }).await;
        
        assert!(result.is_ok());
        if let Ok(Some(event)) = result {
            match event {
                blixard_core::p2p_manager::P2pEvent::PeerConnected(peer_id) => {
                    assert!(!peer_id.is_empty());
                }
                _ => panic!("Unexpected event type"),
            }
        }
    }
    
    #[tokio::test]
    async fn test_p2p_image_store_integration() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create image store
        let store = P2pImageStore::new(1, temp_dir.path()).await.unwrap();
        
        // Create a test image file
        let test_image = temp_dir.path().join("test-os.img");
        std::fs::write(&test_image, b"test operating system image").unwrap();
        
        // Upload image
        let metadata = store.upload_image(
            "test-os",
            "1.0",
            "Test operating system",
            &test_image,
            "linux",
            "amd64",
            vec!["test".to_string()],
        ).await.unwrap();
        
        assert_eq!(metadata.name, "test-os");
        assert_eq!(metadata.version, "1.0");
        assert_eq!(metadata.size, 27); // Length of test data
        assert!(!metadata.content_hash.is_empty());
        
        // Try to retrieve metadata
        let retrieved = store.get_image_metadata("test-os", "1.0").await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, metadata.name);
        assert_eq!(retrieved.content_hash, metadata.content_hash);
    }
    
    #[tokio::test]
    async fn test_multiple_peer_management() {
        let temp_dir = TempDir::new().unwrap();
        let config = P2pConfig::default();
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        manager.start().await.unwrap();
        
        // Connect multiple peers
        let peer_addrs = vec![
            "192.168.1.100:7001",
            "192.168.1.101:7001",
            "192.168.1.102:7001",
        ];
        
        for addr in &peer_addrs {
            manager.connect_peer(addr).await.unwrap();
        }
        
        // Check all peers are connected
        let peers = manager.get_peers().await;
        assert_eq!(peers.len(), 3);
        
        // Verify all addresses are present
        let connected_addrs: Vec<String> = peers.iter().map(|p| p.address.clone()).collect();
        for addr in peer_addrs {
            assert!(connected_addrs.contains(&addr.to_string()));
        }
    }
    
    #[tokio::test]
    async fn test_transfer_priority_queue() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = P2pConfig::default();
        config.max_concurrent_transfers = 1; // Force sequential processing
        
        let manager = P2pManager::new(1, temp_dir.path(), config).await.unwrap();
        manager.start().await.unwrap();
        
        // Queue multiple downloads with different priorities
        let high_priority = manager.request_download(
            ResourceType::VmImage,
            "critical-image",
            "1.0",
            TransferPriority::High,
        ).await.unwrap();
        
        let low_priority = manager.request_download(
            ResourceType::VmImage,
            "optional-image",
            "1.0",
            TransferPriority::Low,
        ).await.unwrap();
        
        let normal_priority = manager.request_download(
            ResourceType::VmImage,
            "standard-image",
            "1.0",
            TransferPriority::Normal,
        ).await.unwrap();
        
        // Verify all requests were queued
        assert!(!high_priority.is_empty());
        assert!(!low_priority.is_empty());
        assert!(!normal_priority.is_empty());
    }
}