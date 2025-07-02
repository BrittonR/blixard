//! Demonstration of distributed document storage using iroh-docs
//!
//! This example shows how to:
//! 1. Create distributed documents
//! 2. Share documents between nodes using tickets
//! 3. Synchronize data automatically
//! 4. Monitor document events

use anyhow::Result;
use blixard_core::iroh_transport_v3::IrohTransportV3;
use blixard_core::iroh_transport_v2::DocumentType;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,iroh=debug"))
        )
        .init();

    info!("Starting iroh-docs distributed document demo");

    // Create temporary directories for two nodes
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    info!("Creating two Iroh transport instances with iroh-docs");
    
    // Create two transport instances
    let transport1 = Arc::new(IrohTransportV3::new(1, temp_dir1.path()).await?);
    let transport2 = Arc::new(IrohTransportV3::new(2, temp_dir2.path()).await?);

    // Get node addresses
    let addr1 = transport1.node_addr().await?;
    let addr2 = transport2.node_addr().await?;
    
    info!("Node 1 address: {}", addr1.node_id);
    info!("Node 2 address: {}", addr2.node_id);

    // Demo 1: Create and share a cluster configuration document
    info!("\n=== Demo 1: Cluster Configuration Document ===");
    
    // Create cluster config document on node 1
    transport1.create_or_join_doc(DocumentType::ClusterConfig, true).await?;
    
    // Write some configuration data
    transport1.write_to_doc(DocumentType::ClusterConfig, "cluster_name", b"production").await?;
    transport1.write_to_doc(DocumentType::ClusterConfig, "version", b"1.0.0").await?;
    transport1.write_to_doc(DocumentType::ClusterConfig, "leader", b"node-1").await?;
    
    info!("Node 1: Written cluster configuration");
    
    // Get a sharing ticket
    let ticket = transport1.get_doc_ticket(DocumentType::ClusterConfig).await?;
    info!("Node 1: Generated sharing ticket: {}", ticket);
    
    // Join the document on node 2
    transport2.join_doc_from_ticket(&ticket, DocumentType::ClusterConfig).await?;
    info!("Node 2: Joined cluster config document");
    
    // Subscribe to document events on node 2
    transport2.subscribe_to_doc_events(DocumentType::ClusterConfig).await?;
    
    // Give some time for initial sync
    sleep(Duration::from_millis(500)).await;
    
    // Try to read the data on node 2
    match transport2.read_from_doc(DocumentType::ClusterConfig, "cluster_name").await {
        Ok(data) => {
            info!("Node 2: Read cluster_name = {}", String::from_utf8_lossy(&data));
        }
        Err(e) => {
            warn!("Node 2: Failed to read cluster_name (may need more sync time): {}", e);
        }
    }

    // Demo 2: VM Images catalog
    info!("\n=== Demo 2: VM Images Catalog ===");
    
    // Create VM images document on node 2 this time
    transport2.create_or_join_doc(DocumentType::VmImages, true).await?;
    
    // Add some VM images
    transport2.write_to_doc(DocumentType::VmImages, "ubuntu-22.04", b"sha256:abc123...").await?;
    transport2.write_to_doc(DocumentType::VmImages, "debian-12", b"sha256:def456...").await?;
    transport2.write_to_doc(DocumentType::VmImages, "alpine-3.18", b"sha256:ghi789...").await?;
    
    info!("Node 2: Added VM images to catalog");
    
    // Get ticket and share with node 1
    let vm_ticket = transport2.get_doc_ticket(DocumentType::VmImages).await?;
    transport1.join_doc_from_ticket(&vm_ticket, DocumentType::VmImages).await?;
    
    info!("Node 1: Joined VM images catalog");
    
    // Demo 3: Real-time updates
    info!("\n=== Demo 3: Real-time Updates ===");
    
    // Node 1 updates the cluster config
    transport1.write_to_doc(DocumentType::ClusterConfig, "leader", b"node-2").await?;
    transport1.write_to_doc(DocumentType::ClusterConfig, "last_election", b"2024-01-25T10:30:00Z").await?;
    
    info!("Node 1: Updated cluster configuration (new leader)");
    
    // Give time for sync
    sleep(Duration::from_millis(500)).await;
    
    // Node 2 should see the updates
    match transport2.read_from_doc(DocumentType::ClusterConfig, "leader").await {
        Ok(data) => {
            info!("Node 2: New leader = {}", String::from_utf8_lossy(&data));
        }
        Err(e) => {
            warn!("Node 2: Failed to read updated leader: {}", e);
        }
    }

    // Demo 4: Metrics document
    info!("\n=== Demo 4: Distributed Metrics ===");
    
    // Both nodes can write to a shared metrics document
    transport1.create_or_join_doc(DocumentType::Metrics, true).await?;
    let metrics_ticket = transport1.get_doc_ticket(DocumentType::Metrics).await?;
    transport2.join_doc_from_ticket(&metrics_ticket, DocumentType::Metrics).await?;
    
    // Node 1 writes its metrics
    transport1.write_to_doc(DocumentType::Metrics, "node1_cpu_usage", b"45.2").await?;
    transport1.write_to_doc(DocumentType::Metrics, "node1_memory_usage", b"2048").await?;
    
    // Node 2 writes its metrics
    transport2.write_to_doc(DocumentType::Metrics, "node2_cpu_usage", b"67.8").await?;
    transport2.write_to_doc(DocumentType::Metrics, "node2_memory_usage", b"3072").await?;
    
    info!("Both nodes have written their metrics");
    
    // Give time for full sync
    sleep(Duration::from_secs(1)).await;
    
    // Each node can read all metrics
    info!("\nReading all metrics from Node 1:");
    for key in ["node1_cpu_usage", "node1_memory_usage", "node2_cpu_usage", "node2_memory_usage"] {
        match transport1.read_from_doc(DocumentType::Metrics, key).await {
            Ok(data) => {
                info!("  {} = {}", key, String::from_utf8_lossy(&data));
            }
            Err(e) => {
                warn!("  Failed to read {}: {}", key, e);
            }
        }
    }

    // Demo 5: Peer-to-peer messaging (separate from documents)
    info!("\n=== Demo 5: Direct P2P Messaging ===");
    
    // Send a direct message from node 1 to node 2
    match transport1.send_to_peer(&addr2, DocumentType::ClusterConfig, b"Hello from Node 1!").await {
        Ok(_) => info!("Successfully sent P2P message"),
        Err(e) => warn!("Failed to send P2P message: {}", e),
    }

    // Keep the demo running for a bit to observe any late sync events
    info!("\nDemo running, press Ctrl+C to exit...");
    sleep(Duration::from_secs(5)).await;

    // Shutdown
    info!("\nShutting down...");
    Arc::try_unwrap(transport1)
        .unwrap_or_else(|_| panic!("Failed to unwrap transport1"))
        .shutdown().await?;
    Arc::try_unwrap(transport2)
        .unwrap_or_else(|_| panic!("Failed to unwrap transport2"))
        .shutdown().await?;

    info!("Demo completed successfully!");
    Ok(())
}