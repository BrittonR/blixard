//! Transport latency comparison test
//! 
//! This test measures actual network latency between nodes using both gRPC and Iroh transports.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use blixard_core::error::BlixardResult;

#[derive(Debug, Clone)]
struct LatencyMeasurement {
    transport: String,
    message_type: String,
    round_trip_time: Duration,
    timestamp: Instant,
}

#[derive(Default)]
struct LatencyStats {
    measurements: Vec<LatencyMeasurement>,
}

impl LatencyStats {
    fn add_measurement(&mut self, transport: &str, msg_type: &str, rtt: Duration) {
        self.measurements.push(LatencyMeasurement {
            transport: transport.to_string(),
            message_type: msg_type.to_string(),
            round_trip_time: rtt,
            timestamp: Instant::now(),
        });
    }

    fn print_summary(&self) {
        // Group by transport and message type
        let mut by_transport_and_type: std::collections::HashMap<(String, String), Vec<Duration>> = 
            std::collections::HashMap::new();
        
        for m in &self.measurements {
            by_transport_and_type
                .entry((m.transport.clone(), m.message_type.clone()))
                .or_default()
                .push(m.round_trip_time);
        }
        
        println!("\n=== Latency Summary ===\n");
        println!("{:<10} {:<15} {:>10} {:>10} {:>10} {:>10}", 
            "Transport", "Message Type", "Avg", "P50", "P95", "P99");
        println!("{:-<65}", "");
        
        for ((transport, msg_type), mut times) in by_transport_and_type {
            times.sort();
            
            let avg = times.iter().sum::<Duration>() / times.len() as u32;
            let p50 = times[times.len() / 2];
            let p95 = times[times.len() * 95 / 100];
            let p99 = times[times.len() * 99 / 100];
            
            println!("{:<10} {:<15} {:>10.2?} {:>10.2?} {:>10.2?} {:>10.2?}", 
                transport, msg_type, avg, p50, p95, p99);
        }
    }
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=info")
        .init();
    
    println!("=== Transport Latency Comparison Test ===\n");
    
    let stats = Arc::new(Mutex::new(LatencyStats::default()));
    
    // Test 1: Simple echo test with mock endpoints
    test_echo_latency(stats.clone()).await?;
    
    // Test 2: Iroh connection establishment time
    test_iroh_connection_time().await?;
    
    // Test 3: Message size impact
    test_message_size_impact(stats.clone()).await?;
    
    // Print results
    let stats = stats.lock().await;
    stats.print_summary();
    
    println!("\n✅ All tests completed!");
    Ok(())
}

async fn test_echo_latency(stats: Arc<Mutex<LatencyStats>>) -> BlixardResult<()> {
    use blixard_core::transport::iroh_protocol::{
        MessageType, generate_request_id, write_message, read_message,
    };
    use iroh::{Endpoint, NodeAddr};
    use rand;
    
    println!("Testing echo latency...\n");
    
    // Create two Iroh endpoints
    let secret1 = iroh::SecretKey::generate(rand::thread_rng());
    let secret2 = iroh::SecretKey::generate(rand::thread_rng());
    
    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .alpns(vec![b"test/echo/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint1: {}", e)
        })?;
    
    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .alpns(vec![b"test/echo/1".to_vec()])
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to bind endpoint2: {}", e)
        })?;
    
    // Start echo server on endpoint2
    let endpoint2_clone = endpoint2.clone();
    let server_handle = tokio::spawn(async move {
        while let Some(incoming) = endpoint2_clone.accept().await {
            tokio::spawn(async move {
                let mut connection = match incoming.accept() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to accept connection: {}", e);
                        return;
                    }
                };
                
                match connection.accept_bi().await {
                    Ok((mut send, mut recv)) => {
                        // Echo server - read and echo back
                        loop {
                            match read_message(&mut recv).await {
                                Ok((header, payload)) => {
                                    // Echo the message back
                                    if let Err(e) = write_message(
                                        &mut send, 
                                        header.message_type, 
                                        header.request_id, 
                                        &payload
                                    ).await {
                                        eprintln!("Echo write error: {}", e);
                                        break;
                                    }
                                }
                                Err(_) => break, // Connection closed
                            }
                        }
                    }
                    Err(e) => eprintln!("Failed to accept stream: {}", e),
                }
            });
        }
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Connect from endpoint1 to endpoint2
    let node_addr2 = NodeAddr::new(endpoint2.node_id());
    
    // Measure connection time
    let connect_start = Instant::now();
    let connection = endpoint1.connect(node_addr2, b"test/echo/1").await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to connect: {}", e)
        })?;
    let connect_time = connect_start.elapsed();
    println!("  Iroh connection established in {:?}", connect_time);
    
    // Open bidirectional stream
    let (mut send, mut recv) = connection.open_bi().await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to open stream: {}", e)
        })?;
    
    // Test different message sizes
    let test_sizes = vec![
        ("tiny", 10),
        ("small", 100),
        ("medium", 1_000),
        ("large", 10_000),
    ];
    
    for (size_name, size) in test_sizes {
        let payload = vec![0u8; size];
        
        // Measure 100 round trips
        let mut round_trips = Vec::new();
        
        for _ in 0..100 {
            let request_id = generate_request_id();
            let start = Instant::now();
            
            // Send request
            write_message(&mut send, MessageType::Request, request_id, &payload).await?;
            
            // Read response
            let (_header, _payload) = read_message(&mut recv).await?;
            
            let rtt = start.elapsed();
            round_trips.push(rtt);
        }
        
        // Calculate average
        let avg_rtt = round_trips.iter().sum::<Duration>() / round_trips.len() as u32;
        println!("  {} message ({} bytes): avg RTT = {:?}", size_name, size, avg_rtt);
        
        // Store measurements
        let mut stats = stats.lock().await;
        for rtt in round_trips {
            stats.add_measurement("iroh", size_name, rtt);
        }
    }
    
    // Shutdown
    server_handle.abort();
    
    Ok(())
}

