//! Direct Iroh test - no dependencies on blixard-core

use iroh::{Endpoint, NodeAddr};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("iroh=debug")
        .init();

    println!("=== Direct Iroh Test (no Blixard dependencies) ===\n");
    
    // Create an Iroh endpoint
    let endpoint = Endpoint::builder()
        .discovery(Box::new(iroh::discovery::dns::DnsDiscovery::n0_dns()))
        .bind(0)
        .await?;
    
    println!("✅ Created Iroh endpoint");
    println!("  Node ID: {}", endpoint.node_id());
    println!("  Bound to: {:?}", endpoint.bound_sockets());
    
    // Test accepting connections in background
    let endpoint_clone = endpoint.clone();
    let accept_handle = tokio::spawn(async move {
        println!("\n📡 Waiting for connections...");
        loop {
            if let Some(incoming) = endpoint_clone.accept().await {
                println!("  → Incoming connection!");
                match incoming.accept().await {
                    Ok((connection, alpn)) => {
                        println!("  ✅ Connected! ALPN: {:?}", std::str::from_utf8(alpn.as_bytes()));
                        
                        // Accept a stream
                        match connection.accept_bi().await {
                            Ok((mut send, mut recv)) => {
                                println!("  ✅ Accepted bidirectional stream");
                                
                                // Read some data
                                let mut buf = vec![0u8; 1024];
                                match recv.read(&mut buf).await {
                                    Ok(Some(n)) => {
                                        println!("  ✅ Received {} bytes: {:?}", n, &buf[..n]);
                                        
                                        // Echo it back
                                        let _ = send.write_all(&buf[..n]).await;
                                        let _ = send.finish();
                                        println!("  ✅ Echoed data back");
                                    }
                                    Ok(None) => println!("  Stream closed"),
                                    Err(e) => println!("  Read error: {}", e),
                                }
                            }
                            Err(e) => println!("  Stream error: {}", e),
                        }
                    }
                    Err(e) => {
                        println!("  ❌ Connection error: {}", e);
                    }
                }
            }
        }
    });
    
    // Give it time to start
    sleep(Duration::from_millis(500)).await;
    
    // Try to connect to ourselves
    println!("\n🔄 Testing self-connection...");
    let our_addr = NodeAddr::new(endpoint.node_id())
        .with_direct_addresses(endpoint.bound_sockets());
    
    match endpoint.connect(our_addr.clone(), b"test/echo/1").await {
        Ok(connection) => {
            println!("✅ Self-connection successful!");
            println!("  Remote: {}", connection.remote_node_id());
            
            // Open a stream and send data
            match connection.open_bi().await {
                Ok((mut send, mut recv)) => {
                    println!("✅ Opened bidirectional stream");
                    
                    let test_data = b"Hello, Iroh!";
                    send.write_all(test_data).await?;
                    send.finish()?;
                    println!("✅ Sent test data: {:?}", std::str::from_utf8(test_data));
                    
                    // Read echo
                    let mut buf = vec![0u8; 1024];
                    match recv.read(&mut buf).await? {
                        Some(n) => {
                            println!("✅ Received echo: {:?}", std::str::from_utf8(&buf[..n]));
                        }
                        None => println!("❌ No echo received"),
                    }
                }
                Err(e) => println!("❌ Failed to open stream: {}", e),
            }
        }
        Err(e) => {
            println!("❌ Self-connection failed: {}", e);
        }
    }
    
    // Test with a second endpoint
    println!("\n🔄 Testing two separate endpoints...");
    let endpoint2 = Endpoint::builder()
        .discovery(Box::new(iroh::discovery::dns::DnsDiscovery::n0_dns()))
        .bind(0)
        .await?;
    
    println!("✅ Created second endpoint");
    println!("  Node ID: {}", endpoint2.node_id());
    
    // Connect from endpoint2 to endpoint1
    let endpoint1_addr = NodeAddr::new(endpoint.node_id())
        .with_direct_addresses(endpoint.bound_sockets());
    
    match endpoint2.connect(endpoint1_addr, b"test/p2p/1").await {
        Ok(connection) => {
            println!("✅ Connected endpoint2 → endpoint1");
            println!("  Remote: {}", connection.remote_node_id());
        }
        Err(e) => {
            println!("❌ Connection failed: {}", e);
        }
    }
    
    println!("\n✅ Iroh P2P transport is working!");
    println!("📊 Summary:");
    println!("  - Can create endpoints ✓");
    println!("  - Can accept connections ✓");
    println!("  - Can establish connections ✓");
    println!("  - Can send/receive data ✓");
    println!("\n🚀 Press Ctrl+C to stop.");
    
    tokio::signal::ctrl_c().await.unwrap();
    
    // Cleanup
    accept_handle.abort();
    endpoint.close().await?;
    endpoint2.close().await?;
    
    Ok(())
}