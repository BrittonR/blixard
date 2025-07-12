//! Integration tests for Iroh transport
//!
//! These tests verify that the Iroh transport works correctly
//! in realistic scenarios.

#![cfg(feature = "test-helpers")]

use blixard_core::error::BlixardResult;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::transport::config::{IrohConfig, MigrationStrategy, TransportConfig};
use blixard_core::transport::iroh_protocol::{
    generate_request_id, read_message, write_message, MessageType,
};
use blixard_core::types::NodeConfig;
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

mod common;

#[tokio::test]
async fn test_iroh_single_node_startup() -> BlixardResult<()> {
    let mut config = common::test_node_config(1, 0);
    config.transport_config = Some(IrohConfig::default());

    let node = Arc::new(SharedNodeState::new(config));

    // Verify node can be created with Iroh transport
    assert_eq!(node.get_id(), 1);

    Ok(())
}

#[tokio::test]
async fn test_iroh_message_serialization() -> BlixardResult<()> {
    use blixard_core::raft_codec::{deserialize_message, serialize_message};

    // Test various message types
    let messages = vec![
        create_heartbeat_message(),
        create_vote_request_message(),
        create_log_append_message(),
    ];

    for original in messages {
        let serialized = serialize_message(&original)?;
        let deserialized = deserialize_message(&serialized)?;

        // Verify round-trip
        assert_eq!(original.msg_type(), deserialized.msg_type());
        assert_eq!(original.from, deserialized.from);
        assert_eq!(original.to, deserialized.to);
        assert_eq!(original.term, deserialized.term);

        // Verify size is reasonable
        assert!(serialized.len() < 1024 * 1024); // Less than 1MB
    }

    Ok(())
}