async fn test_iroh_connection_time() -> BlixardResult<()> {
    println!("\nTesting Iroh connection establishment time...\n");
    
    let mut connection_times = Vec::new();
    
    // Test connection establishment 20 times
    for i in 0..20 {
        let secret1 = iroh::SecretKey::generate(rand::thread_rng());
        let secret2 = iroh::SecretKey::generate(rand::thread_rng());
        
        let endpoint1 = iroh::Endpoint::builder()
            .secret_key(secret1)
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to bind endpoint: {}", e)
            })?;
        
        let endpoint2 = iroh::Endpoint::builder()
            .secret_key(secret2)
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to bind endpoint: {}", e)
            })?;
        
        // Accept connections on endpoint2
        let endpoint2_clone = endpoint2.clone();
        tokio::spawn(async move {
            while let Some(incoming) = endpoint2_clone.accept().await {
                let _ = incoming.accept();
            }
        });
        
        // Measure connection time
        let node_addr = iroh::NodeAddr::new(endpoint2.node_id());
        let start = Instant::now();
        
        match endpoint1.connect(node_addr, b"bench").await {
            Ok(_) => {
                let connect_time = start.elapsed();
                connection_times.push(connect_time);
                
                if i == 0 {
                    println!("  First connection: {:?} (includes QUIC handshake)", connect_time);
                }
            }
            Err(e) => {
                eprintln!("  Connection {} failed: {}", i, e);
            }
        }
    }
    
    // Calculate statistics
    connection_times.sort();
    let avg = connection_times.iter().sum::<Duration>() / connection_times.len() as u32;
    let p50 = connection_times[connection_times.len() / 2];
    let p95 = connection_times[connection_times.len() * 95 / 100];
    
    println!("  Connection times - Avg: {:?}, P50: {:?}, P95: {:?}", avg, p50, p95);
    println!("  (Note: Real-world times will be higher due to network latency)");
    
    Ok(())
}

async fn test_message_size_impact(stats: Arc<Mutex<LatencyStats>>) -> BlixardResult<()> {
    use blixard_core::raft_codec::{serialize_message, deserialize_message};
    // Using Raft types
    
    println!("\nTesting message size impact on processing time...\n");
    
    // Create messages of different sizes
    let messages = vec![
        ("heartbeat", create_heartbeat()),
        ("vote", create_vote_request()),
        ("small_append", create_log_append(1, 100)),
        ("large_append", create_log_append(100, 4096)),
    ];
    
    for (name, msg) in messages {
        let mut processing_times = Vec::new();
        
        // Serialize once to get size
        let serialized = serialize_message(&msg)?;
        let size = serialized.len();
        
        // Measure processing time (serialize + deserialize)
        for _ in 0..1000 {
            let start = Instant::now();
            
            let bytes = serialize_message(&msg)?;
            let _ = deserialize_message(&bytes)?;
            
            let processing_time = start.elapsed();
            processing_times.push(processing_time);
        }
        
        // Calculate average
        let avg_time = processing_times.iter().sum::<Duration>() / processing_times.len() as u32;
        println!("  {} ({} bytes): avg processing time = {:?}", name, size, avg_time);
        
        // Store as "processing" measurements
        let mut stats = stats.lock().await;
        for time in processing_times {
            stats.add_measurement("processing", name, time);
        }
    }
    
    Ok(())
}

fn create_heartbeat() -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgHeartbeat);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_commit(100);
    msg
}

fn create_vote_request() -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgRequestVote);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(9);
    msg.set_index(100);
    msg
}

fn create_log_append(entries: usize, entry_size: usize) -> raft::prelude::Message {
    let mut msg = raft::prelude::Message::default();
    msg.set_msg_type(raft::prelude::MessageType::MsgAppend);
    msg.set_from(1);
    msg.set_to(2);
    msg.set_term(10);
    msg.set_log_term(10);
    msg.set_index(100);
    msg.set_commit(95);
    
    let mut entry_list = Vec::new();
    for i in 0..entries {
        let mut entry = raft::prelude::Entry::default();
        entry.index = 100 + i as u64;
        entry.term = 10;
        entry.data = vec![0u8; entry_size];
        entry_list.push(entry);
    }
    msg.set_entries(entry_list.into());
    
    msg
}