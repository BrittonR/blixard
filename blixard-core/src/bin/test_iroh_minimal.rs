//! Absolute minimal Iroh test - bypasses all Blixard code to test Iroh directly

use iroh::{Endpoint, NodeAddr};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Minimal Iroh Test ===\n");
    
    // Create an Iroh endpoint
    let endpoint = Endpoint::builder()
        .discovery(Box::new(iroh::discovery::dns::DnsDiscovery::n0_dns()))
        .bind(0)
        .await?;
    
    println!("✓ Created Iroh endpoint");
    println!("  Node ID: {}", endpoint.node_id());
    println!("  Bound to: {:?}", endpoint.bound_sockets());
    
    // Test accepting connections in background
    let endpoint_clone = endpoint.clone();
    tokio::spawn(async move {
        println!("\n📡 Waiting for connections...");
        loop {
            if let Some(incoming) = endpoint_clone.accept().await {
                println!("  Incoming connection!");
                match incoming.accept().await {
                    Ok((connection, alpn)) => {
                        println!("  Connected! ALPN: {:?}", std::str::from_utf8(alpn.as_bytes()));
                        // Just accept and close
                    }
                    Err(e) => {
                        println!("  Connection error: {}", e);
                    }
                }
            }
        }
    });
    
    // Give it time to start
    sleep(Duration::from_millis(100)).await;
    
    // Try to connect to ourselves
    println!("\n🔄 Testing self-connection...");
    let our_addr = NodeAddr::new(endpoint.node_id())
        .with_direct_addresses(endpoint.bound_sockets());
    
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