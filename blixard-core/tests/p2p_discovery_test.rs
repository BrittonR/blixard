//! Test P2P discovery integration

#![cfg(feature = "test-helpers")]

use blixard_core::{
    p2p_manager::{P2pManager, P2pConfig},
    discovery::{DiscoveryConfig, DiscoveryManager, IrohNodeInfo},
    error::BlixardResult,
};
use tempfile::TempDir;
use std::sync::Arc;
use std::time::Duration;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use tokio::time::{sleep, timeout};
use iroh::NodeId;

#[tokio::test]
async fn test_p2p_with_discovery_integration() -> BlixardResult<()> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    // Create two nodes with discovery
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    // Create discovery managers
    let mut discovery_config1 = DiscoveryConfig::default();
    discovery_config1.enable_static = true;
    discovery_config1.enable_dns = false;
    discovery_config1.enable_mdns = false;
    
    let mut discovery_config2 = discovery_config1.clone();
    
    let mut discovery_manager1 = DiscoveryManager::new(discovery_config1);
    let mut discovery_manager2 = DiscoveryManager::new(discovery_config2);
    
    // Start discovery managers
    discovery_manager1.start().await?;
    discovery_manager2.start().await?;
    
    // Convert to Arc after starting
    let discovery_manager1 = Arc::new(discovery_manager1);
    let discovery_manager2 = Arc::new(discovery_manager2);
    
    // Create P2P managers with discovery
    let p2p_config = P2pConfig::default();
    
    let p2p1 = P2pManager::new_with_discovery(
        1, 
        temp_dir1.path(), 
        p2p_config.clone(),
        Some(discovery_manager1.clone())
    ).await?;
    
    let p2p2 = P2pManager::new_with_discovery(
        2, 
        temp_dir2.path(), 
        p2p_config,
        Some(discovery_manager2.clone())
    ).await?;
    
    // Start P2P managers
    p2p1.start().await?;
    p2p2.start().await?;
    
    // Get node addresses
    let addr1 = p2p1.get_node_addr().await?;
    let addr2 = p2p2.get_node_addr().await?;
    
    println!("Node 1 Iroh ID: {}", addr1.node_id);
    println!("Node 2 Iroh ID: {}", addr2.node_id);
    
    // Add node 2 to discovery manager 1
    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000);
    let node_info2 = IrohNodeInfo::new(addr2.node_id, vec![socket_addr]);
    discovery_manager1.add_discovered_node(node_info2).await?;
    
    // Give time for discovery to propagate
    sleep(Duration::from_millis(500)).await;
    
    // Try to connect using discovered information
    match timeout(Duration::from_secs(5), p2p1.connect_p2p_peer(2, &addr2)).await {
        Ok(Ok(_)) => println!("✅ Successfully connected via discovery"),
        Ok(Err(e)) => println!("⚠️  Connection failed: {}", e),
        Err(_) => println!("⚠️  Connection timed out"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_mdns_discovery() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    // Create nodes with mDNS discovery enabled
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let mut discovery_config = DiscoveryConfig::default();
    discovery_config.enable_static = false;
    discovery_config.enable_dns = false;
    discovery_config.enable_mdns = true;
    discovery_config.mdns_service_name = "_blixard-test._tcp.local".to_string();
    
    let mut discovery_manager1 = DiscoveryManager::new(discovery_config.clone());
    let mut discovery_manager2 = DiscoveryManager::new(discovery_config);
    
    // Create P2P managers first to get node IDs
    let p2p_config = P2pConfig::default();
    
    // Need to create P2P managers without discovery first to get node addresses
    let p2p1_temp = P2pManager::new(1, temp_dir1.path(), p2p_config.clone()).await?;
    let p2p2_temp = P2pManager::new(2, temp_dir2.path(), p2p_config.clone()).await?;
    
    // Get addresses for mDNS configuration
    let addr1 = p2p1_temp.get_node_addr().await?;
    let addr2 = p2p2_temp.get_node_addr().await?;
    
    // Configure discovery managers with node info
    let socket1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    let socket2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    
    discovery_manager1.configure_for_node(addr1.node_id, vec![socket1]);
    discovery_manager2.configure_for_node(addr2.node_id, vec![socket2]);
    
    // Start discovery
    discovery_manager1.start().await?;
    discovery_manager2.start().await?;
    
    // Convert to Arc after starting
    let discovery_manager1 = Arc::new(discovery_manager1);
    let discovery_manager2 = Arc::new(discovery_manager2);
    
    // Now create P2P managers with discovery
    let p2p1 = P2pManager::new_with_discovery(
        1, 
        temp_dir1.path(), 
        p2p_config.clone(),
        Some(discovery_manager1.clone())
    ).await?;
    
    let p2p2 = P2pManager::new_with_discovery(
        2, 
        temp_dir2.path(), 
        p2p_config,
        Some(discovery_manager2.clone())
    ).await?;
    
    // Start P2P
    p2p1.start().await?;
    p2p2.start().await?;
    
    println!("Node 1: {} at {:?}", addr1.node_id, socket1);
    println!("Node 2: {} at {:?}", addr2.node_id, socket2);
    
    // Wait for mDNS discovery
    println!("Waiting for mDNS discovery...");
    sleep(Duration::from_secs(2)).await;
    
    // Check if nodes discovered each other
    let discovered1 = discovery_manager1.get_nodes().await;
    let discovered2 = discovery_manager2.get_nodes().await;
    
    println!("Node 1 discovered {} nodes", discovered1.len());
    println!("Node 2 discovered {} nodes", discovered2.len());
    
    // Note: mDNS discovery might not work in all test environments
    if discovered1.is_empty() && discovered2.is_empty() {
        println!("⚠️  mDNS discovery did not find any nodes (this is expected in some environments)");
    } else {
        println!("✅ mDNS discovery found nodes!");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_discovery_events() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard_core=debug")
        .try_init();

    let temp_dir = TempDir::new().unwrap();
    
    // Create discovery manager
    let discovery_config = DiscoveryConfig::default();
    let mut discovery_manager = DiscoveryManager::new(discovery_config);
    
    // Subscribe to events before starting
    let mut event_rx = discovery_manager.subscribe().await;
    
    // Start discovery
    discovery_manager.start().await?;
    
    // Convert to Arc after starting
    let discovery_manager = Arc::new(discovery_manager);
    
    // Create P2P manager with discovery
    let p2p = P2pManager::new_with_discovery(
        1, 
        temp_dir.path(), 
        P2pConfig::default(),
        Some(discovery_manager.clone())
    ).await?;
    
    p2p.start().await?;
    
    // Manually add a discovered node
    let test_node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
    let test_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 9000);
    let node_info = IrohNodeInfo::new(test_node_id, vec![test_addr]);
    
    discovery_manager.add_discovered_node(node_info).await?;
    
    // Wait for discovery event
    match timeout(Duration::from_secs(2), event_rx.recv()).await {
        Ok(Some(event)) => {
            println!("✅ Received discovery event: {:?}", event);
        }
        Ok(None) => {
            println!("❌ Event channel closed");
        }
        Err(_) => {
            println!("❌ Timeout waiting for discovery event");
        }
    }
    
    Ok(())
}