//! Basic Iroh connectivity test

use blixard_core::error::BlixardResult;
use blixard_core::transport::iroh_protocol::{
    generate_request_id, read_message, write_message, MessageType,
};
use bytes::Bytes;
use iroh::{Endpoint, NodeAddr, SecretKey};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Basic Iroh Connectivity Test ===\n");

    // Create two endpoints
    let endpoint1 = create_endpoint(1).await?;
    let endpoint2 = create_endpoint(2).await?;

    // Get addresses
    let addr1 = NodeAddr::new(endpoint1.node_id()).with_direct_addresses(endpoint1.bound_sockets());
    let addr2 = NodeAddr::new(endpoint2.node_id()).with_direct_addresses(endpoint2.bound_sockets());

    println!("Endpoint 1: {}", endpoint1.node_id());
    println!(
        "  Addresses: {:?}",
        addr1.direct_addresses().collect::<Vec<_>>()
    );
    println!("Endpoint 2: {}", endpoint2.node_id());
    println!(
        "  Addresses: {:?}",
        addr2.direct_addresses().collect::<Vec<_>>()
    );

    // Start accepting connections on endpoint1
    let ep1_clone = endpoint1.clone();
    let accept_task = tokio::spawn(async move {
        println!("\n📡 Waiting for connection on endpoint 1...");
        match ep1_clone.accept().await {
            Some(incoming) => {
                // Accept the incoming connection
                match incoming.accept() {
                    Ok(connecting) => match connecting.await {
                        Ok(conn) => {
                            println!("✓ Connection accepted from: {:?}", conn.remote_node_id());

                            // Accept a stream
                            match conn.accept_bi().await {
                                Ok((mut send, mut recv)) => {
                                    println!("✓ Stream accepted");

                                    // Read message
                                    let (header, payload) = read_message(&mut recv).await?;
                                    println!(
                                        "✓ Received message type: {:?}, size: {} bytes",
                                        header.msg_type,
                                        payload.len()
                                    );

                                    // Send response
                                    let response = Bytes::from(b"Hello from endpoint 1!".to_vec());
                                    write_message(
                                        &mut send,
                                        MessageType::Response,
                                        header.request_id,
                                        &response,
                                    )
                                    .await?;
                                    println!("✓ Response sent");
                                }
                                Err(e) => {
                                    println!("✗ Failed to accept stream: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("✗ Failed to complete connection: {}", e);
                        }
                    },
                    Err(e) => {
                        println!("✗ Failed to accept connection: {}", e);
                    }
                }
            }
            None => {
                println!("✗ No connection received");
            }
        }
        Ok::<(), BlixardError>(())
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Connect from endpoint2 to endpoint1
    println!("\n🔌 Connecting from endpoint 2 to endpoint 1...");
    match endpoint2.connect(addr1.clone(), b"blixard/1").await {
        Ok(conn) => {
            println!("✓ Connected to endpoint 1");

            // Open a bidirectional stream
            match conn.open_bi().await {
                Ok((mut send, mut recv)) => {
                    println!("✓ Stream opened");

                    // Send a message
                    let request = Bytes::from(b"Hello from endpoint 2!".to_vec());
                    let request_id = generate_request_id();
                    write_message(&mut send, MessageType::Request, request_id, &request).await?;
                    println!("✓ Request sent");

                    // Read response
                    match read_message(&mut recv).await {
                        Ok((_header, payload)) => {
                            println!(
                                "✓ Response received: {:?}",
                                String::from_utf8_lossy(&payload)
                            );
                        }
                        Err(e) => {
                            // Connection might be closed after send, which is fine
                            println!("Note: Stream closed after response (expected): {}", e);
                        }
                    }

                    // Close stream
                    send.finish().ok();
                }
                Err(e) => {
                    println!("✗ Failed to open stream: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to connect: {}", e);
        }
    }

    // Wait for accept task
    let _ = accept_task.await;

    // Cleanup
    endpoint1.close().await;
    endpoint2.close().await;

    println!("\n✅ Test complete!");

    Ok(())
}

async fn create_endpoint(id: u64) -> BlixardResult<Endpoint> {
    let secret = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint {}: {}", id, e),
        })?;

    Ok(endpoint)
}

use blixard_core::error::BlixardError;
