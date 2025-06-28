//! Test Raft transport functionality

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use raft::prelude::*;

use blixard_core::transport::raft_transport_adapter::RaftTransport;
use blixard_core::transport::config::{TransportConfig, GrpcConfig, IrohConfig};
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::NodeConfig;
use blixard_core::error::BlixardResult;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,iroh=info")
        .init();

    println!("=== Raft Transport Performance Test ===\n");

    // Test both transports
    test_transport("gRPC", TransportConfig::Grpc(GrpcConfig::default())).await?;
    println!("\n");
    test_transport("Iroh", TransportConfig::Iroh(IrohConfig::default())).await?;

    Ok(())
}

async fn test_transport(name: &str, config: TransportConfig) -> BlixardResult<()> {
    println!("Testing {} transport...", name);
    
    // Create node
    let node_config = NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        data_dir: std::env::temp_dir().join(format!("blixard-test-{}", uuid::Uuid::new_v4())).to_string_lossy().to_string(),
        vm_backend: "test".to_string(),
        join_addr: None,
        use_tailscale: false,
        transport_config: Some(config.clone()),
    };
    
    let node = Arc::new(SharedNodeState::new(node_config));
    
    // Initialize node (needed for Iroh endpoint)
    if matches!(config, TransportConfig::Iroh(_)) {
        println!("  Initializing Iroh endpoint...");
        // For Iroh, we need to initialize the database and set up Iroh
        use blixard_core::storage::Storage;
        use redb::Database;
        
        // Create database
        let db_path = node.get_data_dir().join("test.db");
        let database = Arc::new(Database::create(&db_path)?);
        node.set_database(database.clone()).await;
        
        // Initialize storage
        let storage = Storage::new(node.get_id(), database.clone(), node.clone()).await?;
        node.set_storage(Arc::new(storage)).await;
        
        // Initialize Iroh endpoint
        let secret_key = iroh::SecretKey::generate();
        let endpoint = iroh::Endpoint::builder()
            .secret_key(secret_key)
            .bind()
            .await?;
        
        // Store the endpoint
        node.set_iroh_endpoint(endpoint.clone(), endpoint.node_id()).await?;
    }
    
    // Create channel for receiving messages
    let (tx, mut rx) = mpsc::unbounded_channel();
    
    // Create transport
    println!("  Creating transport...");
    let transport = RaftTransport::new(node.clone(), tx, &config).await?;
    
    // Start maintenance (for gRPC)
    transport.start_maintenance().await;
    
    // Create test messages
    let messages = create_test_messages();
    println!("  Sending {} test messages...", messages.len());
    
    let start = Instant::now();
    
    // Send messages (to non-existent peer, will fail but measures overhead)
    for msg in &messages {
        let _ = transport.send_message(msg.to, msg.clone()).await;
    }
    
    let elapsed = start.elapsed();
    
    println!("  Completed in {:?}", elapsed);
    println!("  Average per message: {:?}", elapsed / messages.len() as u32);
    println!("  Messages per second: {:.0}", messages.len() as f64 / elapsed.as_secs_f64());
    
    // Get metrics
    let metrics = transport.get_metrics().await;
    println!("  Transport metrics: {:?}", metrics);
    
    // Shutdown
    transport.shutdown().await;
    
    // Clean up
    let _ = tokio::fs::remove_dir_all(&node.get_data_dir()).await;
    
    Ok(())
}

fn create_test_messages() -> Vec<Message> {
    let mut messages = Vec::new();
    
    // Election messages (high priority)
    for i in 0..10 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgRequestVote);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(i as u64);
        messages.push(msg);
    }
    
    // Heartbeats (medium priority)
    for i in 0..50 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHeartbeat);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(10);
        msg.set_commit(i as u64);
        messages.push(msg);
    }
    
    // Log entries (normal priority)
    for i in 0..20 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(10);
        msg.set_index(100 + i as u64);
        
        // Add some entries
        let mut entries = vec![];
        for j in 0..5 {
            let mut entry = Entry::default();
            entry.index = 100 + i as u64 + j;
            entry.term = 10;
            entry.data = vec![0u8; 1024]; // 1KB per entry
            entries.push(entry);
        }
        msg.set_entries(entries.into());
        
        messages.push(msg);
    }
    
    messages
}