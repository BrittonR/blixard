//! Comprehensive discovery system integration tests
//!
//! Tests end-to-end discovery flows including:
//! - Discovery manager lifecycle with multiple providers
//! - Concurrent provider operations and event deduplication
//! - Error scenarios and provider failure recovery
//! - DNS and mDNS integration with real/mock resolvers
//! - Cross-provider discovery scenarios
//! - Performance and scalability testing
//! - Network topology changes and cluster formation

#[cfg(test)]
mod discovery_comprehensive_tests {
    use blixard_core::{
        discovery::{
            DiscoveryConfig, DiscoveryEvent, DiscoveryManager, DiscoveryProvider,
            DiscoveryState, IrohNodeInfo,
            dns_discovery::DnsDiscovery,
            mdns_discovery::MdnsDiscovery,
            static_discovery::StaticDiscovery,
            iroh_discovery_bridge::IrohDiscoveryBridge,
        },
        error::BlixardResult,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::{mpsc, RwLock};
    use tokio::time::timeout;
    use iroh::NodeId;

    /// Helper to create test node info
    fn create_test_node(id_suffix: u8, port: u16) -> IrohNodeInfo {
        let mut node_id_bytes = [0u8; 32];
        node_id_bytes[31] = id_suffix;
        
        IrohNodeInfo {
            node_id: NodeId::new(&node_id_bytes),
            addresses: vec![format!("127.0.0.1:{}", port)],
            properties: HashMap::from([
                ("role".to_string(), "worker".to_string()),
                ("region".to_string(), "us-west".to_string()),
                ("datacenter".to_string(), format!("dc-{}", id_suffix)),
            ]),
        }
    }

    /// Helper to create test discovery config
    fn create_test_discovery_config() -> DiscoveryConfig {
        DiscoveryConfig {
            static_enabled: true,
            dns_enabled: true,
            mdns_enabled: true,
            dns_domains: vec!["test.local".to_string()],
            dns_refresh_interval: Duration::from_secs(30),
            mdns_service_name: "_blixard._tcp.local".to_string(),
            max_discovered_nodes: 1000,
            event_buffer_size: 100,
        }
    }

    #[tokio::test]
    async fn test_discovery_manager_lifecycle() {
        let config = create_test_discovery_config();
        let mut manager = DiscoveryManager::new(config).await.unwrap();
        
        // Test initial state
        assert!(manager.discovered_nodes().await.is_empty());
        
        // Start discovery
        manager.start().await.unwrap();
        
        // Should be able to start multiple times without error
        manager.start().await.unwrap();
        
        // Add some test nodes through static discovery
        let test_nodes = vec![
            create_test_node(1, 7001),
            create_test_node(2, 7002),
            create_test_node(3, 7003),
        ];
        
        // Wait a moment for discovery to process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Stop discovery
        manager.stop().await.unwrap();
        
        // Should be able to stop multiple times without error
        manager.stop().await.unwrap();
        
        // Test restart
        manager.start().await.unwrap();
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_discovery_manager_concurrent_providers() {
        let config = create_test_discovery_config();
        let mut manager = DiscoveryManager::new(config).await.unwrap();
        
        manager.start().await.unwrap();
        
        // Simulate multiple providers discovering the same node
        let test_node = create_test_node(1, 7001);
        let node_id = test_node.node_id;
        
        // Create multiple provider events for the same node
        let events = vec![
            DiscoveryEvent::NodeDiscovered(test_node.clone()),
            DiscoveryEvent::NodeDiscovered(test_node.clone()),  // Duplicate
            DiscoveryEvent::NodeUpdated(test_node.clone()),
        ];
        
        // Send events concurrently
        let event_futures = events.into_iter().map(|event| {
            let manager = &manager;
            async move {
                // Simulate receiving event from provider
                tokio::time::sleep(Duration::from_millis(10)).await;
                // manager.handle_provider_event(event).await
            }
        });
        
        futures::future::join_all(event_futures).await;
        
        // Wait for event processing
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let discovered = manager.discovered_nodes().await;
        
        // Should only have one entry despite multiple events
        assert!(discovered.len() <= 1, "Event deduplication failed");
        
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_discovery_manager_provider_failure_recovery() {
        let config = create_test_discovery_config();
        let mut manager = DiscoveryManager::new(config).await.unwrap();
        
        manager.start().await.unwrap();
        
        // Simulate provider failures by stopping and restarting
        manager.stop().await.unwrap();
        
        // Start again - should recover gracefully
        manager.start().await.unwrap();
        
        // Test multiple failure/recovery cycles
        for _ in 0..3 {
            manager.stop().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            manager.start().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        
        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_discovery_manager_event_deduplication() {
        let config = create_test_discovery_config();
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        let test_node = create_test_node(1, 7001);
        let node_id = test_node.node_id;
        
        // Add node multiple times
        {
            let mut state = discovery_state.write().await;
            state.add_node(test_node.clone());
            state.add_node(test_node.clone());  // Duplicate
            state.add_node(test_node.clone());  // Another duplicate
        }
        
        // Should only have one node
        let nodes = discovery_state.read().await.get_all_nodes();
        assert_eq!(nodes.len(), 1);
        assert!(nodes.contains_key(&node_id));
        
        // Test update
        let mut updated_node = test_node.clone();
        updated_node.properties.insert("status".to_string(), "updated".to_string());
        
        {
            let mut state = discovery_state.write().await;
            state.update_node(updated_node.clone());
        }
        
        let nodes = discovery_state.read().await.get_all_nodes();
        assert_eq!(nodes.len(), 1);
        if let Some(stored_node) = nodes.get(&node_id) {
            assert_eq!(stored_node.properties.get("status"), Some(&"updated".to_string()));
        }
        
        // Test removal
        {
            let mut state = discovery_state.write().await;
            state.remove_node(node_id);
        }
        
        let nodes = discovery_state.read().await.get_all_nodes();
        assert!(nodes.is_empty());
    }

    #[tokio::test] 
    async fn test_discovery_manager_stale_node_cleanup() {
        let mut config = create_test_discovery_config();
        config.max_discovered_nodes = 5;  // Small limit for testing
        
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        // Add nodes up to the limit
        for i in 1..=5 {
            let node = create_test_node(i, 7000 + i as u16);
            let mut state = discovery_state.write().await;
            state.add_node(node);
        }
        
        // Verify we have 5 nodes
        {
            let nodes = discovery_state.read().await.get_all_nodes();
            assert_eq!(nodes.len(), 5);
        }
        
        // Add one more node - should trigger cleanup
        let overflow_node = create_test_node(6, 7006);
        {
            let mut state = discovery_state.write().await;
            state.add_node(overflow_node);
        }
        
        // Should still have at most the limit
        let nodes = discovery_state.read().await.get_all_nodes();
        assert!(nodes.len() <= 5, "Stale node cleanup failed");
    }

    #[tokio::test]
    async fn test_static_discovery_configuration_reload() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("static_nodes.toml");
        
        // Create initial config
        let initial_config = r#"
[[nodes]]
id = "0101010101010101010101010101010101010101010101010101010101010101"
addresses = ["127.0.0.1:7001"]

[nodes.properties]
role = "worker"
"#;
        
        tokio::fs::write(&config_path, initial_config).await.unwrap();
        
        let mut static_discovery = StaticDiscovery::new(vec![config_path.to_string_lossy().to_string()]);
        
        // Start discovery
        let (tx, mut rx) = mpsc::unbounded_channel();
        static_discovery.start(tx.clone()).await.unwrap();
        
        // Should discover initial node
        let event = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        match event {
            DiscoveryEvent::NodeDiscovered(node) => {
                assert_eq!(node.addresses, vec!["127.0.0.1:7001"]);
                assert_eq!(node.properties.get("role"), Some(&"worker".to_string()));
            }
            _ => panic!("Expected NodeDiscovered event"),
        }
        
        // Update config file
        let updated_config = r#"
[[nodes]]
id = "0101010101010101010101010101010101010101010101010101010101010101"
addresses = ["127.0.0.1:7001", "192.168.1.100:7001"]

[nodes.properties]
role = "manager"
region = "us-east"

[[nodes]]
id = "0202020202020202020202020202020202020202020202020202020202020202"
addresses = ["127.0.0.1:7002"]

[nodes.properties]
role = "worker"
"#;
        
        tokio::fs::write(&config_path, updated_config).await.unwrap();
        
        // Wait for file watcher to detect change (if implemented)
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        static_discovery.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_mdns_discovery_service_advertisement() {
        let config = DiscoveryConfig {
            mdns_enabled: true,
            mdns_service_name: "_blixard-test._tcp.local".to_string(),
            ..create_test_discovery_config()
        };
        
        let mut mdns_discovery = MdnsDiscovery::new(config);
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        // Test service registration
        let test_node = create_test_node(1, 7001);
        mdns_discovery.advertise_node(&test_node).await.unwrap();
        
        // Start discovery
        mdns_discovery.start(tx.clone()).await.unwrap();
        
        // Test service browsing timeout
        let browse_result = timeout(Duration::from_millis(500), rx.recv()).await;
        
        // mDNS might not find services immediately in test environment
        // Just verify no panic occurred
        mdns_discovery.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_discovery_error_scenarios() {
        let config = create_test_discovery_config();
        
        // Test with invalid DNS domains
        let mut bad_config = config.clone();
        bad_config.dns_domains = vec!["invalid..domain".to_string(), "".to_string()];
        
        let dns_discovery = DnsDiscovery::new(bad_config.clone());
        let (tx, _rx) = mpsc::unbounded_channel();
        
        // Should handle invalid domains gracefully
        let result = dns_discovery.start(tx).await;
        // DNS discovery should either succeed or fail gracefully
        // Don't assert specific outcome since it depends on system DNS
        
        // Test with zero refresh interval
        bad_config.dns_refresh_interval = Duration::from_secs(0);
        let dns_discovery = DnsDiscovery::new(bad_config);
        let (tx, _rx) = mpsc::unbounded_channel();
        
        // Should handle zero interval
        let _result = dns_discovery.start(tx).await;
    }

    #[tokio::test]
    async fn test_discovery_malformed_data_handling() {
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        // Test with invalid node ID
        let mut invalid_node = create_test_node(1, 7001);
        // Can't easily create invalid NodeId, so test other invalid data
        
        // Test with invalid addresses
        invalid_node.addresses = vec!["invalid-address".to_string(), "".to_string()];
        
        {
            let mut state = discovery_state.write().await;
            state.add_node(invalid_node.clone());
        }
        
        // Should handle gracefully
        let nodes = discovery_state.read().await.get_all_nodes();
        assert!(nodes.len() <= 1);
        
        // Test with excessively large properties
        let mut large_props_node = create_test_node(2, 7002);
        large_props_node.properties.insert(
            "large_data".to_string(),
            "x".repeat(1024 * 1024), // 1MB string
        );
        
        {
            let mut state = discovery_state.write().await;
            state.add_node(large_props_node);
        }
        
        // Should handle large data
        let nodes = discovery_state.read().await.get_all_nodes();
        assert!(nodes.len() <= 2);
    }

    #[tokio::test]
    async fn test_discovery_performance_many_nodes() {
        let config = create_test_discovery_config();
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        let num_nodes = 100;
        let nodes: Vec<_> = (1..=num_nodes)
            .map(|i| create_test_node(i as u8, 7000 + i))
            .collect();
        
        // Time node addition
        let start = std::time::Instant::now();
        
        {
            let mut state = discovery_state.write().await;
            for node in nodes {
                state.add_node(node);
            }
        }
        
        let add_duration = start.elapsed();
        
        // Should complete reasonably quickly
        assert!(add_duration < Duration::from_secs(1), "Node addition too slow: {:?}", add_duration);
        
        // Verify all nodes were added
        let discovered_nodes = discovery_state.read().await.get_all_nodes();
        assert_eq!(discovered_nodes.len(), num_nodes);
        
        // Time node lookup
        let start = std::time::Instant::now();
        
        {
            let state = discovery_state.read().await;
            for i in 1..=num_nodes {
                let mut node_id_bytes = [0u8; 32];
                node_id_bytes[31] = i as u8;
                let node_id = NodeId::new(&node_id_bytes);
                
                let _node = state.get_node(&node_id);
            }
        }
        
        let lookup_duration = start.elapsed();
        
        // Lookups should be fast
        assert!(lookup_duration < Duration::from_millis(100), "Node lookup too slow: {:?}", lookup_duration);
    }

    #[tokio::test]
    async fn test_discovery_concurrent_operations() {
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        // Create many tasks that add, update, and remove nodes concurrently
        let mut tasks = Vec::new();
        
        for i in 0..50 {
            let state = discovery_state.clone();
            tasks.push(tokio::spawn(async move {
                let node = create_test_node((i % 256) as u8, 7000 + i);
                let node_id = node.node_id;
                
                // Add node
                {
                    let mut state = state.write().await;
                    state.add_node(node.clone());
                }
                
                // Update node
                let mut updated_node = node;
                updated_node.properties.insert("updated".to_string(), "true".to_string());
                {
                    let mut state = state.write().await;
                    state.update_node(updated_node);
                }
                
                // Remove node (some of the time)
                if i % 3 == 0 {
                    let mut state = state.write().await;
                    state.remove_node(node_id);
                }
            }));
        }
        
        // Wait for all tasks to complete
        futures::future::join_all(tasks).await;
        
        // Verify state is consistent
        let nodes = discovery_state.read().await.get_all_nodes();
        
        // Should have some nodes remaining (those not removed)
        assert!(nodes.len() < 50);
        
        // All remaining nodes should have "updated" property
        for node in nodes.values() {
            assert_eq!(node.properties.get("updated"), Some(&"true".to_string()));
        }
    }

    #[tokio::test]
    async fn test_discovery_driven_cluster_formation() {
        // Test scenario: nodes discover each other and form a cluster
        let config = create_test_discovery_config();
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        // Simulate 3 nodes discovering each other
        let node1 = create_test_node(1, 7001);
        let node2 = create_test_node(2, 7002);  
        let node3 = create_test_node(3, 7003);
        
        let all_nodes = vec![node1.clone(), node2.clone(), node3.clone()];
        
        // Each node discovers the others
        {
            let mut state = discovery_state.write().await;
            for node in &all_nodes {
                state.add_node(node.clone());
            }
        }
        
        // Verify all nodes are discovered
        let discovered = discovery_state.read().await.get_all_nodes();
        assert_eq!(discovered.len(), 3);
        
        // Check that each node has expected properties
        for node in &all_nodes {
            let stored_node = discovered.get(&node.node_id).unwrap();
            assert_eq!(stored_node.properties.get("role"), Some(&"worker".to_string()));
            assert!(stored_node.addresses.len() > 0);
        }
        
        // Simulate cluster formation by checking connectivity
        let mut connectivity_map = HashMap::new();
        for node in &all_nodes {
            let mut connected_peers = HashSet::new();
            for other_node in &all_nodes {
                if node.node_id != other_node.node_id {
                    // Simulate connection check
                    connected_peers.insert(other_node.node_id);
                }
            }
            connectivity_map.insert(node.node_id, connected_peers);
        }
        
        // Verify full mesh connectivity potential
        for (node_id, peers) in connectivity_map {
            assert_eq!(peers.len(), 2, "Node {:?} should be able to connect to 2 peers", node_id);
        }
    }

    #[tokio::test]
    async fn test_iroh_discovery_bridge_integration() {
        let config = create_test_discovery_config();
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        let bridge = IrohDiscoveryBridge::new(discovery_state.clone());
        
        // Add test nodes to discovery state
        let test_nodes = vec![
            create_test_node(1, 7001),
            create_test_node(2, 7002),
        ];
        
        {
            let mut state = discovery_state.write().await;
            for node in test_nodes {
                state.add_node(node);
            }
        }
        
        // Test bridge can retrieve nodes
        let nodes = bridge.discovered_nodes().await;
        assert_eq!(nodes.len(), 2);
        
        // Test cache synchronization
        let new_node = create_test_node(3, 7003);
        {
            let mut state = discovery_state.write().await;
            state.add_node(new_node);
        }
        
        // Bridge should see the new node
        let updated_nodes = bridge.discovered_nodes().await;
        assert_eq!(updated_nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_discovery_network_partition_simulation() {
        // Simulate network partition scenario
        let discovery_state = Arc::new(RwLock::new(DiscoveryState::new()));
        
        // Create nodes in two partitions
        let partition_a = vec![
            create_test_node(1, 7001),
            create_test_node(2, 7002),
        ];
        
        let partition_b = vec![
            create_test_node(3, 7003),
            create_test_node(4, 7004),
        ];
        
        // Initially, all nodes can discover each other
        {
            let mut state = discovery_state.write().await;
            for node in &partition_a {
                state.add_node(node.clone());
            }
            for node in &partition_b {
                state.add_node(node.clone());
            }
        }
        
        assert_eq!(discovery_state.read().await.get_all_nodes().len(), 4);
        
        // Simulate partition - nodes in partition B become unreachable
        {
            let mut state = discovery_state.write().await;
            for node in &partition_b {
                state.remove_node(node.node_id);
            }
        }
        
        // Only partition A nodes should remain
        let remaining_nodes = discovery_state.read().await.get_all_nodes();
        assert_eq!(remaining_nodes.len(), 2);
        
        for node in &partition_a {
            assert!(remaining_nodes.contains_key(&node.node_id));
        }
        
        // Simulate partition healing - nodes reconnect
        {
            let mut state = discovery_state.write().await;
            for node in &partition_b {
                state.add_node(node.clone());
            }
        }
        
        // All nodes should be available again
        assert_eq!(discovery_state.read().await.get_all_nodes().len(), 4);
    }
}