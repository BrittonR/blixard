//! Multi-writer distributed document example using iroh-docs
//!
//! This example demonstrates:
//! 1. Multiple nodes writing to the same document
//! 2. Conflict resolution (last-writer-wins per key)
//! 3. Document event monitoring
//! 4. Persistent storage across restarts

use anyhow::Result;
use blixard_core::iroh_transport_v2::DocumentType;
use blixard_core::iroh_transport_v3::IrohTransportV3;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, Duration};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

/// Simulate a node that periodically writes status updates
async fn run_status_writer(
    node_id: u64,
    transport: Arc<IrohTransportV3>,
    mut shutdown: mpsc::Receiver<()>,
) {
    let mut ticker = interval(Duration::from_secs(2));
    let mut counter = 0u64;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                counter += 1;

                // Write node status
                let status_key = format!("node_{}_status", node_id);
                let status_value = format!("online,counter={}", counter);

                if let Err(e) = transport.write_to_doc(
                    DocumentType::ClusterConfig,
                    &status_key,
                    status_value.as_bytes()
                ).await {
                    warn!("Node {}: Failed to write status: {}", node_id, e);
                } else {
                    debug!("Node {}: Wrote status update #{}", node_id, counter);
                }

                // Write timestamp
                let ts_key = format!("node_{}_last_seen", node_id);
                let ts_value = chrono::Utc::now().to_rfc3339();

                if let Err(e) = transport.write_to_doc(
                    DocumentType::ClusterConfig,
                    &ts_key,
                    ts_value.as_bytes()
                ).await {
                    warn!("Node {}: Failed to write timestamp: {}", node_id, e);
                }
            }
            _ = shutdown.recv() => {
                info!("Node {}: Shutting down status writer", node_id);
                break;
            }
        }
    }
}

