//! Comprehensive Raft transport comparison between gRPC and Iroh

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::mpsc;
use raft::prelude::*;

use blixard_core::transport::raft_transport_adapter::RaftTransport;
use blixard_core::transport::config::{TransportConfig, GrpcConfig, IrohConfig, MigrationStrategy};
use rand;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::NodeConfig;
use blixard_core::error::BlixardResult;
use blixard_core::storage::Storage;
use redb::Database;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
struct BenchmarkResult {
    transport: String,
    scenario: String,
    messages_sent: usize,
    total_duration: Duration,
    avg_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    throughput_msgs_per_sec: f64,
    errors: usize,
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=info,iroh=info")
        .init();

    println!("=== Raft Transport Performance Comparison ===\n");
    println!("Comparing gRPC vs Iroh P2P transport for Raft consensus\n");

    let mut results = Vec::new();

    // Test scenarios
    let scenarios = vec![
        ("election", create_election_messages(100)),
        ("heartbeat", create_heartbeat_messages(1000)),
        ("log_replication", create_log_replication_messages(100)),
        ("mixed_workload", create_mixed_workload()),
    ];

    // Test both transports
    for transport_type in &["grpc", "iroh", "dual"] {
        println!("\n--- Testing {} transport ---", transport_type);
        
        for (scenario_name, messages) in &scenarios {
            println!("  Running {} scenario ({} messages)...", scenario_name, messages.len());
            
            let result = run_benchmark(transport_type, scenario_name, messages.clone()).await?;
            println!("    ✓ Avg latency: {:?}, Throughput: {:.0} msg/s", 
                result.avg_latency, result.throughput_msgs_per_sec);
            
            results.push(result);
        }
    }

    // Print summary
    print_benchmark_summary(&results);

    Ok(())
}

async fn run_benchmark(
    transport_type: &str,
    scenario: &str,
    messages: Vec<Message>,
) -> BlixardResult<BenchmarkResult> {
    // Create test nodes
    let nodes = create_test_nodes(transport_type, 3).await?;
    
    // Give nodes time to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Track latencies
    let mut latencies = Vec::new();
    let start = Instant::now();
    let mut errors = 0;
    
    // Send messages from node 1 to others
    let transport = &nodes[0].1;
    
    for msg in &messages {
        let msg_start = Instant::now();
        
        // Send to appropriate peer (round-robin for testing)
        let to_node = if msg.to == 0 { 2 } else { msg.to };
        
        match transport.send_message(to_node, msg.clone()).await {
            Ok(_) => {
                let latency = msg_start.elapsed();
                latencies.push(latency);
            }
            Err(e) => {
                tracing::debug!("Send error: {}", e);
                errors += 1;
            }
        }
        
        // Small delay between messages to avoid overwhelming
        tokio::time::sleep(Duration::from_micros(100)).await;
    }
    
    let total_duration = start.elapsed();
    
    // Calculate statistics
    latencies.sort();
    let avg_latency = if !latencies.is_empty() {
        Duration::from_nanos(
            latencies.iter().map(|d| d.as_nanos() as u64).sum::<u64>() / latencies.len() as u64
        )
    } else {
        Duration::from_secs(0)
    };
    
    let p50 = latencies.get(latencies.len() / 2).copied().unwrap_or_default();
    let p95 = latencies.get(latencies.len() * 95 / 100).copied().unwrap_or_default();
    let p99 = latencies.get(latencies.len() * 99 / 100).copied().unwrap_or_default();
    
    let throughput = messages.len() as f64 / total_duration.as_secs_f64();
    
    // Cleanup
    for (node, transport) in nodes {
        transport.shutdown().await;
        let _ = tokio::fs::remove_dir_all(&node.get_data_dir()).await;
    }
    
    Ok(BenchmarkResult {
        transport: transport_type.to_string(),
        scenario: scenario.to_string(),
        messages_sent: messages.len(),
        total_duration,
        avg_latency,
        p50_latency: p50,
        p95_latency: p95,
        p99_latency: p99,
        throughput_msgs_per_sec: throughput,
        errors,
    })
}

