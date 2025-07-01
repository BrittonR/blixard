//! Example demonstrating the discovery module usage

use blixard_core::discovery::{DiscoveryConfig, DiscoveryManager, DiscoveryEvent};
use iroh::NodeId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting discovery demo");

    // Create a sample node ID for demonstration
    let our_node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
    let our_address: SocketAddr = "127.0.0.1:7000".parse()?;

    // Configure static nodes for discovery
    let mut static_nodes = HashMap::new();
    
    // Add some example static nodes
    let node2_id = NodeId::from_bytes(&[2u8; 32]).unwrap();
    let node3_id = NodeId::from_bytes(&[3u8; 32]).unwrap();
    
    static_nodes.insert(
        node2_id.to_string(),
        vec!["127.0.0.1:7001".to_string(), "192.168.1.100:7001".to_string()],
    );
    
    static_nodes.insert(
        node3_id.to_string(),
        vec!["127.0.0.1:7002".to_string()],
    );

    // Create discovery configuration
    let config = DiscoveryConfig {
        enable_static: true,
        enable_dns: false,  // DNS requires actual DNS records
        enable_mdns: true,
        refresh_interval: 30,
        node_stale_timeout: 300,
        static_nodes,
        dns_domains: vec!["_iroh._tcp.example.com".to_string()],
        mdns_service_name: "_blixard-iroh._tcp.local".to_string(),
    };

    // Create and configure discovery manager
    let mut manager = DiscoveryManager::new(config);
    manager.configure_for_node(our_node_id, vec![our_address]);

    // Subscribe to discovery events
    let mut event_receiver = manager.subscribe().await;

    // Start discovery
    info!("Starting discovery manager");
    manager.start().await?;

    // Spawn a task to handle discovery events
    let event_task = tokio::spawn(async move {
        info!("Listening for discovery events");
        while let Some(event) = event_receiver.recv().await {
            match event {
                DiscoveryEvent::NodeDiscovered(info) => {
                    info!(
                        "Node discovered: {} at {:?}",
                        info.node_id,
                        info.addresses
                    );
                    if let Some(name) = &info.name {
                        info!("  Name: {}", name);
                    }
                    for (key, value) in &info.metadata {
                        info!("  {}: {}", key, value);
                    }
                }
                DiscoveryEvent::NodeRemoved(node_id) => {
                    info!("Node removed: {}", node_id);
                }
                DiscoveryEvent::NodeUpdated(info) => {
                    info!(
                        "Node updated: {} at {:?}",
                        info.node_id,
                        info.addresses
                    );
                }
            }
        }
    });

    // Let discovery run for a while
    info!("Discovery running, waiting for events...");
    sleep(Duration::from_secs(5)).await;

    // Get all discovered nodes
    let nodes = manager.get_nodes().await;
    info!("Total discovered nodes: {}", nodes.len());
    for node in &nodes {
        info!("  - {} at {:?}", node.node_id, node.addresses);
    }

    // Check specific node
    if let Some(node_info) = manager.get_node(&node2_id).await {
        info!("Found node2: {:?}", node_info);
    }

    // Keep running for a bit more
    sleep(Duration::from_secs(5)).await;

    // Stop discovery
    info!("Stopping discovery manager");
    manager.stop().await?;

    // Cancel event task
    event_task.abort();

    info!("Discovery demo complete");
    Ok(())
}