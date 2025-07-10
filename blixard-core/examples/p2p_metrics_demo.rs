//! Demonstrates P2P transfer metrics collection
//!
//! This example shows how metrics are collected during P2P image operations:
//! - Import metrics (chunks processed, deduplication rates)
//! - Download metrics (transfer times, cache hits)
//! - Verification metrics (NAR hash checks)

use blixard_core::{
    abstractions::command::TokioCommandExecutor,
    error::BlixardResult,
    metrics_otel::{init_prometheus, prometheus_metrics},
    metrics_server::start_metrics_server,
    nix_image_store::NixImageStore,
    p2p_manager::{P2pConfig, P2pManager},
};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== P2P Transfer Metrics Demo ===");

    // Initialize metrics
    init_prometheus()?;

    // Start metrics HTTP server
    let metrics_handle = tokio::spawn(async {
        if let Err(e) = start_metrics_server("127.0.0.1:9090").await {
            tracing::error!("Metrics server error: {}", e);
        }
    });

    info!("Metrics server started at http://127.0.0.1:9090/metrics");

    // Create temporary directories
    let temp_dir = TempDir::new()?;
    let node1_dir = temp_dir.path().join("node1");
    let node2_dir = temp_dir.path().join("node2");
    std::fs::create_dir_all(&node1_dir)?;
    std::fs::create_dir_all(&node2_dir)?;

    // Set up first node with P2P and image store
    let p2p_manager1 = Arc::new(P2pManager::new(1, &node1_dir, P2pConfig::default()).await?);
    let command_executor = Arc::new(TokioCommandExecutor::new());
    let image_store1 = NixImageStore::new(1, p2p_manager1, &node1_dir, None, command_executor.clone()).await?;

    // Create some test content
    let test_file = node1_dir.join("test-system");
    let test_content = vec![0u8; 10 * 1024 * 1024]; // 10MB file
    tokio::fs::write(&test_file, &test_content).await?;

    info!("\n1. Importing test image (10MB)...");
    let metadata = image_store1
        .import_microvm("metrics-test-vm", &test_file, None)
        .await?;

    info!("   Image ID: {}", metadata.id);
    info!("   Total chunks: {}", metadata.chunk_hashes.len());

    // Give metrics time to be recorded
    sleep(Duration::from_millis(100)).await;

    // Display current metrics
    info!("\n2. Current P2P metrics:");
    display_p2p_metrics();

    info!("\n3. Simulating download (all chunks cached)...");
    let (_path, stats) = image_store1
        .download_image(&metadata.id, Some(&node1_dir.join("downloads")))
        .await?;

    info!("   Transfer completed:");
    info!("   - Duration: {:?}", stats.duration);
    info!("   - Chunks transferred: {}", stats.chunks_transferred);
    info!("   - Chunks deduplicated: {}", stats.chunks_deduplicated);

    sleep(Duration::from_millis(100)).await;

    info!("\n4. Updated P2P metrics:");
    display_p2p_metrics();

    // Set up second node to simulate cross-node transfer
    let p2p_manager2 = Arc::new(P2pManager::new(2, &node2_dir, P2pConfig::default()).await?);
    let image_store2 = NixImageStore::new(2, p2p_manager2, &node2_dir, None, command_executor).await?;

    info!("\n5. Importing similar image with 50% overlap...");
    // Create a file that's 50% identical to the first
    let mut similar_content = test_content.clone();
    for i in 0..similar_content.len() / 2 {
        similar_content[i] ^= 0xFF; // Flip bits in first half
    }
    let similar_file = node2_dir.join("similar-system");
    tokio::fs::write(&similar_file, &similar_content).await?;

    let metadata2 = image_store2
        .import_microvm("similar-test-vm", &similar_file, None)
        .await?;

    info!("   Image ID: {}", metadata2.id);
    info!("   Total chunks: {}", metadata2.chunk_hashes.len());

    sleep(Duration::from_millis(100)).await;

    info!("\n6. Final P2P metrics:");
    display_p2p_metrics();

    info!("\n7. Verification metrics test...");
    // Test verification success
    let is_valid = image_store1.verify_image(&metadata.id).await?;
    info!("   Verification result: {}", is_valid);

    sleep(Duration::from_millis(100)).await;

    info!("\n8. Complete metrics output:");
    println!("{}", prometheus_metrics());

    // Cleanup
    metrics_handle.abort();

    info!("\n✅ P2P metrics demo completed!");
    Ok(())
}

fn display_p2p_metrics() {
    let metrics = prometheus_metrics();
    let lines: Vec<&str> = metrics.lines().collect();

    // Extract P2P-related metrics
    for line in lines {
        if line.starts_with("p2p_") && !line.starts_with("# ") {
            println!("   {}", line);
        }
    }
}