/// Monitor and display the cluster status from all nodes
async fn run_cluster_monitor(
    transport: Arc<IrohTransportV3>,
    node_ids: Vec<u64>,
    mut shutdown: mpsc::Receiver<()>,
) {
    let mut ticker = interval(Duration::from_secs(3));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                info!("\n=== Cluster Status ===");

                for node_id in &node_ids {
                    let status_key = format!("node_{}_status", node_id);
                    let ts_key = format!("node_{}_last_seen", node_id);

                    let status = match transport.read_from_doc(
                        DocumentType::ClusterConfig,
                        &status_key
                    ).await {
                        Ok(data) => String::from_utf8_lossy(&data).to_string(),
                        Err(_) => "unknown".to_string(),
                    };

                    let last_seen = match transport.read_from_doc(
                        DocumentType::ClusterConfig,
                        &ts_key
                    ).await {
                        Ok(data) => String::from_utf8_lossy(&data).to_string(),
                        Err(_) => "never".to_string(),
                    };

                    info!("Node {}: {} (last seen: {})", node_id, status, last_seen);
                }
                info!("==================\n");
            }
            _ = shutdown.recv() => {
                info!("Shutting down cluster monitor");
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,blixard_core=debug")),
        )
        .init();

    info!("Starting multi-writer distributed document demo");

    // Use persistent directories to test persistence
    let base_dir = PathBuf::from("/tmp/blixard-docs-demo");
    std::fs::create_dir_all(&base_dir)?;

    let dir1 = base_dir.join("node1");
    let dir2 = base_dir.join("node2");
    let dir3 = base_dir.join("node3");

    // Create directories
    std::fs::create_dir_all(&dir1)?;
    std::fs::create_dir_all(&dir2)?;
    std::fs::create_dir_all(&dir3)?;

    info!("Creating three Iroh transport instances");

    // Create three transport instances
    let transport1 = Arc::new(IrohTransportV3::new(1, &dir1).await?);
    let transport2 = Arc::new(IrohTransportV3::new(2, &dir2).await?);
    let transport3 = Arc::new(IrohTransportV3::new(3, &dir3).await?);

    // Get node addresses
    let addr1 = transport1.node_addr().await?;
    let addr2 = transport2.node_addr().await?;
    let addr3 = transport3.node_addr().await?;

    info!("Node 1: {}", addr1.node_id);
    info!("Node 2: {}", addr2.node_id);
    info!("Node 3: {}", addr3.node_id);

    // Create shared cluster config document on node 1
    transport1
        .create_or_join_doc(DocumentType::ClusterConfig, true)
        .await?;

    // Get sharing ticket
    let ticket = transport1
        .get_doc_ticket(DocumentType::ClusterConfig)
        .await?;
    info!("Sharing ticket: {}", ticket);

    // Join document on other nodes
    transport2
        .join_doc_from_ticket(&ticket, DocumentType::ClusterConfig)
        .await?;
    transport3
        .join_doc_from_ticket(&ticket, DocumentType::ClusterConfig)
        .await?;

    info!("All nodes have joined the cluster config document");

    // Subscribe to events on all nodes
    transport1
        .subscribe_to_doc_events(DocumentType::ClusterConfig)
        .await?;
    transport2
        .subscribe_to_doc_events(DocumentType::ClusterConfig)
        .await?;
    transport3
        .subscribe_to_doc_events(DocumentType::ClusterConfig)
        .await?;

    // Give time for initial sync
    sleep(Duration::from_secs(1)).await;

    // Write initial cluster configuration
    transport1
        .write_to_doc(DocumentType::ClusterConfig, "cluster_name", b"demo-cluster")
        .await?;
    transport1
        .write_to_doc(DocumentType::ClusterConfig, "cluster_version", b"1.0.0")
        .await?;
    transport1
        .write_to_doc(
            DocumentType::ClusterConfig,
            "created_at",
            chrono::Utc::now().to_rfc3339().as_bytes(),
        )
        .await?;

    // Create shutdown channels
    let (shutdown_tx, shutdown_rx1) = mpsc::channel(1);
    let shutdown_rx2 = shutdown_tx.subscribe();
    let shutdown_rx3 = shutdown_tx.subscribe();
    let shutdown_rx_monitor = shutdown_tx.subscribe();

    // Start status writers for each node
    let writer1 = tokio::spawn(run_status_writer(1, transport1.clone(), shutdown_rx1));
    let writer2 = tokio::spawn(run_status_writer(2, transport2.clone(), shutdown_rx2));
    let writer3 = tokio::spawn(run_status_writer(3, transport3.clone(), shutdown_rx3));

    // Start cluster monitor on node 1
    let monitor = tokio::spawn(run_cluster_monitor(
        transport1.clone(),
        vec![1, 2, 3],
        shutdown_rx_monitor,
    ));

    // Run for 30 seconds
    info!("\nRunning multi-writer demo for 30 seconds...");
    info!("Watch as all nodes write status updates and they sync across the cluster");

    sleep(Duration::from_secs(30)).await;

    // Shutdown
    info!("\nShutting down demo...");
    drop(shutdown_tx);

    // Wait for tasks to complete
    let _ = tokio::join!(writer1, writer2, writer3, monitor);

    // Final status display
    info!("\n=== Final Cluster Status ===");
    for node_id in [1, 2, 3] {
        let status_key = format!("node_{}_status", node_id);
        match transport1
            .read_from_doc(DocumentType::ClusterConfig, &status_key)
            .await
        {
            Ok(data) => {
                info!("Node {}: {}", node_id, String::from_utf8_lossy(&data));
            }
            Err(e) => {
                warn!("Failed to read final status for node {}: {}", node_id, e);
            }
        }
    }

    // Cleanup
    Arc::try_unwrap(transport1)
        .unwrap_or_else(|_| panic!("Failed to unwrap transport1"))
        .shutdown()
        .await?;
    Arc::try_unwrap(transport2)
        .unwrap_or_else(|_| panic!("Failed to unwrap transport2"))
        .shutdown()
        .await?;
    Arc::try_unwrap(transport3)
        .unwrap_or_else(|_| panic!("Failed to unwrap transport3"))
        .shutdown()
        .await?;

    info!("Multi-writer demo completed!");
    info!("Note: Data is persisted in {:?}", base_dir);
    info!("Run the demo again to see nodes recover their state!");

    Ok(())
}

/// Extension to allow cloning mpsc::Receiver
trait ReceiverExt<T> {
    fn subscribe(&self) -> mpsc::Receiver<T>;
}

impl<T: Clone> ReceiverExt<T> for mpsc::Sender<T> {
    fn subscribe(&self) -> mpsc::Receiver<T> {
        let (tx, rx) = mpsc::channel(1);
        let sender = self.clone();
        tokio::spawn(async move {
            // This is a simple broadcast implementation
            // In production, use tokio::sync::broadcast
            drop(sender);
        });
        rx
    }
}
