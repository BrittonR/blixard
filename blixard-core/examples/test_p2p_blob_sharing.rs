use blixard_core::iroh_transport_v2::{IrohTransportV2, DocumentType};
use tempfile::TempDir;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("Testing P2P blob sharing between nodes...\n");

    // Create temporary directories for both nodes
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;
    
    // Create two transport instances
    println!("Creating two Iroh transport nodes...");
    let transport1 = Arc::new(IrohTransportV2::new(1, temp_dir1.path()).await?);
    let transport2 = Arc::new(IrohTransportV2::new(2, temp_dir2.path()).await?);
    
    // Get node addresses
    let addr1 = transport1.node_addr().await?;
    let addr2 = transport2.node_addr().await?;
    println!("Node 1 ID: {}", addr1.node_id);
    println!("Node 2 ID: {}", addr2.node_id);
    
    // Set up blob request handler for node 1
    let (tx, mut rx) = mpsc::channel(10);
    let transport1_handler = transport1.clone();
    tokio::spawn(async move {
        transport1_handler.handle_blob_requests(|request| {
            println!("Node 1 received request: {}", request);
            // For this example, we don't handle custom requests
            None
        }).await.unwrap();
    });
    
    // Test 1: Node 1 shares a file
    println!("\n--- Test 1: Node 1 shares a file ---");
    let test_file = temp_dir1.path().join("shared.txt");
    let test_content = b"This is a file shared by Node 1 for P2P transfer!";
    std::fs::write(&test_file, test_content)?;
    
    let hash = transport1.share_file(&test_file).await?;
    println!("Node 1 shared file with hash: {}", hash);
    
    // Test 2: Send blob info to Node 2
    println!("\n--- Test 2: Send blob info to Node 2 ---");
    transport1.send_blob_info(&addr2, hash, "shared.txt").await?;
    println!("Blob info sent to Node 2");
    
    // Test 3: Accept incoming connections on Node 2
    let transport2_accept = transport2.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        transport2_accept.accept_connections(move |doc_type, data| {
            if doc_type == DocumentType::FileTransfer {
                if let Ok(message) = String::from_utf8(data.clone()) {
                    println!("Node 2 received FileTransfer message: {}", message);
                    if message.starts_with("BLOB:") {
                        // Parse blob info
                        let parts: Vec<&str> = message.strip_prefix("BLOB:").unwrap().split(':').collect();
                        if parts.len() == 2 {
                            let hash_str = parts[0];
                            let filename = parts[1];
                            println!("Received blob info - Hash: {}, Filename: {}", hash_str, filename);
                            let _ = tx_clone.try_send((hash_str.to_string(), filename.to_string()));
                        }
                    }
                }
            }
        }).await.unwrap();
    });
    
    // Give some time for the message to be received
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Test 4: Node 2 downloads the blob from Node 1
    println!("\n--- Test 4: Node 2 downloads blob from Node 1 ---");
    
    // In a real implementation, Node 2 would request the blob from Node 1
    // For this example, we'll simulate the process
    let output_file = temp_dir2.path().join("received.txt");
    match transport2.download_blob_from_peer(&addr1, hash, &output_file).await {
        Ok(_) => {
            println!("Node 2 successfully downloaded blob from Node 1");
            let received_content = std::fs::read(&output_file)?;
            if received_content == test_content {
                println!("✓ Downloaded content matches original!");
            } else {
                println!("✗ Downloaded content does not match!");
            }
        }
        Err(e) => {
            println!("Note: Direct P2P download requires blob request handler setup");
            println!("Error: {}", e);
            
            // In a real P2P implementation, Node 1 would need to run handle_blob_requests
            // to respond to GET_BLOB requests from Node 2.
            println!("\nNote: For full P2P blob transfer, Node 1 needs to handle blob requests.");
            println!("This example demonstrates the blob sharing API.");
        }
    }
    
    // Test 5: Node 2 shares a different file
    println!("\n--- Test 5: Node 2 shares a different file ---");
    let test_file2 = temp_dir2.path().join("response.txt");
    let test_content2 = b"This is Node 2's response file!";
    std::fs::write(&test_file2, test_content2)?;
    
    let hash2 = transport2.share_file(&test_file2).await?;
    println!("Node 2 shared file with hash: {}", hash2);
    
    // Clean up
    println!("\n--- Shutting down transports ---");
    // Note: We can't await shutdown on Arc'd transports, so we'll just drop them
    drop(transport1);
    drop(transport2);
    println!("✓ Transports shut down");
    
    println!("\nP2P blob sharing demonstration completed!");
    Ok(())
}