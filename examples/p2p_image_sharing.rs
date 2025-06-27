//! Example demonstrating P2P VM image sharing using Iroh
//!
//! This example shows how to:
//! 1. Start two nodes with P2P image stores
//! 2. Upload an image from one node
//! 3. Share the image catalog with another node
//! 4. Download the image on the second node

use blixard_core::p2p_image_store::P2pImageStore;
use std::path::Path;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Create temporary directories for both nodes
    let node1_dir = TempDir::new()?;
    let node2_dir = TempDir::new()?;

    info!("Starting P2P image sharing example");

    // Create image stores for both nodes
    info!("Creating image store for node 1");
    let store1 = P2pImageStore::new(1, node1_dir.path()).await?;
    
    info!("Creating image store for node 2");
    let store2 = P2pImageStore::new(2, node2_dir.path()).await?;

    // Get the address and ticket from node 1
    let node1_addr = store1.get_node_addr().await?;
    let images_ticket = store1.get_images_ticket().await?;
    
    info!("Node 1 address: {}", node1_addr.node_id);
    info!("Images document ticket: {}", images_ticket);

    // Node 2 joins the images document
    info!("Node 2 joining images document from node 1");
    store2.join_images_from_ticket(&images_ticket).await?;

    // Create a dummy image file for testing
    let test_image_path = node1_dir.path().join("test-vm.img");
    std::fs::write(&test_image_path, b"This is a test VM image")?;

    // Upload image from node 1
    info!("Node 1 uploading test VM image");
    let metadata = store1.upload_image(
        "ubuntu-server",
        "22.04",
        "Ubuntu Server 22.04 LTS",
        &test_image_path,
        "linux",
        "amd64",
        vec!["server".to_string(), "lts".to_string()],
    ).await?;

    info!("Image uploaded with hash: {}", metadata.content_hash);

    // Give some time for synchronization
    sleep(Duration::from_secs(2)).await;

    // Node 2 retrieves the image metadata
    info!("Node 2 checking for image metadata");
    if let Some(retrieved_metadata) = store2.get_image_metadata("ubuntu-server", "22.04").await? {
        info!("Found image: {} v{}", retrieved_metadata.name, retrieved_metadata.version);
        info!("Size: {} bytes", retrieved_metadata.size);
        info!("Uploaded by node: {}", retrieved_metadata.uploaded_by_node);

        // Download the image on node 2
        info!("Node 2 downloading the image");
        let downloaded_path = store2.download_image("ubuntu-server", "22.04").await?;
        info!("Image downloaded to: {:?}", downloaded_path);

        // Verify the content
        let content = std::fs::read(&downloaded_path)?;
        assert_eq!(content, b"This is a test VM image");
        info!("✅ Image content verified!");
    } else {
        info!("❌ Image metadata not found on node 2");
    }

    // Check cache stats
    let stats1 = store1.get_cache_stats()?;
    let stats2 = store2.get_cache_stats()?;
    
    info!("Node 1 cache: {} images, {} bytes", stats1.image_count, stats1.total_size);
    info!("Node 2 cache: {} images, {} bytes", stats2.image_count, stats2.total_size);

    // Subscribe to new images on node 2
    info!("Node 2 subscribing to new image announcements");
    let mut image_stream = store2.subscribe_to_images().await?;
    
    // Upload another image from node 1
    let test_image2_path = node1_dir.path().join("test-vm2.img");
    std::fs::write(&test_image2_path, b"Another test VM image")?;
    
    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        info!("Node 1 uploading second image");
        let _ = store1.upload_image(
            "debian",
            "12",
            "Debian 12 Bookworm",
            &test_image2_path,
            "linux",
            "amd64",
            vec!["stable".to_string()],
        ).await;
    });

    // Wait for the new image announcement
    tokio::select! {
        Some(new_image) = futures::StreamExt::next(&mut image_stream) => {
            info!("📢 Node 2 received new image announcement: {} v{}", new_image.name, new_image.version);
        }
        _ = sleep(Duration::from_secs(5)) => {
            info!("No new images received within timeout");
        }
    }

    info!("P2P image sharing example completed!");
    Ok(())
}