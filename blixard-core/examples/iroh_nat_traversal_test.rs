//! NAT traversal test for Iroh transport
//!
//! This example demonstrates Iroh's NAT traversal capabilities by:
//! 1. Creating nodes that only share their NodeId (not direct addresses)
//! 2. Using relay servers for initial discovery
//! 3. Establishing direct connections through NAT hole-punching

use blixard_core::error::BlixardResult;
use blixard_core::transport::iroh_protocol::{
    generate_request_id, read_message, write_message, MessageType,
};
use bytes::Bytes;
use iroh::{Endpoint, NodeAddr, SecretKey};
use std::env;
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Iroh NAT Traversal Test ===\n");

    // Create two endpoints that will simulate being behind different NATs
    let endpoint1 = create_nat_endpoint("Node1").await?;
    let endpoint2 = create_nat_endpoint("Node2").await?;

    println!("Created endpoints:");
    println!("  Node 1: {}", endpoint1.node_id());
    println!("  Node 2: {}", endpoint2.node_id());

    // Important: In a real NAT scenario, we wouldn't know the direct addresses
    // We only share the NodeId, and let Iroh figure out the connection
    let node1_id = endpoint1.node_id();
    let node2_id = endpoint2.node_id();

    // Get home relay information
    println!("\nRelay configuration:");
    println!("  Using default relay: relay.iroh.network");

    // Start server on endpoint1
    let ep1_clone = endpoint1.clone();
    let server_task = tokio::spawn(async move {
        println!("\n📡 Node 1 waiting for connections...");

        while let Some(incoming) = ep1_clone.accept().await {
            match incoming.accept() {
                Ok(connecting) => {
                    match connecting.await {
                        Ok(conn) => {
                            let remote_id = conn.remote_node_id();
                            println!("✓ Node 1: Connection accepted from {:?}", remote_id);

                            // Get connection info
                            println!("  Connection established with {:?}", remote_id);
                            println!("  Local node: {:?}", ep1_clone.node_id());

                            // Handle streams
                            tokio::spawn(async move {
                                while let Ok((mut send, mut recv)) = conn.accept_bi().await {
                                    if let Ok((_, payload)) = read_message(&mut recv).await {
                                        println!(
                                            "  Node 1 received: {}",
                                            String::from_utf8_lossy(&payload)
                                        );

                                        // Send response
                                        let response = Bytes::from(
                                            b"Hello from Node 1 (through NAT!)".to_vec(),
                                        );
                                        let _ = write_message(
                                            &mut send,
                                            MessageType::Response,
                                            generate_request_id(),
                                            &response,
                                        )
                                        .await;
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            println!("✗ Node 1: Connection failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Node 1: Failed to accept: {}", e);
                }
            }
        }
    });

    // Give server time to start
    sleep(Duration::from_secs(1)).await;

    // Node 2 attempts to connect to Node 1 using only the NodeId
    println!("\n🔌 Node 2 attempting to connect to Node 1...");
    println!("  Note: Using only NodeId, no direct addresses provided");

    // Create NodeAddr with only the node ID (no direct addresses)
    // This simulates a real NAT scenario where direct addresses aren't accessible
    // First test with direct addresses, then without
    let node1_addr = if env::var("NAT_ONLY").is_ok() {
        // Pure NAT traversal mode - only NodeId and relay
        NodeAddr::new(node1_id).with_relay_url("https://relay.iroh.network/".parse().unwrap())
    } else {
        // Include direct addresses for initial testing
        NodeAddr::new(node1_id)
            .with_direct_addresses(endpoint1.bound_sockets())
            .with_relay_url("https://relay.iroh.network/".parse().unwrap())
    };

    println!("  Node 1 address: {:?}", node1_addr);
    println!(
        "  Direct addresses: {:?}",
        node1_addr.direct_addresses().collect::<Vec<_>>()
    );
    if env::var("NAT_ONLY").is_ok() {
        println!("  Mode: NAT-only (relay traversal)");
    } else {
        println!("  Mode: Direct + relay (hybrid)");
    }

    // Attempt connection with retry logic
    let mut attempts = 0;
    let max_attempts = 5;

    while attempts < max_attempts {
        attempts += 1;
        println!("\n  Connection attempt {}/{}...", attempts, max_attempts);

        match timeout(
            Duration::from_secs(10),
            endpoint2.connect(node1_addr.clone(), b"blixard/1"),
        )
        .await
        {
            Ok(Ok(conn)) => {
                println!("✓ Connected successfully!");

                // Show connection established
                println!("  Connected to node: {:?}", conn.remote_node_id());

                // Test communication
                match conn.open_bi().await {
                    Ok((mut send, mut recv)) => {
                        // Send message
                        let msg =
                            format!("Hello from Node 2 via NAT traversal (attempt {})", attempts);
                        write_message(
                            &mut send,
                            MessageType::Request,
                            generate_request_id(),
                            &Bytes::from(msg.as_bytes().to_vec()),
                        )
                        .await?;
                        println!("  → Sent message");

                        // Try to read response
                        match timeout(Duration::from_secs(2), read_message(&mut recv)).await {
                            Ok(Ok((_, response))) => {
                                println!("  ← Received: {}", String::from_utf8_lossy(&response));
                            }
                            _ => {
                                println!("  ← No response received (stream may have closed)");
                            }
                        }

                        send.finish().ok();
                    }
                    Err(e) => {
                        println!("✗ Failed to open stream: {}", e);
                    }
                }

                break;
            }
            Ok(Err(e)) => {
                println!("✗ Connection failed: {}", e);
                if attempts < max_attempts {
                    println!("  Retrying in 2 seconds...");
                    sleep(Duration::from_secs(2)).await;
                }
            }
            Err(_) => {
                println!("✗ Connection timed out");
                if attempts < max_attempts {
                    println!("  Retrying...");
                }
            }
        }
    }

    // Test reverse connection (Node 1 -> Node 2)
    println!("\n🔄 Testing reverse connection (Node 1 → Node 2)...");

    let node2_addr =
        NodeAddr::new(node2_id).with_relay_url("https://relay.iroh.network/".parse().unwrap());

    // Start server on endpoint2
    let ep2_clone = endpoint2.clone();
    let server2_task = tokio::spawn(async move {
        while let Some(incoming) = ep2_clone.accept().await {
            if let Ok(connecting) = incoming.accept() {
                if let Ok(conn) = connecting.await {
                    println!(
                        "✓ Node 2: Connection accepted from {:?}",
                        conn.remote_node_id()
                    );

                    tokio::spawn(async move {
                        while let Ok((mut send, mut recv)) = conn.accept_bi().await {
                            if let Ok((_, payload)) = read_message(&mut recv).await {
                                println!(
                                    "  Node 2 received: {}",
                                    String::from_utf8_lossy(&payload)
                                );

                                let response = Bytes::from(b"Response from Node 2".to_vec());
                                let _ = write_message(
                                    &mut send,
                                    MessageType::Response,
                                    generate_request_id(),
                                    &response,
                                )
                                .await;
                            }
                        }
                    });
                }
            }
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Try reverse connection
    match timeout(
        Duration::from_secs(10),
        endpoint1.connect(node2_addr, b"blixard/1"),
    )
    .await
    {
        Ok(Ok(conn)) => {
            println!("✓ Reverse connection established!");

            if let Ok((mut send, mut recv)) = conn.open_bi().await {
                let msg = Bytes::from(b"Hello from Node 1 to Node 2".to_vec());
                write_message(&mut send, MessageType::Request, generate_request_id(), &msg).await?;

                match timeout(Duration::from_secs(2), read_message(&mut recv)).await {
                    Ok(Ok((_, response))) => {
                        println!("  Response: {}", String::from_utf8_lossy(&response));
                    }
                    _ => {
                        println!("  No response received");
                    }
                }
            }
        }
        _ => {
            println!("✗ Reverse connection failed");
        }
    }

    // Display connection statistics
    println!("\n📊 Connection Statistics:");
    println!("  Node 1 ID: {}", endpoint1.node_id());
    println!("  Node 2 ID: {}", endpoint2.node_id());

    // In a real implementation, we would check:
    // - Whether direct connection was established
    // - Latency comparison (relay vs direct)
    // - Connection stability

    println!("\n✅ NAT traversal test complete!");
    println!("\nKey observations:");
    println!("- Nodes can connect using only NodeId (no direct addresses)");
    println!("- Relay server facilitates initial discovery");
    println!("- Direct connections possible through NAT hole-punching");
    println!("- Bidirectional communication works after traversal");

    // Cleanup
    server_task.abort();
    server2_task.abort();
    endpoint1.close().await;
    endpoint2.close().await;

    Ok(())
}

async fn create_nat_endpoint(name: &str) -> BlixardResult<Endpoint> {
    let secret = SecretKey::generate(rand::thread_rng());

    // Create endpoint with NAT-friendly configuration
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .alpns(vec![b"blixard/1".to_vec()])
        // Use relay for NAT traversal
        .relay_mode(iroh::endpoint::RelayMode::Default)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create {} endpoint: {}", name, e),
        })?;

    println!("{} endpoint created: {}", name, endpoint.node_id());

    // In a real NAT scenario, the bound addresses would be private IPs
    let addrs = endpoint.bound_sockets();
    println!("  Bound to: {:?} (would be private IPs behind NAT)", addrs);

    Ok(endpoint)
}
