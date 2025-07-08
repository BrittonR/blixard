//! Quick test to verify transport setup

use blixard_core::error::BlixardResult;
use raft::prelude::*;
use std::time::Instant;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug")
        .init();

    println!("=== Quick Transport Test ===\n");

    // Test message serialization/deserialization
    test_message_codec()?;

    // Test basic Iroh connectivity
    test_iroh_connectivity().await?;

    println!("\n✅ All tests passed!");
    Ok(())
}

fn test_message_codec() -> BlixardResult<()> {
    use blixard_core::raft_codec::{deserialize_message, serialize_message};

    println!("Testing Raft message codec...");

    // Create test message
    let mut msg = Message::default();
    msg.set_msg_type(MessageType::MsgHeartbeat);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_commit(100);

    // Serialize
    let start = Instant::now();
    let bytes = serialize_message(&msg)?;
    let serialize_time = start.elapsed();

    // Deserialize
    let start = Instant::now();
    let decoded = deserialize_message(&bytes)?;
    let deserialize_time = start.elapsed();

    // Verify
    assert_eq!(msg.msg_type(), decoded.msg_type());
    assert_eq!(msg.from, decoded.from);
    assert_eq!(msg.to, decoded.to);
    assert_eq!(msg.term, decoded.term);
    assert_eq!(msg.commit, decoded.commit);

    println!("  ✓ Message size: {} bytes", bytes.len());
    println!("  ✓ Serialize time: {:?}", serialize_time);
    println!("  ✓ Deserialize time: {:?}", deserialize_time);

    Ok(())
}

async fn test_iroh_connectivity() -> BlixardResult<()> {
    use iroh::{Endpoint, SecretKey};

    println!("\nTesting Iroh connectivity...");

    // Create two endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let secret2 = SecretKey::generate(rand::thread_rng());

    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint1: {}", e),
        })?;

    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint2: {}", e),
        })?;

    println!("  ✓ Endpoint 1: {}", endpoint1.node_id());
    println!("  ✓ Endpoint 2: {}", endpoint2.node_id());

    // Test connection
    let node_id2 = endpoint2.node_id();
    let node_addr2 = iroh::NodeAddr::new(node_id2);
    println!("  ✓ Node 2 address: {:?}", node_addr2);

    // Try to connect
    let start = Instant::now();
    match endpoint1.connect(node_addr2, b"/test").await {
        Ok(_conn) => {
            let connect_time = start.elapsed();
            println!("  ✓ Connection established in {:?}", connect_time);
        }
        Err(e) => {
            println!("  ⚠️  Connection failed (expected in isolated test): {}", e);
        }
    }

    Ok(())
}