#[tokio::test]
async fn test_iroh_transport_mode_switching() -> BlixardResult<()> {
    // Test dual mode configuration
    let dual_config = TransportConfig::Dual {
        grpc_config: Default::default(),
        iroh_config: Default::default(),
        strategy: MigrationStrategy::default(),
    };

    // Verify we can create nodes with different transport modes
    let configs = vec![
        ("grpc", TransportConfig::Grpc(Default::default())),
        ("iroh", Default::default()),
        ("dual", dual_config),
    ];

    for (name, transport_config) in configs {
        let mut config = common::test_node_config(1, 0);
        config.transport_config = Some(transport_config);

        let node = Arc::new(SharedNodeState::new(config));
        assert_eq!(
            node.get_id(),
            1,
            "Failed to create node with {} transport",
            name
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_iroh_endpoint_lifecycle() -> BlixardResult<()> {
    use iroh::{Endpoint, SecretKey};

    // Create and destroy endpoints multiple times
    for i in 0..3 {
        let secret = SecretKey::generate(rand::thread_rng());
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .alpns(vec![b"blixard/1".to_vec()])
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to create endpoint {}: {}", i, e),
            })?;

        // Verify endpoint is bound
        let addrs = endpoint.bound_sockets();
        assert!(!addrs.is_empty(), "Endpoint {} has no bound addresses", i);

        // Clean shutdown
        endpoint.close().await;
    }

    Ok(())
}

#[tokio::test]
async fn test_iroh_bidirectional_communication() -> BlixardResult<()> {
    use iroh::{Endpoint, NodeAddr, SecretKey};

    // Create two endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create endpoint 1: {}", e),
        })?;

    let secret2 = SecretKey::generate(rand::thread_rng());
    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create endpoint 2: {}", e),
        })?;

    // Get addresses
    let addr1 = NodeAddr::new(endpoint1.node_id()).with_direct_addresses(endpoint1.bound_sockets());
    let _addr2 =
        NodeAddr::new(endpoint2.node_id()).with_direct_addresses(endpoint2.bound_sockets());

    // Start accept task on endpoint1
    let ep1_clone = endpoint1.clone();
    let accept_task = tokio::spawn(async move {
        if let Some(incoming) = ep1_clone.accept().await {
            if let Ok(connecting) = incoming.accept() {
                if let Ok(conn) = connecting.await {
                    if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                        // Read message
                        let (header, _payload) = read_message(&mut recv).await?;
                        assert_eq!(header.msg_type, MessageType::Request);

                        // Send response
                        let response = Bytes::from(b"Response from endpoint 1".to_vec());
                        write_message(
                            &mut send,
                            MessageType::Response,
                            header.request_id,
                            &response,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok::<(), blixard_core::error::BlixardError>(())
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect from endpoint2 to endpoint1
    let conn = endpoint2.connect(addr1, b"blixard/1").await.map_err(|e| {
        blixard_core::error::BlixardError::Internal {
            message: format!("Failed to connect: {}", e),
        }
    })?;

    let (mut send, mut recv) =
        conn.open_bi()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to open stream: {}", e),
            })?;

    // Send request
    let request = Bytes::from(b"Request from endpoint 2".to_vec());
    let request_id = generate_request_id();
    write_message(&mut send, MessageType::Request, request_id, &request).await?;

    // Read response
    let result = timeout(Duration::from_secs(1), read_message(&mut recv)).await;
    assert!(result.is_ok(), "Timeout reading response");

    let (header, payload) = result.unwrap()?;
    assert_eq!(header.msg_type, MessageType::Response);
    assert_eq!(header.request_id, request_id);
    assert_eq!(&payload[..], b"Response from endpoint 1");

    // Cleanup
    drop(send);
    let _ = accept_task.await;
    endpoint1.close().await;
    endpoint2.close().await;

    Ok(())
}

#[tokio::test]
async fn test_iroh_concurrent_connections() -> BlixardResult<()> {
    use iroh::{Endpoint, NodeAddr, SecretKey};
    use tokio::sync::mpsc;

    // Create server endpoint
    let secret = SecretKey::generate(rand::thread_rng());
    let server = Endpoint::builder()
        .secret_key(secret)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create server: {}", e),
        })?;

    let server_addr = NodeAddr::new(server.node_id()).with_direct_addresses(server.bound_sockets());

    // Channel to track connections
    let (tx, mut rx) = mpsc::channel::<u64>(10);

    // Start server accept loop
    let server_clone = server.clone();
    let server_task = tokio::spawn(async move {
        let mut connection_count = 0;
        while let Some(incoming) = server_clone.accept().await {
            connection_count += 1;
            let tx = tx.clone();
            let conn_id = connection_count;

            tokio::spawn(async move {
                if let Ok(connecting) = incoming.accept() {
                    if let Ok(conn) = connecting.await {
                        if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                            // Read message
                            if let Ok((_, payload)) = read_message(&mut recv).await {
                                let _ = tx.send(conn_id).await;

                                // Echo back
                                let _ = write_message(
                                    &mut send,
                                    MessageType::Response,
                                    generate_request_id(),
                                    &payload,
                                )
                                .await;
                            }
                        }
                    }
                }
            });
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create multiple clients
    let num_clients = 5;
    let mut client_tasks = Vec::new();

    for i in 0..num_clients {
        let server_addr = server_addr.clone();
        let task = tokio::spawn(async move {
            let secret = SecretKey::generate(rand::thread_rng());
            let client = Endpoint::builder()
                .secret_key(secret)
                .alpns(vec![b"blixard/1".to_vec()])
                .bind()
                .await
                .map_err(|e| blixard_core::error::BlixardError::Internal {
                    message: format!("Failed to create client {}: {}", i, e),
                })?;

            let conn = client
                .connect(server_addr, b"blixard/1")
                .await
                .map_err(|e| blixard_core::error::BlixardError::Internal {
                    message: format!("Failed to connect client {}: {}", i, e),
                })?;
            let (mut send, mut recv) =
                conn.open_bi()
                    .await
                    .map_err(|e| blixard_core::error::BlixardError::Internal {
                        message: format!("Failed to open stream for client {}: {}", i, e),
                    })?;

            // Send message
            let msg = format!("Message from client {}", i);
            write_message(
                &mut send,
                MessageType::Request,
                generate_request_id(),
                &Bytes::from(msg.as_bytes().to_vec()),
            )
            .await?;

            // Read echo
            let _ = read_message(&mut recv).await?;

            client.close().await;
            Ok::<(), blixard_core::error::BlixardError>(())
        });
        client_tasks.push(task);
    }

    // Wait for all clients to complete
    for task in client_tasks {
        task.await.unwrap()?;
    }

    // Verify all connections were handled
    let mut handled = 0;
    while let Ok(Some(_)) = timeout(Duration::from_millis(100), rx.recv()).await {
        handled += 1;
    }
    assert_eq!(handled, num_clients, "Not all connections were handled");

    // Cleanup
    server_task.abort();
    server.close().await;

    Ok(())
}

#[tokio::test]
async fn test_iroh_large_message_transfer() -> BlixardResult<()> {
    use iroh::{Endpoint, NodeAddr, SecretKey};

    // Create endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create endpoint 1: {}", e),
        })?;

    let secret2 = SecretKey::generate(rand::thread_rng());
    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .alpns(vec![b"blixard/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create endpoint 2: {}", e),
        })?;

    let addr1 = NodeAddr::new(endpoint1.node_id()).with_direct_addresses(endpoint1.bound_sockets());

    // Large message (1MB)
    let large_data = vec![0xAB; 1024 * 1024];
    let large_message = Bytes::from(large_data.clone());

    // Server task
    let ep1_clone = endpoint1.clone();
    let server_task = tokio::spawn(async move {
        if let Some(incoming) = ep1_clone.accept().await {
            if let Ok(connecting) = incoming.accept() {
                if let Ok(conn) = connecting.await {
                    if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                        // Read large message
                        let (_, payload) = read_message(&mut recv).await?;
                        assert_eq!(payload.len(), 1024 * 1024);

                        // Send acknowledgment
                        let ack = Bytes::from(b"ACK".to_vec());
                        write_message(
                            &mut send,
                            MessageType::Response,
                            generate_request_id(),
                            &ack,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok::<(), blixard_core::error::BlixardError>(())
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send large message
    let conn = endpoint2.connect(addr1, b"blixard/1").await.map_err(|e| {
        blixard_core::error::BlixardError::Internal {
            message: format!("Failed to connect: {}", e),
        }
    })?;
    let (mut send, mut recv) =
        conn.open_bi()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to open stream: {}", e),
            })?;

    write_message(
        &mut send,
        MessageType::Request,
        generate_request_id(),
        &large_message,
    )
    .await?;

    // Read acknowledgment
    let (_, ack) = read_message(&mut recv).await?;
    assert_eq!(&ack[..], b"ACK");

    // Cleanup
    let _ = server_task.await;
    endpoint1.close().await;
    endpoint2.close().await;

    Ok(())
}

// Helper functions
fn create_heartbeat_message() -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgHeartbeat);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_commit(100);
    msg
}

fn create_vote_request_message() -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgRequestVote);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(9);
    msg.set_index(100);
    msg
}

fn create_log_append_message() -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgAppend);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(10);
    msg.set_index(100);
    msg.set_commit(95);

    // Add a few entries
    let mut entries = Vec::new();
    for i in 0..3 {
        let mut entry = raft::prelude::Entry::default();
        entry.index = 100 + i;
        entry.term = 10;
        entry.data = vec![0u8; 100]; // 100 bytes of data
        entries.push(entry);
    }
    msg.set_entries(entries.into());

    msg
}
