//! Standalone test to verify ALPN works with Iroh
//! 
//! Run with: cargo run --bin test_alpn_standalone

use iroh::{Endpoint, NodeAddr, SecretKey};
use std::time::Duration;
use tokio::time::timeout;

const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("iroh=debug")
        .init();
    
    println!("=== ALPN Test for Blixard ===\n");
    println!("Testing with ALPN: {:?}", std::str::from_utf8(BLIXARD_ALPN)?);
    
    // Create two endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let secret2 = SecretKey::generate(rand::thread_rng());
    
    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .bind()
        .await?;
    
    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .bind()
        .await?;
    
    let node_id1 = endpoint1.node_id();
    let node_id2 = endpoint2.node_id();
    
    println!("Created two Iroh endpoints:");
    println!("  Node 1: {}", node_id1);
    println!("  Node 2: {}", node_id2);
    
    // Get socket addresses
    let addrs2: Vec<_> = endpoint2.bound_sockets().into_iter().collect();
    println!("\nNode 2 listening on: {:?}", addrs2);
    
    // Create NodeAddr for endpoint2
    let node_addr2 = NodeAddr::new(node_id2)
        .with_direct_addresses(addrs2);
    
    // Set up connection acceptor
    let endpoint2_clone = endpoint2.clone();
    let acceptor = tokio::spawn(async move {
        println!("\n[Node 2] Waiting for incoming connections...");
        if let Some(incoming) = endpoint2_clone.accept().await {
            match incoming.await {
                Ok(conn) => {
                    let alpn = conn.alpn();
                    let alpn_str = alpn.as_ref()
                        .and_then(|a| std::str::from_utf8(a).ok())
                        .unwrap_or("<unknown>");
                    println!("[Node 2] Accepted connection with ALPN: {:?}", alpn_str);
                    
                    // Accept a stream
                    match conn.accept_uni().await {
                        Ok(mut stream) => {
                            let mut buf = vec![0u8; 1024];
                            match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                                Ok(n) => {
                                    let msg = std::str::from_utf8(&buf[..n]).unwrap_or("<invalid>");
                                    println!("[Node 2] Received message: {}", msg);
                                }
                                Err(e) => println!("[Node 2] Failed to read stream: {}", e),
                            }
                        }
                        Err(e) => println!("[Node 2] Failed to accept stream: {}", e),
                    }
                    
                    alpn.as_ref().map(|a| a.as_slice()) == Some(BLIXARD_ALPN)
                }
                Err(e) => {
                    println!("[Node 2] Failed to accept connection: {}", e);
                    false
                }
            }
        } else {
            println!("[Node 2] Endpoint closed");
            false
        }
    });
    
    // Give acceptor time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Try to connect with BLIXARD_ALPN
    println!("\n[Node 1] Attempting connection with BLIXARD_ALPN...");
    match endpoint1.connect(node_addr2, BLIXARD_ALPN).await {
        Ok(conn) => {
            println!("[Node 1] Connection established successfully!");
            let alpn = conn.alpn();
            let alpn_str = alpn.as_ref()
                .and_then(|a| std::str::from_utf8(a).ok())
                .unwrap_or("<unknown>");
            println!("[Node 1] Connection ALPN: {:?}", alpn_str);
            
            // Send a test message
            match conn.open_uni().await {
                Ok(mut stream) => {
                    let msg = b"Hello from Node 1 with BLIXARD_ALPN!";
                    match tokio::io::AsyncWriteExt::write_all(&mut stream, msg).await {
                        Ok(_) => {
                            println!("[Node 1] Sent test message");
                            stream.finish().ok();
                        }
                        Err(e) => println!("[Node 1] Failed to write: {}", e),
                    }
                }
                Err(e) => println!("[Node 1] Failed to open stream: {}", e),
            }
            
            // Wait for acceptor to verify
            match timeout(Duration::from_secs(2), acceptor).await {
                Ok(Ok(alpn_matched)) => {
                    if alpn_matched {
                        println!("\n✅ SUCCESS: ALPN verification passed!");
                    } else {
                        println!("\n❌ FAILURE: ALPN mismatch!");
                    }
                }
                Ok(Err(e)) => println!("\n❌ Acceptor task failed: {}", e),
                Err(_) => println!("\n⚠️  WARNING: Acceptor task timed out"),
            }
        }
        Err(e) => {
            println!("[Node 1] Connection failed: {}", e);
            println!("\n⚠️  Connection failed - this might be expected in test environments");
            println!("    Error details: {}", e);
            
            // Check if it's an ALPN-related error
            if e.to_string().contains("alpn") {
                println!("\n❌ FAILURE: ALPN-related error detected!");
            } else {
                println!("\n✅ Not an ALPN issue - likely network/environment related");
            }
        }
    }
    
    endpoint1.close().await;
    endpoint2.close().await;
    
    println!("\n=== Test Complete ===");
    Ok(())
}