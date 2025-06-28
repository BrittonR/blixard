//! Benchmarks for Raft transport performance comparison
//!
//! This module provides benchmarks to compare Raft consensus performance
//! over gRPC vs Iroh P2P transport.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use raft::prelude::*;

use crate::transport::raft_transport_adapter::RaftTransport;
use crate::transport::config::{TransportConfig, GrpcConfig, IrohConfig, RaftTransportPreference};
use crate::node_shared::SharedNodeState;
use crate::types::NodeConfig;

/// Benchmark message types
#[derive(Clone)]
struct BenchmarkScenario {
    name: &'static str,
    messages: Vec<Message>,
}

impl BenchmarkScenario {
    /// Create election scenario (high priority messages)
    fn election() -> Self {
        let mut messages = Vec::new();
        
        // Request vote messages
        for i in 1..=10 {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgRequestVote);
            msg.set_from(1);
            msg.set_to(i % 3 + 2); // Send to nodes 2, 3, 1
            msg.set_term(10);
            messages.push(msg);
        }
        
        // Vote responses
        for i in 1..=10 {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgRequestVoteResponse);
            msg.set_from(i % 3 + 2);
            msg.set_to(1);
            msg.set_term(10);
            messages.push(msg);
        }
        
        Self {
            name: "election",
            messages,
        }
    }
    
    /// Create heartbeat scenario (medium priority)
    fn heartbeat() -> Self {
        let mut messages = Vec::new();
        
        // Heartbeat messages from leader
        for i in 1..=50 {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgHeartbeat);
            msg.set_from(1);
            msg.set_to(i % 3 + 2);
            msg.set_term(10);
            msg.set_commit(100);
            messages.push(msg);
        }
        
        // Heartbeat responses
        for i in 1..=50 {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgHeartbeatResponse);
            msg.set_from(i % 3 + 2);
            msg.set_to(1);
            msg.set_term(10);
            messages.push(msg);
        }
        
        Self {
            name: "heartbeat",
            messages,
        }
    }
    
    /// Create log replication scenario (normal priority)
    fn log_replication() -> Self {
        let mut messages = Vec::new();
        
        // Append entries with various sizes
        for i in 1..=20 {
            let mut msg = Message::default();
            msg.set_msg_type(MessageType::MsgAppend);
            msg.set_from(1);
            msg.set_to(i % 3 + 2);
            msg.set_term(10);
            msg.set_log_term(10);
            msg.set_index(100 + i as u64);
            msg.set_commit(100);
            
            // Add entries of varying sizes
            let mut entries = vec![];
            for j in 0..5 {
                let mut entry = Entry::default();
                entry.index = 100 + i as u64 + j;
                entry.term = 10;
                entry.data = vec![0u8; 1024 * (j + 1) as usize]; // 1KB to 5KB
                entries.push(entry);
            }
            msg.set_entries(entries.into());
            
            messages.push(msg);
        }
        
        Self {
            name: "log_replication",
            messages,
        }
    }
    
    /// Create mixed workload
    fn mixed() -> Self {
        let mut messages = Vec::new();
        
        // Mix of all message types
        let election = Self::election();
        let heartbeat = Self::heartbeat();
        let log_repl = Self::log_replication();
        
        messages.extend(election.messages);
        messages.extend(heartbeat.messages);
        messages.extend(log_repl.messages);
        
        Self {
            name: "mixed",
            messages,
        }
    }
}

/// Benchmark transport setup
async fn setup_transport(
    transport_type: &str,
) -> (Arc<SharedNodeState>, RaftTransport, mpsc::UnboundedReceiver<(u64, Message)>) {
    // Create node configuration
    let config = NodeConfig {
        id: 1,
        bind_address: "127.0.0.1:7001".to_string(),
        data_dir: std::env::temp_dir().join(format!("blixard-bench-{}", uuid::Uuid::new_v4())),
        vm_backend: "test".to_string(),
        join_address: None,
        bootstrap: true,
    };
    
    let node = Arc::new(SharedNodeState::new(config));
    
    // Create channel for receiving messages
    let (tx, rx) = mpsc::unbounded_channel();
    
    // Create transport config
    let transport_config = match transport_type {
        "grpc" => TransportConfig::Grpc(GrpcConfig::default()),
        "iroh" => TransportConfig::Iroh(IrohConfig::default()),
        _ => panic!("Unknown transport type"),
    };
    
    // Create transport
    let transport = RaftTransport::new(node.clone(), tx, &transport_config)
        .await
        .expect("Failed to create transport");
    
    (node, transport, rx)
}

