//! Three-node Iroh cluster test - validates multi-node P2P communication

use blixard_core::error::BlixardResult;
use blixard_core::transport::iroh_protocol::{
    generate_request_id, read_message, write_message, MessageType,
};
use bytes::Bytes;
use iroh::{Endpoint, NodeAddr, SecretKey};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Three-Node Iroh Cluster Test ===\n");

    // Create three nodes
    let node1 = create_node(1).await?;
    let node2 = create_node(2).await?;
    let node3 = create_node(3).await?;

    println!("✓ Created three nodes:");
    println!("  Node 1: {}", node1.node_id());
    println!("  Node 2: {}", node2.node_id());
    println!("  Node 3: {}", node3.node_id());

    // Get node addresses
    let addr1 = NodeAddr::new(node1.node_id()).with_direct_addresses(node1.bound_sockets());
    let addr2 = NodeAddr::new(node2.node_id()).with_direct_addresses(node2.bound_sockets());
    let addr3 = NodeAddr::new(node3.node_id()).with_direct_addresses(node3.bound_sockets());

    // Create message channels for nodes
    let (tx1, mut rx1) = mpsc::channel::<String>(10);
    let (tx2, mut rx2) = mpsc::channel::<String>(10);
    let (tx3, mut rx3) = mpsc::channel::<String>(10);

    // Start accept loops for each node
    let node1_clone = node1.clone();
    let tx1_clone = tx1.clone();
    let accept1 = tokio::spawn(async move {
        accept_connections(node1_clone, 1, tx1_clone).await;
    });

    let node2_clone = node2.clone();
    let tx2_clone = tx2.clone();
    let accept2 = tokio::spawn(async move {
        accept_connections(node2_clone, 2, tx2_clone).await;
    });

    let node3_clone = node3.clone();
    let tx3_clone = tx3.clone();
    let accept3 = tokio::spawn(async move {
        accept_connections(node3_clone, 3, tx3_clone).await;
    });

    // Give servers time to start
    sleep(Duration::from_millis(100)).await;

    println!("\n📡 Testing mesh connectivity...\n");

    // Test 1: Node 1 -> Node 2
    println!("Test 1: Node 1 → Node 2");
    send_message(&node1, &addr2, "Hello from Node 1 to Node 2!").await?;
    if let Some(msg) = rx2.recv().await {
        println!("  ✓ Node 2 received: {}", msg);
    }

    // Test 2: Node 2 -> Node 3
    println!("\nTest 2: Node 2 → Node 3");
    send_message(&node2, &addr3, "Hello from Node 2 to Node 3!").await?;
    if let Some(msg) = rx3.recv().await {
        println!("  ✓ Node 3 received: {}", msg);
    }

    // Test 3: Node 3 -> Node 1
    println!("\nTest 3: Node 3 → Node 1");
    send_message(&node3, &addr1, "Hello from Node 3 to Node 1!").await?;
    if let Some(msg) = rx1.recv().await {
        println!("  ✓ Node 1 received: {}", msg);
    }

    // Test 4: Broadcast from Node 1 to all
    println!("\nTest 4: Broadcast from Node 1");
    send_message(&node1, &addr2, "Broadcast from Node 1").await?;
    send_message(&node1, &addr3, "Broadcast from Node 1").await?;

    let mut received = 0;
    while received < 2 {
        tokio::select! {
            Some(msg) = rx2.recv() => {
                println!("  ✓ Node 2 received broadcast: {}", msg);
                received += 1;
            }
            Some(msg) = rx3.recv() => {
                println!("  ✓ Node 3 received broadcast: {}", msg);
                received += 1;
            }
            _ = sleep(Duration::from_millis(100)) => {
                break;
            }
        }
    }

    // Cleanup
    accept1.abort();
    accept2.abort();
    accept3.abort();

    node1.close().await;
    node2.close().await;
    node3.close().await;

    println!("\n✅ All tests complete!");

    Ok(())
}

async fn create_node(id: u64) -> BlixardResult<Endpoint> {
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

async fn accept_connections(endpoint: Endpoint, node_id: u64, tx: mpsc::Sender<String>) {
    loop {
        match endpoint.accept().await {
            Some(incoming) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Ok(connecting) = incoming.accept() {
                        if let Ok(conn) = connecting.await {
                            while let Ok((mut send, mut recv)) = conn.accept_bi().await {
                                // Read message
                                if let Ok((_header, payload)) = read_message(&mut recv).await {
                                    let msg = String::from_utf8_lossy(&payload).to_string();
                                    let _ = tx.send(msg).await;

                                    // Send acknowledgment
                                    let response = Bytes::from(
                                        format!("ACK from Node {}", node_id).into_bytes(),
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
                        }
                    }
                });
            }
            None => break,
        }
    }
}

async fn send_message(from: &Endpoint, to: &NodeAddr, message: &str) -> BlixardResult<()> {
    match from.connect(to.clone(), b"blixard/1").await {
        Ok(conn) => {
            match conn.open_bi().await {
                Ok((mut send, mut recv)) => {
                    // Send message
                    let request = Bytes::from(message.as_bytes().to_vec());
                    let request_id = generate_request_id();
                    write_message(&mut send, MessageType::Request, request_id, &request).await?;

                    // Try to read acknowledgment
                    match tokio::time::timeout(Duration::from_millis(100), read_message(&mut recv))
                        .await
                    {
                        Ok(Ok((_header, payload))) => {
                            let ack = String::from_utf8_lossy(&payload);
                            println!("  → Received {}", ack);
                        }
                        _ => {
                            // Timeout or error reading ack is fine
                        }
                    }

                    send.finish().ok();
                }
                Err(e) => {
                    return Err(blixard_core::error::BlixardError::Internal {
                        message: format!("Failed to open stream: {}", e),
                    });
                }
            }
        }
        Err(e) => {
            return Err(blixard_core::error::BlixardError::Internal {
                message: format!("Failed to connect: {}", e),
            });
        }
    }

    Ok(())
}

use blixard_core::error::BlixardError;
