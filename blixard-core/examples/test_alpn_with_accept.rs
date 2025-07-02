//! Test ALPN with proper acceptor configuration

use iroh::{Endpoint, NodeAddr, SecretKey};
use std::time::Duration;
use tokio::time::timeout;

const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("iroh=info")
        .init();
    
    println!("=== ALPN Accept Test for Blixard ===\n");
    println!("Testing with ALPN: {:?}", std::str::from_utf8(BLIXARD_ALPN)?);
    
    // Create endpoint that will accept connections
    let secret = SecretKey::generate(rand::thread_rng());
    
    // Build endpoint with ALPN configured
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .alpns(vec![BLIXARD_ALPN.to_vec()])  // Configure accepted ALPNs
        .bind()
        .await?;
    
    let node_id = endpoint.node_id();
    let addrs: Vec<_> = endpoint.bound_sockets().into_iter().collect();
    
    println!("Created endpoint:");
    println!("  Node ID: {}", node_id);
    println!("  Listening on: {:?}", addrs);
    
    // Create client endpoint
    let client_secret = SecretKey::generate(rand::thread_rng());
    let client_endpoint = Endpoint::builder()
        .secret_key(client_secret)
        .bind()
        .await?;
    
    println!("\nClient endpoint created: {}", client_endpoint.node_id());
    
    // Create NodeAddr for server
    let server_addr = NodeAddr::new(node_id)
        .with_direct_addresses(addrs);
    
    // Set up acceptor
    let endpoint_clone = endpoint.clone();
    let acceptor = tokio::spawn(async move {
        println!("\n[Server] Waiting for connections with ALPN support...");
        if let Some(incoming) = endpoint_clone.accept().await {
            match incoming.await {
                Ok(conn) => {
                    let alpn = conn.alpn();
                    let alpn_str = alpn.as_ref()
                        .and_then(|a| std::str::from_utf8(a).ok())
                        .unwrap_or("<none>");
                    println!("[Server] Accepted connection with ALPN: {}", alpn_str);
                    
                    // Handle a simple echo
                    if let Ok(mut stream) = conn.accept_uni().await {
                        let mut buf = vec![0u8; 1024];
                        if let Ok(n) = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                            println!("[Server] Received {} bytes", n);
                        }
                    }
                    
                    alpn.as_ref().map(|a| a.as_slice()) == Some(BLIXARD_ALPN)
                }
                Err(e) => {
                    println!("[Server] Failed to accept: {}", e);
                    false
                }
            }
        } else {
            false
        }
    });
    
    // Small delay
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Try to connect
    println!("\n[Client] Connecting with BLIXARD_ALPN...");
    match client_endpoint.connect(server_addr, BLIXARD_ALPN).await {
        Ok(conn) => {
            println!("[Client] Connected successfully!");
            let alpn = conn.alpn();
            let alpn_str = alpn.as_ref()
                .and_then(|a| std::str::from_utf8(a).ok())
                .unwrap_or("<none>");
            println!("[Client] Connection ALPN: {}", alpn_str);
            
            // Send test data
            if let Ok(mut stream) = conn.open_uni().await {
                tokio::io::AsyncWriteExt::write_all(&mut stream, b"ALPN test").await?;
                stream.finish()?;
                println!("[Client] Sent test data");
            }
            
            // Check acceptor result
            match timeout(Duration::from_secs(2), acceptor).await {
                Ok(Ok(alpn_matched)) => {
                    if alpn_matched {
                        println!("\n✅ SUCCESS: ALPN matched on both sides!");
                    } else {
                        println!("\n❌ FAILURE: ALPN mismatch!");
                    }
                }
                _ => println!("\n⚠️  Acceptor timeout or error"),
            }
        }
        Err(e) => {
            println!("[Client] Connection failed: {}", e);
            if e.to_string().contains("protocol") {
                println!("\n❌ FAILURE: Protocol negotiation failed!");
                println!("This suggests ALPN configuration issue.");
            }
        }
    }
    
    endpoint.close().await;
    client_endpoint.close().await;
    
    println!("\n=== Test Complete ===");
    Ok(())
}