/// Benchmark sending messages
fn bench_send_messages(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let scenarios = vec![
        BenchmarkScenario::election(),
        BenchmarkScenario::heartbeat(),
        BenchmarkScenario::log_replication(),
        BenchmarkScenario::mixed(),
    ];
    
    let transports = vec!["grpc", "iroh"];
    
    let mut group = c.benchmark_group("raft_transport_send");
    group.measurement_time(Duration::from_secs(10));
    
    for scenario in &scenarios {
        for transport_type in &transports {
            group.bench_with_input(
                BenchmarkId::new(scenario.name, transport_type),
                &(scenario, transport_type),
                |b, (scenario, transport_type)| {
                    b.to_async(&runtime).iter(|| async {
                        let (_node, transport, _rx) = setup_transport(transport_type).await;
                        
                        // Send all messages
                        for msg in &scenario.messages {
                            let _ = transport.send_message(msg.to, msg.clone()).await;
                        }
                        
                        // Cleanup
                        transport.shutdown().await;
                    });
                },
            );
        }
    }
    group.finish();
}

/// Benchmark message throughput
fn bench_throughput(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("raft_transport_throughput");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(criterion::Throughput::Elements(1000)); // Messages per iteration
    
    for transport_type in &["grpc", "iroh"] {
        group.bench_with_input(
            BenchmarkId::from_parameter(transport_type),
            transport_type,
            |b, transport_type| {
                b.to_async(&runtime).iter(|| async {
                    let (_node, transport, _rx) = setup_transport(transport_type).await;
                    
                    // Create 1000 small messages
                    for i in 0..1000 {
                        let mut msg = Message::default();
                        msg.set_msg_type(MessageType::MsgHeartbeat);
                        msg.set_from(1);
                        msg.set_to(2);
                        msg.set_term(i as u64);
                        
                        let _ = transport.send_message(2, msg).await;
                    }
                    
                    transport.shutdown().await;
                });
            },
        );
    }
    group.finish();
}

/// Benchmark large message handling (snapshots)
fn bench_large_messages(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("raft_transport_large");
    group.measurement_time(Duration::from_secs(15));
    
    let sizes = vec![
        ("1MB", 1024 * 1024),
        ("10MB", 10 * 1024 * 1024),
        ("50MB", 50 * 1024 * 1024),
    ];
    
    for (size_name, size) in sizes {
        for transport_type in &["grpc", "iroh"] {
            group.bench_with_input(
                BenchmarkId::new(size_name, transport_type),
                &(size, transport_type),
                |b, (size, transport_type)| {
                    b.to_async(&runtime).iter(|| async {
                        let (_node, transport, _rx) = setup_transport(transport_type).await;
                        
                        // Create snapshot message
                        let mut msg = Message::default();
                        msg.set_msg_type(MessageType::MsgSnapshot);
                        msg.set_from(1);
                        msg.set_to(2);
                        msg.set_term(10);
                        
                        let mut snapshot = Snapshot::default();
                        snapshot.set_data(vec![0u8; *size]);
                        msg.set_snapshot(snapshot);
                        
                        let _ = transport.send_message(2, msg).await;
                        
                        transport.shutdown().await;
                    });
                },
            );
        }
    }
    group.finish();
}

/// Benchmark message prioritization
fn bench_prioritization(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("raft_transport_priority");
    group.measurement_time(Duration::from_secs(10));
    
    group.bench_function("iroh_priority", |b| {
        b.to_async(&runtime).iter(|| async {
            let (_node, transport, _rx) = setup_transport("iroh").await;
            
            // Send mixed priority messages
            let mut messages = Vec::new();
            
            // Low priority (log append)
            for i in 0..10 {
                let mut msg = Message::default();
                msg.set_msg_type(MessageType::MsgAppend);
                msg.set_from(1);
                msg.set_to(2);
                msg.set_index(i);
                messages.push(msg);
            }
            
            // High priority (election)
            let mut vote_msg = Message::default();
            vote_msg.set_msg_type(MessageType::MsgRequestVote);
            vote_msg.set_from(1);
            vote_msg.set_to(2);
            vote_msg.set_term(100);
            messages.push(vote_msg);
            
            // Send all messages
            for msg in messages {
                let _ = transport.send_message(2, msg).await;
            }
            
            transport.shutdown().await;
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_send_messages,
    bench_throughput,
    bench_large_messages,
    bench_prioritization
);
criterion_main!(benches);