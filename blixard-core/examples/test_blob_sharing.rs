use blixard_core::iroh_transport_v2::IrohTransportV2;
use iroh_blobs::Hash;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("Testing Iroh blob sharing functionality...\n");

    // Create temporary directory
    let temp_dir = TempDir::new()?;

    // Create transport instance
    println!("Creating Iroh transport...");
    let transport = IrohTransportV2::new(1, temp_dir.path()).await?;

    // Get node address
    let addr = transport.node_addr().await?;
    println!("Node initialized with ID: {}", addr.node_id);

    // Test 1: Share a file
    println!("\n--- Test 1: Share a file ---");
    let test_file = temp_dir.path().join("test.txt");
    let test_content = b"Hello, this is a test file for blob sharing!";
    std::fs::write(&test_file, test_content)?;

    let hash = transport.share_file(&test_file).await?;
    println!("File shared successfully!");
    println!("Hash: {}", hash);
    println!("Size: {} bytes", test_content.len());

    // Test 2: Download the file locally
    println!("\n--- Test 2: Download file from local store ---");
    let output_file = temp_dir.path().join("downloaded.txt");
    transport.download_file(hash, &output_file).await?;

    let downloaded_content = std::fs::read(&output_file)?;
    if downloaded_content == test_content {
        println!("✓ Downloaded content matches original!");
    } else {
        println!("✗ Downloaded content does not match!");
    }

    // Test 3: Share another file
    println!("\n--- Test 3: Share multiple files ---");
    let test_file2 = temp_dir.path().join("test2.txt");
    let test_content2 = b"This is another test file with different content.";
    std::fs::write(&test_file2, test_content2)?;

    let hash2 = transport.share_file(&test_file2).await?;
    println!("Second file shared with hash: {}", hash2);

    // Verify both files are in the store
    let output_file2 = temp_dir.path().join("downloaded2.txt");
    transport.download_file(hash2, &output_file2).await?;
    println!("✓ Successfully downloaded second file");

    // Test 4: Try to download non-existent blob
    println!("\n--- Test 4: Download non-existent blob ---");
    let fake_hash = Hash::from(*blake3::hash(b"non-existent").as_bytes());
    match transport
        .download_file(fake_hash, &temp_dir.path().join("fake.txt"))
        .await
    {
        Err(e) => println!("✓ Expected error: {}", e),
        Ok(_) => println!("✗ Unexpected success!"),
    }

    // Clean up
    println!("\n--- Shutting down transport ---");
    transport.shutdown().await?;
    println!("✓ Transport shut down successfully");

    println!("\nAll tests completed!");
    Ok(())
}
