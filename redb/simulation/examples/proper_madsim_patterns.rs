//! Proper MadSim usage patterns for distributed system testing

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use madsim::{runtime::Runtime, Config};
use madsim::net::{NetSim, Config as NetConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

/// Distributed counter with leader election
struct DistributedCounter {
    id: u64,
    value: Arc<RwLock<i64>>,
    is_leader: Arc<RwLock<bool>>,
    peers: Vec<String>,
}

impl DistributedCounter {
    fn new(id: u64, peers: Vec<String>) -> Self {
        Self {
            id,
            value: Arc::new(RwLock::new(0)),
            is_leader: Arc::new(RwLock::new(false)),
            peers,
        }
    }
    
    async fn start_leader_election(&self) {
        info!("Node {} starting leader election", self.id);
        
        // Simple leader election: lowest ID wins
        let mut is_leader = true;
        for peer in &self.peers {
            if let Ok(addr) = peer.parse::<SocketAddr>() {
                match tokio::time::timeout(
                    Duration::from_millis(100),
                    TcpStream::connect(addr)
                ).await {
                    Ok(Ok(_)) => {
                        // Peer is alive, check if it has lower ID
                        if addr.port() < 8080 + self.id as u16 {
                            is_leader = false;
                        }
                    }
                    _ => {
                        info!("Peer {} unreachable", peer);
                    }
                }
            }
        }
        
        *self.is_leader.write().await = is_leader;
        if is_leader {
            info!("Node {} became leader", self.id);
        }
    }
    
    async fn increment(&self) -> Option<i64> {
        if *self.is_leader.read().await {
            let mut val = self.value.write().await;
            *val += 1;
            let new_val = *val;
            info!("Leader {} incremented counter to {}", self.id, new_val);
            Some(new_val)
        } else {
            warn!("Node {} is not leader, cannot increment", self.id);
            None
        }
    }
    
    async fn get_value(&self) -> i64 {
        *self.value.read().await
    }
}

/// Example: Cluster with leader election and network partitions
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("=== MadSim Distributed System Patterns ===\n");
    
    // Pattern 1: Basic cluster with network configuration
    test_configured_cluster().await;
    
    // Pattern 2: Leader election with partitions
    test_leader_election_with_partitions().await;
    
    // Pattern 3: Chaos engineering
    test_chaos_engineering().await;
}

async fn test_configured_cluster() {
    println!("1. Testing cluster with packet loss and latency");
    
    let mut config = Config::default();
    config.net = NetConfig {
        packet_loss_rate: 0.05, // 5% packet loss
        send_latency: Duration::from_millis(5)..Duration::from_millis(25),
    };
    
    let runtime = Runtime::with_config(config);
    
    runtime.block_on(async {
        let mut nodes = vec![];
        let mut counters = vec![];
        
        // Create 3 nodes
        for i in 0..3 {
            let ip = format!("10.0.0.{}", i + 1);
            let node = runtime.create_node()
                .name(&format!("node{}", i))
                .ip(ip.parse().unwrap())
                .build();
            
            let peers = (0..3)
                .filter(|&j| j != i)
                .map(|j| format!("10.0.0.{}:808{}", j + 1, j))
                .collect();
            
            let counter = DistributedCounter::new(i, peers);
            
            nodes.push(node);
            counters.push(counter);
        }
        
        // Start each node
        let mut tasks = vec![];
        for (i, (node, counter)) in nodes.iter().zip(&counters).enumerate() {
            let counter = DistributedCounter::new(i as u64, counter.peers.clone());
            let task = node.spawn(async move {
                // Start leader election
                counter.start_leader_election().await;
                
                // Try some operations
                for _ in 0..5 {
                    counter.increment().await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                
                counter.get_value().await
            });
            tasks.push(task);
        }
        
        // Wait for all nodes
        let results = futures::future::join_all(tasks).await;
        
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(value) => println!("   Node {} final value: {}", i, value),
                Err(e) => println!("   Node {} error: {}", i, e),
            }
        }
    });
}

