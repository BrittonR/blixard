//! Example demonstrating a 3-node cluster using discovery mechanisms
//!
//! This example shows how to:
//! - Start a bootstrap node with static seed configuration
//! - Have nodes discover each other via static discovery
//! - Simulate DNS discovery for additional nodes
//! - Properly shutdown the cluster

use blixard_core::{
    config_v2::{ConfigBuilder, DiscoveryConfigBuilder},
    discovery::{DiscoveryConfig, IrohNodeInfo},
    error::BlixardResult,
    node::Node,
    iroh_types::{JoinClusterRequest, JoinClusterResponse},
    transport::iroh_client::IrohClusterServiceClient,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=debug".parse().unwrap())
                .add_directive("discovery_cluster=info".parse().unwrap()),
        )
        .init();

    info!("Starting discovery cluster example");

    // Create data directories
    let base_dir = tempfile::tempdir()?;
    let node1_dir = base_dir.path().join("node1");
    let node2_dir = base_dir.path().join("node2");
    let node3_dir = base_dir.path().join("node3");
    
    std::fs::create_dir_all(&node1_dir)?;
    std::fs::create_dir_all(&node2_dir)?;
    std::fs::create_dir_all(&node3_dir)?;

    // Node 1: Bootstrap node with static discovery
    info!("Starting bootstrap node (Node 1)");
    let node1_config = ConfigBuilder::new()
        .node_id(1)
        .bind_address("127.0.0.1:7001".to_string())
        .data_dir(node1_dir.to_str().unwrap())
        .vm_backend("mock")
        .discovery(
            DiscoveryConfigBuilder::new()
                .enable_static(true)
                .enable_dns(false)
                .enable_mdns(true)
                .mdns_service_name("_blixard-demo._tcp.local")
                .build()
        )
        .build()?;

    let mut node1 = Node::new(node1_config.to_node_config());
    let node1_state = node1.shared();
    node1.initialize().await?;
    
    // Get Node 1's Iroh information for discovery
    let node1_iroh_info = if let Some(p2p_manager) = node1_state.get_p2p_manager().await {
        let node_addr = p2p_manager.get_node_addr().await?;
        Some((node_addr.node_id, node_addr))
    } else {
        None
    };
    
    info!("Node 1 started with Iroh ID: {:?}", node1_iroh_info.as_ref().map(|(id, _)| id));

    // Give node 1 time to fully start
    sleep(Duration::from_secs(1)).await;

    // Node 2: Uses static discovery to find Node 1
    info!("Starting Node 2 with static discovery");
    
    // Create static discovery configuration for Node 2
    let mut static_nodes = HashMap::new();
    if let Some((iroh_id, _)) = &node1_iroh_info {
        static_nodes.insert(
            format!("{}", iroh_id),
            vec!["127.0.0.1:7001".to_string()],
        );
    }
    
    let node2_config = ConfigBuilder::new()
        .node_id(2)
        .bind_address("127.0.0.1:7002".to_string())
        .data_dir(node2_dir.to_str().unwrap())
        .vm_backend("mock")
        .join_address(Some("127.0.0.1:7001".to_string()))
        .discovery(
            DiscoveryConfigBuilder::new()
                .enable_static(true)
                .enable_dns(false)
                .enable_mdns(true)
                .mdns_service_name("_blixard-demo._tcp.local")
                .static_nodes(static_nodes.clone())
                .build()
        )
        .build()?;

    let mut node2 = Node::new(node2_config.to_node_config());
    let node2_state = node2.shared();
    node2.initialize().await?;
    
    info!("Node 2 started and joining cluster");

    // Wait for Node 2 to join
    sleep(Duration::from_secs(2)).await;

    // Node 3: Simulates DNS discovery
    info!("Starting Node 3 with simulated DNS discovery");
    
    // In a real deployment, DNS would return SRV records
    // Here we simulate it with static configuration
    let node3_config = ConfigBuilder::new()
        .node_id(3)
        .bind_address("127.0.0.1:7003".to_string())
        .data_dir(node3_dir.to_str().unwrap())
        .vm_backend("mock")
        .join_address(Some("127.0.0.1:7001".to_string()))
        .discovery(
            DiscoveryConfigBuilder::new()
                .enable_static(true)
                .enable_dns(true)
                .enable_mdns(true)
                .mdns_service_name("_blixard-demo._tcp.local")
                .dns_domains(vec!["_blixard._tcp.example.com".to_string()])
                .static_nodes(static_nodes) // Fallback to static for demo
                .build()
        )
        .build()?;

    let mut node3 = Node::new(node3_config.to_node_config());
    let node3_state = node3.shared();
    node3.initialize().await?;
    
    info!("Node 3 started and joining cluster");

    // Wait for cluster to stabilize
    sleep(Duration::from_secs(3)).await;

    // Check cluster status from each node's perspective
    info!("Checking cluster status...");
    
    for (id, state) in vec![
        (1, &node1_state),
        (2, &node2_state),
        (3, &node3_state),
    ] {
        match state.get_cluster_status().await {
            Ok((leader_id, peers, term)) => {
                info!(
                    "Node {} sees: leader={}, peers={:?}, term={}",
                    id, leader_id, peers, term
                );
            }
            Err(e) => {
                warn!("Node {} failed to get cluster status: {}", id, e);
            }
        }
    }

    // Demonstrate discovery information
    info!("Discovery information:");
    
    // Show discovered nodes from Node 1's discovery manager
    if let Some(p2p_manager) = node1_state.get_p2p_manager().await {
        // In a real implementation, we'd access the discovery manager through P2P manager
        info!("Node 1 P2P manager active");
    }

    // Demonstrate mDNS discovery working
    info!("Waiting for mDNS discovery to propagate...");
    sleep(Duration::from_secs(2)).await;

    // Create a task to demonstrate the cluster is working
    info!("Creating a test task to verify cluster operation");
    if let Some(p2p_manager) = node1_state.get_p2p_manager().await {
        let node_addr = p2p_manager.get_node_addr().await?;
        
        // Create client to submit task
        let endpoint = iroh::Endpoint::builder()
            .bind()
            .await?;
        
        let client = IrohClusterServiceClient::new(Arc::new(endpoint), node_addr);
        
        // Submit a simple task
        let task = blixard_core::transport::iroh_cluster_service::Task {
            id: "test-task-1".to_string(),
            name: "Test Task".to_string(),
            task_type: "test".to_string(),
            payload: vec![],
            priority: 1,
            created_at: 0,
            status: "pending".to_string(),
            assigned_to: None,
            result: None,
        };
        
        match client.propose_task(task).await {
            Ok(response) => {
                info!("Task submitted successfully: {:?}", response);
            }
            Err(e) => {
                warn!("Failed to submit task: {}", e);
            }
        }
    }

    // Let the cluster run for a bit
    info!("Cluster running... Press Ctrl+C to stop");
    sleep(Duration::from_secs(10)).await;

    // Proper shutdown
    info!("Shutting down cluster");
    
    // Stop nodes in reverse order
    node3.stop().await?;
    info!("Node 3 stopped");
    
    node2.stop().await?;
    info!("Node 2 stopped");
    
    node1.stop().await?;
    info!("Node 1 stopped");

    info!("Discovery cluster example completed successfully");
    Ok(())
}