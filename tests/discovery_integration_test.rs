//! Integration tests for node discovery

use blixard::discovery_manager::{DiscoveryManager, DiscoveryConfig, DiscoverySource, DiscoveredPeer};
use blixard::node_discovery::{NodeDiscovery, NodeRegistryEntry};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

#[tokio::test]
async fn test_node_registry_discovery() {
    let temp_dir = TempDir::new().unwrap();
    let registry_path = temp_dir.path().join("node1.json");
    
    // Create a node registry entry
    let entry = NodeRegistryEntry {
        cluster_node_id: 42,
        iroh_node_id: base64::encode(vec![1, 2, 3, 4]),
        direct_addresses: vec!["127.0.0.1:7001".to_string()],
        relay_url: Some("https://relay.example.com".to_string()),
        address: "127.0.0.1:7001".to_string(),
    };
    
    // Save the registry
    blixard::node_discovery::save_node_registry(
        registry_path.to_str().unwrap(), 
        &entry
    ).await.unwrap();
    
    // Test discovery
    let mut discovery = NodeDiscovery::new();
    let discovered = discovery.discover_node(registry_path.to_str().unwrap()).await.unwrap();
    
    assert_eq!(discovered.cluster_node_id, 42);
    assert_eq!(discovered.iroh_node_id, entry.iroh_node_id);
    assert_eq!(discovered.direct_addresses, entry.direct_addresses);
}

#[tokio::test]
async fn test_discovery_manager_static_file() {
    let temp_dir = TempDir::new().unwrap();
    let static_file = temp_dir.path().join("peers.json");
    
    // Create static peer list
    let peers = vec![
        DiscoveredPeer {
            node_id: 1,
            iroh_node_id: base64::encode(vec![1, 2, 3]),
            direct_addresses: vec!["127.0.0.1:7001".to_string()],
            relay_url: None,
            source: "static".to_string(),
            discovered_at: std::time::SystemTime::now(),
        },
        DiscoveredPeer {
            node_id: 2,
            iroh_node_id: base64::encode(vec![4, 5, 6]),
            direct_addresses: vec!["127.0.0.1:7002".to_string()],
            relay_url: Some("https://relay.example.com".to_string()),
            source: "static".to_string(),
            discovered_at: std::time::SystemTime::now(),
        },
    ];
    
    // Save to file
    std::fs::write(&static_file, serde_json::to_string(&peers).unwrap()).unwrap();
    
    // Create discovery manager
    let config = DiscoveryConfig {
        sources: vec![
            DiscoverySource::StaticFile {
                path: static_file.to_str().unwrap().to_string(),
            }
        ],
        refresh_interval: Duration::from_secs(60),
        auto_connect: false, // Disable auto-connect for testing
    };
    
    let manager = DiscoveryManager::new(config);
    
    // Refresh peers
    manager.refresh_peers().await.unwrap();
    
    // Check discovered peers
    let discovered = manager.get_peers().await;
    assert_eq!(discovered.len(), 2);
    
    let peer1 = manager.get_peer(1).await.unwrap();
    assert_eq!(peer1.node_id, 1);
    assert_eq!(peer1.direct_addresses[0], "127.0.0.1:7001");
    
    let peer2 = manager.get_peer(2).await.unwrap();
    assert_eq!(peer2.node_id, 2);
    assert_eq!(peer2.relay_url, Some("https://relay.example.com".to_string()));
}

#[tokio::test]
async fn test_discovery_manager_multiple_sources() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create registry file
    let registry_path = temp_dir.path().join("node1.json");
    let registry_entry = NodeRegistryEntry {
        cluster_node_id: 1,
        iroh_node_id: base64::encode(vec![1, 2, 3]),
        direct_addresses: vec!["127.0.0.1:7001".to_string()],
        relay_url: None,
        address: "127.0.0.1:7001".to_string(),
    };
    blixard::node_discovery::save_node_registry(
        registry_path.to_str().unwrap(), 
        &registry_entry
    ).await.unwrap();
    
    // Create static file
    let static_file = temp_dir.path().join("static.json");
    let static_peers = vec![
        DiscoveredPeer {
            node_id: 2,
            iroh_node_id: base64::encode(vec![4, 5, 6]),
            direct_addresses: vec!["127.0.0.1:7002".to_string()],
            relay_url: None,
            source: "static".to_string(),
            discovered_at: std::time::SystemTime::now(),
        },
    ];
    std::fs::write(&static_file, serde_json::to_string(&static_peers).unwrap()).unwrap();
    
    // Create discovery manager with multiple sources
    let config = DiscoveryConfig {
        sources: vec![
            DiscoverySource::NodeRegistry {
                locations: vec![registry_path.to_str().unwrap().to_string()],
            },
            DiscoverySource::StaticFile {
                path: static_file.to_str().unwrap().to_string(),
            },
        ],
        refresh_interval: Duration::from_secs(60),
        auto_connect: false,
    };
    
    let manager = DiscoveryManager::new(config);
    manager.refresh_peers().await.unwrap();
    
    // Should have discovered both peers
    let discovered = manager.get_peers().await;
    assert_eq!(discovered.len(), 2);
    
    // Check sources
    let peer1 = manager.get_peer(1).await.unwrap();
    assert!(peer1.source.starts_with("registry:"));
    
    let peer2 = manager.get_peer(2).await.unwrap();
    assert!(peer2.source.starts_with("static:"));
}

#[tokio::test]
async fn test_discovery_config_from_env() {
    use std::env;
    
    // Set environment variable
    let temp_dir = TempDir::new().unwrap();
    let reg1 = temp_dir.path().join("reg1.json");
    let reg2 = temp_dir.path().join("reg2.json");
    
    env::set_var("BLIXARD_NODE_REGISTRIES", format!("{},{}", 
        reg1.to_str().unwrap(), 
        reg2.to_str().unwrap()
    ));
    
    // Create discovery config
    let config = blixard::discovery_manager::create_discovery_config(vec![
        "manual-reg.json".to_string()
    ]);
    
    // Should have one source with 3 locations total
    assert_eq!(config.sources.len(), 1);
    if let DiscoverySource::NodeRegistry { locations } = &config.sources[0] {
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0], "manual-reg.json");
    } else {
        panic!("Expected NodeRegistry source");
    }
    
    // Clean up
    env::remove_var("BLIXARD_NODE_REGISTRIES");
}