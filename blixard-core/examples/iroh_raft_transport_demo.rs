//! Demo of Raft consensus over Iroh P2P transport
//!
//! This example demonstrates how to use the Iroh Raft transport adapter
//! for running Raft consensus over P2P connections instead of gRPC.

use blixard_core::transport::config::{TransportConfig, IrohConfig, GrpcConfig, MigrationStrategy, RaftTransportPreference};
use blixard_core::transport::raft_transport_adapter::RaftTransport;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::NodeConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use raft::prelude::*;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    info!("Starting Iroh Raft transport demo");
    
    // Create three nodes for a simple cluster
    let nodes = create_demo_nodes().await?;
    
    // Demo 1: Compare message latency between gRPC and Iroh
    info!("\n=== Demo 1: Message Latency Comparison ===");
    demo_message_latency(&nodes).await?;
    
    // Demo 2: Throughput test
    info!("\n=== Demo 2: Throughput Test ===");
    demo_throughput(&nodes).await?;
    
    // Demo 3: Large message handling (snapshots)
    info!("\n=== Demo 3: Large Message Handling ===");
    demo_large_messages(&nodes).await?;
    
    // Demo 4: Priority handling
    info!("\n=== Demo 4: Message Priority Handling ===");
    demo_priority_handling(&nodes).await?;
    
    // Cleanup
    for (_, transport, _) in nodes {
        transport.shutdown().await;
    }
    
    info!("Demo completed successfully!");
    Ok(())
}

/// Create demo nodes with different transport configurations
async fn create_demo_nodes() -> Result<Vec<(Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>)>, Box<dyn std::error::Error>> {
    let mut nodes = Vec::new();
    
    // Node 1: gRPC transport
    let config1 = NodeConfig {
        id: 1,
        bind_address: "127.0.0.1:7001".to_string(),
        data_dir: std::env::temp_dir().join("blixard-demo-1"),
        vm_backend: "test".to_string(),
        join_address: None,
        bootstrap: true,
    };
    let node1 = Arc::new(SharedNodeState::new(config1));
    let (tx1, rx1) = mpsc::unbounded_channel();
    let transport1 = RaftTransport::new(
        node1.clone(),
        tx1,
        &TransportConfig::Grpc(GrpcConfig::default()),
    ).await?;
    nodes.push((node1, transport1, rx1));
    
    // Node 2: Iroh transport
    let config2 = NodeConfig {
        id: 2,
        bind_address: "127.0.0.1:7002".to_string(),
        data_dir: std::env::temp_dir().join("blixard-demo-2"),
        vm_backend: "test".to_string(),
        join_address: Some("127.0.0.1:7001".to_string()),
        bootstrap: false,
    };
    let node2 = Arc::new(SharedNodeState::new(config2));
    let (tx2, rx2) = mpsc::unbounded_channel();
    let transport2 = RaftTransport::new(
        node2.clone(),
        tx2,
        &TransportConfig::Iroh(IrohConfig::default()),
    ).await?;
    nodes.push((node2, transport2, rx2));
    
    // Node 3: Dual transport
    let config3 = NodeConfig {
        id: 3,
        bind_address: "127.0.0.1:7003".to_string(),
        data_dir: std::env::temp_dir().join("blixard-demo-3"),
        vm_backend: "test".to_string(),
        join_address: Some("127.0.0.1:7001".to_string()),
        bootstrap: false,
    };
    let node3 = Arc::new(SharedNodeState::new(config3));
    let (tx3, rx3) = mpsc::unbounded_channel();
    let mut strategy = MigrationStrategy::default();
    strategy.raft_transport = RaftTransportPreference::AlwaysIroh;
    let transport3 = RaftTransport::new(
        node3.clone(),
        tx3,
        &TransportConfig::Dual {
            grpc_config: GrpcConfig::default(),
            iroh_config: IrohConfig::default(),
            strategy,
        },
    ).await?;
    nodes.push((node3, transport3, rx3));
    
    Ok(nodes)
}

