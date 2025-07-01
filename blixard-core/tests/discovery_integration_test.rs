//! Integration tests for the discovery module

use blixard_core::discovery::{DiscoveryConfig, DiscoveryManager, DiscoveryEvent};
use iroh::NodeId;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_static_discovery_integration() {
    // Create node IDs for testing
    let node1 = NodeId::from_bytes(&[1u8; 32]).unwrap();
    let node2 = NodeId::from_bytes(&[2u8; 32]).unwrap();
    
    // Configure static nodes
    let mut static_nodes = HashMap::new();
    static_nodes.insert(
        node1.to_string(),
        vec!["127.0.0.1:7001".to_string()],
    );
    static_nodes.insert(
        node2.to_string(),
        vec!["127.0.0.1:7002".to_string(), "192.168.1.100:7002".to_string()],
    );
    
    // Create discovery configuration
    let config = DiscoveryConfig {
        enable_static: true,
        enable_dns: false,
        enable_mdns: false,
        static_nodes,
        ..Default::default()
    };
    
    // Create and start discovery manager
    let mut manager = DiscoveryManager::new(config);
    let mut event_receiver = manager.subscribe().await;
    
    manager.start().await.unwrap();
    
    // Wait for discovery events
    let mut discovered_nodes = Vec::new();
    
    while discovered_nodes.len() < 2 {
        match timeout(Duration::from_secs(5), event_receiver.recv()).await {
            Ok(Some(DiscoveryEvent::NodeDiscovered(info))) => {
                discovered_nodes.push(info.node_id);
            }
            Ok(Some(_)) => {} // Ignore other events
            Ok(None) => break,
            Err(_) => break, // Timeout
        }
    }
    
    // Verify we discovered both nodes
    assert_eq!(discovered_nodes.len(), 2);
    assert!(discovered_nodes.contains(&node1));
    assert!(discovered_nodes.contains(&node2));
    
    // Get all nodes from manager
    let all_nodes = manager.get_nodes().await;
    assert_eq!(all_nodes.len(), 2);
    
    // Check specific node
    let node1_info = manager.get_node(&node1).await.unwrap();
    assert_eq!(node1_info.node_id, node1);
    assert_eq!(node1_info.addresses.len(), 1);
    
    // Stop manager
    manager.stop().await.unwrap();
}

#[tokio::test]
async fn test_discovery_manager_lifecycle() {
    let config = DiscoveryConfig::default();
    let mut manager = DiscoveryManager::new(config);
    
    // Should not be running initially
    assert!(!manager.is_running());
    
    // Start manager
    manager.start().await.unwrap();
    assert!(manager.is_running());
    
    // Starting again should fail
    assert!(manager.start().await.is_err());
    
    // Stop manager
    manager.stop().await.unwrap();
    assert!(!manager.is_running());
    
    // Stopping again should be ok
    manager.stop().await.unwrap();
}

#[tokio::test]
async fn test_mdns_configuration() {
    let our_node = NodeId::from_bytes(&[10u8; 32]).unwrap();
    let our_addr = "127.0.0.1:8000".parse().unwrap();
    
    let config = DiscoveryConfig {
        enable_static: false,
        enable_dns: false,
        enable_mdns: true,
        ..Default::default()
    };
    
    let mut manager = DiscoveryManager::new(config);
    manager.configure_for_node(our_node, vec![our_addr]);
    
    // Start should work even with just mDNS
    manager.start().await.unwrap();
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    manager.stop().await.unwrap();
}