async fn create_test_nodes(
    transport_type: &str,
    count: usize,
) -> BlixardResult<Vec<(Arc<SharedNodeState>, RaftTransport)>> {
    let mut nodes = Vec::new();
    
    for i in 1..=count {
        let port = 7000 + i as u16;
        let config = NodeConfig {
            id: i as u64,
            bind_addr: format!("127.0.0.1:{}", port).parse::<SocketAddr>().unwrap(),
            data_dir: std::env::temp_dir()
                .join(format!("blixard-bench-{}-{}", transport_type, i))
                .to_string_lossy()
                .to_string(),
            vm_backend: "test".to_string(),
            join_addr: if i > 1 { Some("127.0.0.1:7001".to_string()) } else { None },
            use_tailscale: false,
            transport_config: Some(create_transport_config(transport_type)),
        };
        
        let node = Arc::new(SharedNodeState::new(config));
        
        // Initialize database
        let db_path = node.get_data_dir().join("test.db");
        let database = Arc::new(Database::create(&db_path)?);
        node.set_database(database.clone()).await;
        
        // Initialize storage
        let storage = Storage::new(node.get_id(), database.clone(), node.clone()).await?;
        node.set_storage(Arc::new(storage)).await;
        
        // Add peer info for other nodes
        for j in 1..=count {
            if i != j {
                let peer_id = j as u64;
                let peer_addr = format!("127.0.0.1:{}", 7000 + j);
                
                // For Iroh, we need P2P node IDs
                if transport_type == "iroh" || transport_type == "dual" {
                    // Generate deterministic node ID for testing
                    let node_id = format!("node{}", j);
                    node.add_peer(peer_id, peer_addr.clone()).await;
                } else {
                    node.add_peer(peer_id, peer_addr).await;
                }
            }
        }
        
        // Initialize Iroh if needed
        if transport_type == "iroh" || transport_type == "dual" {
            let secret_key = iroh::SecretKey::generate(rand::thread_rng());
            let endpoint = iroh::Endpoint::builder()
                .secret_key(secret_key)
                .bind()
                .await
                .map_err(|e| blixard_core::error::BlixardError::Internal {
                    message: format!("Failed to bind Iroh endpoint: {}", e)
                })?;
            
            // Store endpoint info in node's p2p_node_id field for identification
            let p2p_node_id = endpoint.node_id().to_string();
            node.set_p2p_node_id(p2p_node_id).await;
        }
        
        // Create transport
        let (tx, _rx) = mpsc::unbounded_channel();
        let transport_config = create_transport_config(transport_type);
        let transport = RaftTransport::new(node.clone(), tx, &transport_config).await?;
        
        // Start maintenance
        transport.start_maintenance().await;
        
        nodes.push((node, transport));
    }
    
    Ok(nodes)
}

fn create_transport_config(transport_type: &str) -> TransportConfig {
    match transport_type {
        "grpc" => TransportConfig::Grpc(GrpcConfig::default()),
        "iroh" => TransportConfig::Iroh(IrohConfig::default()),
        "dual" => TransportConfig::Dual {
            grpc_config: GrpcConfig::default(),
            iroh_config: IrohConfig::default(),
            strategy: MigrationStrategy::default(),
        },
        _ => panic!("Unknown transport type"),
    }
}

fn create_election_messages(count: usize) -> Vec<Message> {
    let mut messages = Vec::new();
    
    for i in 0..count {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgRequestVote);
        msg.set_from(1);
        msg.set_to((i % 2) as u64 + 2);
        msg.set_term((i / 10) as u64 + 1);
        msg.set_log_term((i / 10) as u64);
        msg.set_index(i as u64);
        messages.push(msg);
    }
    
    messages
}

