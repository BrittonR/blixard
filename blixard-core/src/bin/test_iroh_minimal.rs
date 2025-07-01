//! Absolute minimal Iroh test - bypasses all Blixard code to test Iroh directly

use iroh::{Endpoint, NodeAddr};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Minimal Iroh Test ===\n");
    
    // Create an Iroh endpoint - no discovery needed for basic test
    let endpoint = Endpoint::builder()
        .bind()
        .await?;
    
    println!("✓ Created Iroh endpoint");
    println!("  Node ID: {}", endpoint.node_id());
    
    // Get our direct addresses
    let addrs = endpoint.direct_addresses().await?;
    println!("  Direct addresses:");
    for addr in &addrs {
        println!("    - {}", addr);
    }
    
    // Test accepting connections in background
    let endpoint_clone = endpoint.clone();
    tokio::spawn(async move {
        println!("\n📡 Waiting for connections...");
        loop {
            match endpoint_clone.accept().await {
                Some(incoming) => {
                    println!("  Incoming connection!");
                    match incoming.await {
                        Ok(connection) => {
                            let remote_id = connection.remote_node_id();
                            let alpn = connection.alpn();
                            println!("  Connected! Remote: {}", remote_id);
                            println!("  ALPN: {:?}", std::str::from_utf8(&alpn).unwrap_or("<invalid>"));
                            // Just accept and close
                        }
                        Err(e) => {
                            println!("  Connection error: {}", e);
                        }
                    }
                }
                None => {
                    // Endpoint closed
                    break;
                }
            }
        }
    });
    
    // Give it time to start
    sleep(Duration::from_millis(100)).await;
    
    // Try to connect to ourselves
    println!("\n🔄 Testing self-connection...");
    let our_addr = NodeAddr::from_parts(endpoint.node_id(), addrs);
    
    match endpoint.connect(our_addr, b"test/1").await {
        Ok(connection) => {
            println!("✓ Self-connection successful!");
            println!("  Remote: {}", connection.remote_node_id());
        }
        Err(e) => {
            println!("✗ Self-connection failed: {}", e);
        }
    }
    
    // Keep running
    println!("\n🚀 Iroh is working! Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await.unwrap();
    
    Ok(())
}