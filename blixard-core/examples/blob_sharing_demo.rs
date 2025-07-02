use blixard_core::iroh_transport_v2::IrohTransportV2;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Blixard Blob Sharing Demo ===\n");
    println!("This demonstrates the blob storage functionality in IrohTransportV2.\n");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    
    // Create transport instance
    println!("1. Creating Iroh transport with blob storage...");
    let transport = IrohTransportV2::new(1, temp_dir.path()).await?;
    let addr = transport.node_addr().await?;
    println!("   ✓ Node initialized with ID: {}", addr.node_id);
    
    // Demo 1: Share a text file
    println!("\n2. Sharing a text file...");
    let text_file = temp_dir.path().join("document.txt");
    let text_content = b"# Important Document\n\nThis is a document that will be content-addressed and stored as a blob.";
    std::fs::write(&text_file, text_content)?;
    
    let text_hash = transport.share_file(&text_file).await?;
    println!("   ✓ Text file shared");
    println!("   - Hash: {}", text_hash);
    println!("   - Size: {} bytes", text_content.len());
    
    // Demo 2: Share a binary file (simulated image)
    println!("\n3. Sharing a binary file (simulated image)...");
    let image_file = temp_dir.path().join("image.bin");
    let image_content = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG magic bytes
    std::fs::write(&image_file, &image_content)?;
    
    let image_hash = transport.share_file(&image_file).await?;
    println!("   ✓ Binary file shared");
    println!("   - Hash: {}", image_hash);
    println!("   - Size: {} bytes", image_content.len());
    
    // Demo 3: Download files by hash
    println!("\n4. Downloading files by their content hash...");
    
    let downloaded_text = temp_dir.path().join("downloaded_doc.txt");
    transport.download_file(text_hash, &downloaded_text).await?;
    let downloaded_content = std::fs::read(&downloaded_text)?;
    println!("   ✓ Text file downloaded");
    println!("   - Content matches: {}", downloaded_content == text_content);
    
    let downloaded_image = temp_dir.path().join("downloaded_img.bin");
    transport.download_file(image_hash, &downloaded_image).await?;
    let downloaded_img_content = std::fs::read(&downloaded_image)?;
    println!("   ✓ Binary file downloaded");
    println!("   - Content matches: {}", downloaded_img_content == image_content);
    
    // Demo 4: Content addressing
    println!("\n5. Demonstrating content addressing...");
    let duplicate_file = temp_dir.path().join("duplicate.txt");
    std::fs::write(&duplicate_file, text_content)?;
    
    let duplicate_hash = transport.share_file(&duplicate_file).await?;
    println!("   ✓ Duplicate file shared");
    println!("   - Original hash:  {}", text_hash);
    println!("   - Duplicate hash: {}", duplicate_hash);
    println!("   - Hashes match: {} (deduplication works!)", text_hash == duplicate_hash);
    
    // Demo 5: Error handling
    println!("\n6. Testing error handling...");
    let fake_hash = iroh_blobs::Hash::from(*blake3::hash(b"non-existent-content").as_bytes());
    match transport.download_file(fake_hash, &temp_dir.path().join("fake.txt")).await {
        Err(e) => println!("   ✓ Correctly handled missing blob: {}", e),
        Ok(_) => println!("   ✗ Unexpected success!"),
    }
    
    // Summary
    println!("\n=== Summary ===");
    println!("The IrohTransportV2 blob storage provides:");
    println!("- Content-addressed storage using Blake3 hashes");
    println!("- Automatic deduplication of identical content");
    println!("- Simple API for sharing and downloading files");
    println!("- Foundation for P2P file sharing between nodes");
    
    println!("\nFor P2P transfer between nodes:");
    println!("1. Node A shares a file -> gets hash");
    println!("2. Node A sends blob info to Node B (hash + metadata)");
    println!("3. Node B requests blob from Node A using the hash");
    println!("4. Node A serves the blob content");
    println!("5. Node B verifies the hash and stores locally");
    
    // Clean up
    transport.shutdown().await?;
    println!("\n✓ Transport shut down successfully");
    
    Ok(())
}