/// Demo message latency comparison
async fn demo_message_latency(nodes: &[(Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>)]) -> Result<(), Box<dyn std::error::Error>> {
    // Send election messages and measure latency
    let mut election_msg = Message::default();
    election_msg.set_msg_type(MessageType::MsgRequestVote);
    election_msg.set_from(1);
    election_msg.set_to(2);
    election_msg.set_term(10);
    
    // From node 1 (gRPC) to node 2 (Iroh)
    let start = std::time::Instant::now();
    nodes[0].1.send_message(2, election_msg.clone()).await?;
    let grpc_latency = start.elapsed();
    info!("gRPC -> Iroh latency: {:?}", grpc_latency);
    
    // From node 2 (Iroh) to node 1 (gRPC)
    election_msg.set_from(2);
    election_msg.set_to(1);
    let start = std::time::Instant::now();
    nodes[1].1.send_message(1, election_msg.clone()).await?;
    let iroh_latency = start.elapsed();
    info!("Iroh -> gRPC latency: {:?}", iroh_latency);
    
    // From node 3 (Dual) to node 1
    election_msg.set_from(3);
    election_msg.set_to(1);
    let start = std::time::Instant::now();
    nodes[2].1.send_message(1, election_msg).await?;
    let dual_latency = start.elapsed();
    info!("Dual -> gRPC latency: {:?}", dual_latency);
    
    Ok(())
}

/// Demo throughput test
async fn demo_throughput(nodes: &[(Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>)]) -> Result<(), Box<dyn std::error::Error>> {
    let message_count = 1000;
    
    // Test gRPC throughput
    let start = std::time::Instant::now();
    for i in 0..message_count {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHeartbeat);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(i);
        nodes[0].1.send_message(2, msg).await?;
    }
    let grpc_duration = start.elapsed();
    let grpc_throughput = message_count as f64 / grpc_duration.as_secs_f64();
    info!("gRPC throughput: {:.2} messages/sec", grpc_throughput);
    
    // Test Iroh throughput
    let start = std::time::Instant::now();
    for i in 0..message_count {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHeartbeat);
        msg.set_from(2);
        msg.set_to(3);
        msg.set_term(i);
        nodes[1].1.send_message(3, msg).await?;
    }
    let iroh_duration = start.elapsed();
    let iroh_throughput = message_count as f64 / iroh_duration.as_secs_f64();
    info!("Iroh throughput: {:.2} messages/sec", iroh_throughput);
    
    info!("Throughput improvement: {:.1}%", (iroh_throughput / grpc_throughput - 1.0) * 100.0);
    
    Ok(())
}

/// Demo large message handling
async fn demo_large_messages(nodes: &[(Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>)]) -> Result<(), Box<dyn std::error::Error>> {
    let sizes = vec![
        ("1MB", 1024 * 1024),
        ("5MB", 5 * 1024 * 1024),
        ("10MB", 10 * 1024 * 1024),
    ];
    
    for (size_name, size) in sizes {
        // Create snapshot message
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgSnapshot);
        msg.set_from(1);
        msg.set_to(2);
        msg.set_term(10);
        
        let mut snapshot = Snapshot::default();
        snapshot.set_data(vec![0u8; size]);
        msg.set_snapshot(snapshot);
        
        // Test gRPC
        let start = std::time::Instant::now();
        nodes[0].1.send_message(2, msg.clone()).await?;
        let grpc_time = start.elapsed();
        
        // Test Iroh
        msg.set_from(2);
        msg.set_to(3);
        let start = std::time::Instant::now();
        nodes[1].1.send_message(3, msg).await?;
        let iroh_time = start.elapsed();
        
        info!("{} snapshot - gRPC: {:?}, Iroh: {:?} (improvement: {:.1}%)",
            size_name, grpc_time, iroh_time,
            (1.0 - iroh_time.as_secs_f64() / grpc_time.as_secs_f64()) * 100.0
        );
    }
    
    Ok(())
}

/// Demo message priority handling
async fn demo_priority_handling(nodes: &[(Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>)]) -> Result<(), Box<dyn std::error::Error>> {
    info!("Sending mixed priority messages to Iroh transport...");
    
    // Send low priority messages first
    for i in 0..20 {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.set_from(2);
        msg.set_to(3);
        msg.set_index(i);
        nodes[1].1.send_message(3, msg).await?;
    }
    
    // Then send high priority election message
    let mut election_msg = Message::default();
    election_msg.set_msg_type(MessageType::MsgRequestVote);
    election_msg.set_from(2);
    election_msg.set_to(3);
    election_msg.set_term(100);
    
    let start = std::time::Instant::now();
    nodes[1].1.send_message(3, election_msg).await?;
    let priority_time = start.elapsed();
    
    info!("High priority message sent after {} low priority messages", 20);
    info!("Time to send high priority message: {:?}", priority_time);
    info!("(Iroh transport prioritizes election messages over log append)");
    
    Ok(())
}