fn create_heartbeat_messages(count: usize) -> Vec<Message> {
    let mut messages = Vec::new();
    
    for i in 0..count {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgHeartbeat);
        msg.set_from(1);
        msg.set_to((i % 2) as u64 + 2);
        msg.set_term(10);
        msg.set_commit(i as u64);
        messages.push(msg);
    }
    
    messages
}

fn create_log_replication_messages(count: usize) -> Vec<Message> {
    let mut messages = Vec::new();
    
    for i in 0..count {
        let mut msg = Message::default();
        msg.set_msg_type(MessageType::MsgAppend);
        msg.set_from(1);
        msg.set_to((i % 2) as u64 + 2);
        msg.set_term(10);
        msg.set_log_term(10);
        msg.set_index(100 + i as u64);
        msg.set_commit(100);
        
        // Add entries
        let mut entries = vec![];
        for j in 0..5 {
            let mut entry = Entry::default();
            entry.index = 100 + i as u64 + j;
            entry.term = 10;
            entry.data = vec![0u8; 1024]; // 1KB entries
            entries.push(entry);
        }
        msg.set_entries(entries.into());
        
        messages.push(msg);
    }
    
    messages
}

fn create_mixed_workload() -> Vec<Message> {
    let mut messages = Vec::new();
    
    // Mix of all message types
    messages.extend(create_election_messages(20));
    messages.extend(create_heartbeat_messages(100));
    messages.extend(create_log_replication_messages(30));
    
    // Shuffle to simulate real workload
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    messages.shuffle(&mut rng);
    
    messages
}

fn print_benchmark_summary(results: &[BenchmarkResult]) {
    println!("\n\n=== Benchmark Summary ===\n");
    
    // Group by scenario
    let mut by_scenario: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
    for result in results {
        by_scenario.entry(result.scenario.clone()).or_default().push(result);
    }
    
    for (scenario, scenario_results) in &by_scenario {
        println!("Scenario: {}", scenario);
        println!("{:<10} {:>15} {:>15} {:>15} {:>15} {:>15} {:>8}",
            "Transport", "Avg Latency", "P50", "P95", "P99", "Throughput", "Errors");
        println!("{:-<100}", "");
        
        for result in scenario_results {
            println!("{:<10} {:>15} {:>15} {:>15} {:>15} {:>12.0} msg/s {:>8}",
                result.transport,
                format!("{:?}", result.avg_latency),
                format!("{:?}", result.p50_latency),
                format!("{:?}", result.p95_latency),
                format!("{:?}", result.p99_latency),
                result.throughput_msgs_per_sec,
                result.errors
            );
        }
        println!();
    }
    
    // Calculate improvements
    println!("\n--- Performance Comparison ---");
    for (scenario, scenario_results) in &by_scenario {
        if let (Some(grpc), Some(iroh)) = (
            scenario_results.iter().find(|r| r.transport == "grpc"),
            scenario_results.iter().find(|r| r.transport == "iroh")
        ) {
            let latency_improvement = (grpc.avg_latency.as_micros() as f64 - iroh.avg_latency.as_micros() as f64) 
                / grpc.avg_latency.as_micros() as f64 * 100.0;
            let throughput_improvement = (iroh.throughput_msgs_per_sec - grpc.throughput_msgs_per_sec) 
                / grpc.throughput_msgs_per_sec * 100.0;
            
            println!("{} scenario:", scenario);
            println!("  Latency: {:.1}% {} (gRPC: {:?}, Iroh: {:?})",
                latency_improvement.abs(),
                if latency_improvement > 0.0 { "better" } else { "worse" },
                grpc.avg_latency,
                iroh.avg_latency
            );
            println!("  Throughput: {:.1}% {} (gRPC: {:.0} msg/s, Iroh: {:.0} msg/s)",
                throughput_improvement.abs(),
                if throughput_improvement > 0.0 { "better" } else { "worse" },
                grpc.throughput_msgs_per_sec,
                iroh.throughput_msgs_per_sec
            );
        }
    }
}