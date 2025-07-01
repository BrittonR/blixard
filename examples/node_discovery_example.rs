//! Example demonstrating automatic node discovery
//!
//! This example shows how to:
//! 1. Create a node with discovery enabled
//! 2. Use node registry files for peer discovery
//! 3. Automatically connect to discovered peers

use blixard::BlixardError;
use blixard_core::{NodeConfig, node::Node};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("=== Blixard Node Discovery Example ===\n");
    
    // Example 1: Bootstrap node that creates a registry file
    println!("1. Starting bootstrap node...");
    let bootstrap_config = NodeConfig {
        id: 1,
        data_dir: "./data/node1".to_string(),
        bind_addr: "127.0.0.1:7001".parse::<SocketAddr>()?,
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: Some(blixard_core::transport::config::TransportConfig {
            transport_type: blixard_core::transport::config::TransportType::Iroh,
            ..Default::default()
        }),
        topology: Default::default(),
    };
    
    let mut bootstrap_node = Node::new(bootstrap_config);
    bootstrap_node.initialize().await?;
    bootstrap_node.start().await?;
    
    // Get P2P node address and save to registry
    if let Some(p2p_addr) = bootstrap_node.shared().get_p2p_node_addr().await {
        let registry_entry = blixard::node_discovery::NodeRegistryEntry {
            cluster_node_id: 1,
            iroh_node_id: base64::encode(p2p_addr.node_id().as_bytes()),
            direct_addresses: p2p_addr.direct_addresses()
                .map(|addr| addr.to_string())
                .collect(),
            relay_url: p2p_addr.relay_url().map(|url| url.to_string()),
            address: "127.0.0.1:7001".to_string(),
        };
        
        // Save registry to file
        let registry_path = "./data/node1-registry.json";
        blixard::node_discovery::save_node_registry(registry_path, &registry_entry).await?;
        println!("Bootstrap node registry saved to: {}", registry_path);
    }
    
    // Example 2: Joining node that discovers bootstrap via registry
    println!("\n2. Starting joining node with discovery...");
    
    // Set environment variable for discovery
    std::env::set_var("BLIXARD_NODE_REGISTRIES", "./data/node1-registry.json");
    
    let joining_config = NodeConfig {
        id: 2,
        data_dir: "./data/node2".to_string(),
        bind_addr: "127.0.0.1:7002".parse::<SocketAddr>()?,
        join_addr: Some("./data/node1-registry.json".to_string()), // Use registry path
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: Some(blixard_core::transport::config::TransportConfig {
            transport_type: blixard_core::transport::config::TransportType::Iroh,
            ..Default::default()
        }),
        topology: Default::default(),
    };
    
    let mut joining_node = Node::new(joining_config);
    joining_node.initialize().await?;
    
    // Discovery manager will be automatically created and started
    // It will discover node1 from the registry and auto-connect
    
    joining_node.start().await?;
    
    // Give time for discovery and connection
    sleep(Duration::from_secs(2)).await;
    
    // Check discovered peers
    if let Some(discovery_manager) = joining_node.shared()
        .get_discovery_manager::<blixard::discovery_manager::DiscoveryManager>()
        .await 
    {
        let peers = discovery_manager.get_peers().await;
        println!("\nDiscovered {} peers:", peers.len());
        for peer in peers {
            println!("  - Node {}: {} (source: {})", 
                peer.node_id, 
                peer.direct_addresses.join(", "),
                peer.source
            );
        }
    }
    
    // Example 3: Static discovery file
    println!("\n3. Creating static discovery file...");
    
    // Create a static discovery file with multiple peers
    let discovery_peers = vec![
        blixard::discovery_manager::DiscoveredPeer {
            node_id: 1,
            iroh_node_id: "dummy_node_id_1".to_string(),
            direct_addresses: vec!["127.0.0.1:7001".to_string()],
            relay_url: Some("https://relay.iroh.network".to_string()),
            source: "static".to_string(),
            discovered_at: std::time::SystemTime::now(),
        },
        blixard::discovery_manager::DiscoveredPeer {
            node_id: 3,
            iroh_node_id: "dummy_node_id_3".to_string(),
            direct_addresses: vec!["192.168.1.10:7003".to_string()],
            relay_url: None,
            source: "static".to_string(),
            discovered_at: std::time::SystemTime::now(),
        },
    ];
    
    let static_file = "./data/node2/discovery-peers.json";
    std::fs::create_dir_all("./data/node2")?;
    std::fs::write(static_file, serde_json::to_string_pretty(&discovery_peers)?)?;
    println!("Static discovery file created at: {}", static_file);
    
    // The discovery manager will automatically pick up this file on the next refresh
    
    println!("\n4. Discovery sources:");
    println!("   - Join address (node registry)");
    println!("   - Environment variable BLIXARD_NODE_REGISTRIES");  
    println!("   - Static file at {data_dir}/discovery-peers.json");
    
    // Keep nodes running for a bit
    println!("\nNodes running with discovery enabled. Press Ctrl+C to exit.");
    sleep(Duration::from_secs(60)).await;
    
    // Cleanup
    bootstrap_node.stop().await?;
    joining_node.stop().await?;
    
    Ok(())
}