async fn test_leader_election_with_partitions() {
    println!("\n2. Testing leader election with network partitions");
    
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        // Create 5 nodes
        let mut nodes = vec![];
        for i in 0..5 {
            let ip = format!("10.0.0.{}", i + 1);
            let node = runtime.create_node()
                .name(&format!("node{}", i))
                .ip(ip.parse().unwrap())
                .build();
            nodes.push(node);
        }
        
        // Start election process on each node
        let mut tasks = vec![];
        for (i, node) in nodes.iter().enumerate() {
            let node_id = node.id();
            let task = node.spawn(async move {
                let counter = DistributedCounter::new(
                    i as u64,
                    (0..5).filter(|&j| j != i)
                        .map(|j| format!("10.0.0.{}:808{}", j + 1, j))
                        .collect()
                );
                
                // Initial election
                counter.start_leader_election().await;
                let initial_leader = *counter.is_leader.read().await;
                
                // Wait a bit
                tokio::time::sleep(Duration::from_millis(200)).await;
                
                // Re-election after potential partition
                counter.start_leader_election().await;
                let final_leader = *counter.is_leader.read().await;
                
                (initial_leader, final_leader)
            });
            tasks.push((node_id, task));
        }
        
        // Create partition after 100ms
        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("Creating network partition");
        
        // Partition: nodes 0,1 vs nodes 2,3,4
        net.clog_link(nodes[0].id(), nodes[2].id());
        net.clog_link(nodes[0].id(), nodes[3].id());
        net.clog_link(nodes[0].id(), nodes[4].id());
        net.clog_link(nodes[1].id(), nodes[2].id());
        net.clog_link(nodes[1].id(), nodes[3].id());
        net.clog_link(nodes[1].id(), nodes[4].id());
        
        // Wait for re-election
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        // Heal partition
        info!("Healing network partition");
        net.unclog_link(nodes[0].id(), nodes[2].id());
        net.unclog_link(nodes[0].id(), nodes[3].id());
        net.unclog_link(nodes[0].id(), nodes[4].id());
        net.unclog_link(nodes[1].id(), nodes[2].id());
        net.unclog_link(nodes[1].id(), nodes[3].id());
        net.unclog_link(nodes[1].id(), nodes[4].id());
        
        // Collect results
        let results = futures::future::join_all(
            tasks.into_iter().map(|(_, task)| task)
        ).await;
        
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok((initial, final_leader)) => {
                    println!("   Node {}: initial_leader={}, final_leader={}", 
                             i, initial, final_leader);
                }
                Err(e) => println!("   Node {} error: {}", i, e),
            }
        }
    });
}

async fn test_chaos_engineering() {
    println!("\n3. Testing chaos engineering scenarios");
    
    let mut config = Config::default();
    config.net = NetConfig {
        packet_loss_rate: 0.2, // 20% packet loss
        send_latency: Duration::from_millis(10)..Duration::from_millis(100),
    };
    
    let runtime = Runtime::with_config(config);
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        // Create service nodes
        let service_node = runtime.create_node()
            .name("service")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let client_node = runtime.create_node()
            .name("client")
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        let service_id = service_node.id();
        let client_id = client_node.id();
        
        // Start resilient service
        let service_task = service_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            let mut request_count = 0;
            
            loop {
                match tokio::time::timeout(
                    Duration::from_millis(500),
                    listener.accept()
                ).await {
                    Ok(Ok((mut stream, _))) => {
                        request_count += 1;
                        let mut buf = [0; 1024];
                        if let Ok(n) = stream.read(&mut buf).await {
                            if n > 0 {
                                // Echo back with request count
                                let response = format!("Response {}", request_count);
                                stream.write_all(response.as_bytes()).await.ok();
                                info!("Service handled request {}", request_count);
                            }
                        }
                    }
                    _ => {
                        // Timeout or error - continue serving
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                
                if request_count >= 10 {
                    break;
                }
            }
            
            request_count
        });
        
        // Start chaos client
        let chaos_task = client_node.spawn(async {
            let mut successful_requests = 0;
            let mut failed_requests = 0;
            
            for i in 0..20 {
                match tokio::time::timeout(
                    Duration::from_secs(2),
                    async {
                        let mut stream = TcpStream::connect("10.0.0.1:8080").await?;
                        stream.write_all(format!("Request {}", i).as_bytes()).await?;
                        
                        let mut response = String::new();
                        let mut buf = [0; 1024];
                        let n = stream.read(&mut buf).await?;
                        response.push_str(&String::from_utf8_lossy(&buf[..n]));
                        
                        Ok::<String, std::io::Error>(response)
                    }
                ).await {
                    Ok(Ok(response)) => {
                        successful_requests += 1;
                        info!("Client: successful request {}: {}", i, response);
                    }
                    _ => {
                        failed_requests += 1;
                        warn!("Client: failed request {}", i);
                    }
                }
                
                // Random delay
                tokio::time::sleep(Duration::from_millis(50 + i * 10)).await;
            }
            
            (successful_requests, failed_requests)
        });
        
        // Inject chaos: randomly partition nodes
        let chaos_injector = tokio::spawn(async move {
            for i in 0..10 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                
                if i % 3 == 0 {
                    info!("Chaos: partitioning nodes");
                    net.clog_node(service_id);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    net.unclog_node(service_id);
                } else if i % 3 == 1 {
                    info!("Chaos: blocking client");
                    net.clog_node(client_id);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    net.unclog_node(client_id);
                }
                // else: no chaos this round
            }
        });
        
        let (service_result, client_result, _) = tokio::join!(
            service_task,
            chaos_task,
            chaos_injector
        );
        
        let requests_handled = service_result.unwrap();
        let (successful, failed) = client_result.unwrap();
        
        println!("   Chaos results:");
        println!("     Service handled: {} requests", requests_handled);
        println!("     Client: {} successful, {} failed out of 20", successful, failed);
        println!("     Success rate: {:.1}%", (successful as f64 / 20.0) * 100.0);
        
        // Under chaos, we expect some failures but the system should be resilient
        assert!(successful > 0, "System completely failed under chaos");
        assert!(requests_handled > 0, "Service handled no requests");
    });
    
    println!("\n✅ All MadSim patterns demonstrated successfully!");
}