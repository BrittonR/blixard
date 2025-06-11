// Comprehensive madsim integration tests for distributed scenarios

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[cfg(feature = "simulation")]
use madsim::{
    Runtime,
    time::{sleep, Duration, Instant},
    net::{TcpListener, TcpStream},
    task,
};

#[cfg(feature = "simulation")]
use madsim::net::NetSim;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Simple distributed key-value store for testing
#[derive(Clone)]
struct KVNode {
    id: usize,
    addr: String,
    store: Arc<Mutex<BTreeMap<String, String>>>,
    peers: Arc<Mutex<Vec<String>>>,
    is_leader: Arc<Mutex<bool>>,
}

impl KVNode {
    fn new(id: usize, addr: String) -> Self {
        Self {
            id,
            addr: addr.clone(),
            store: Arc::new(Mutex::new(BTreeMap::new())),
            peers: Arc::new(Mutex::new(Vec::new())),
            is_leader: Arc::new(Mutex::new(false)),
        }
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("Node {} listening on {}", self.id, self.addr);

        // Simple leader election: lowest ID wins initially
        if self.id == 0 {
            *self.is_leader.lock().unwrap() = true;
        }

        loop {
            tokio::select! {
                result = listener.accept() => {
                    if let Ok((stream, _)) = result {
                        let node = self.clone();
                        task::spawn(async move {
                            let _ = node.handle_connection(stream).await;
                        });
                    }
                }
            }
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;
        
        let request = String::from_utf8_lossy(&buf[..n]);
        let parts: Vec<&str> = request.split_whitespace().collect();
        
        let response = match parts.as_slice() {
            ["GET", key] => {
                let store = self.store.lock().unwrap();
                match store.get(*key) {
                    Some(value) => format!("OK {}", value),
                    None => "NOT_FOUND".to_string(),
                }
            }
            ["SET", key, value] => {
                if *self.is_leader.lock().unwrap() {
                    let mut store = self.store.lock().unwrap();
                    store.insert(key.to_string(), value.to_string());
                    "OK".to_string()
                } else {
                    "NOT_LEADER".to_string()
                }
            }
            ["LEADER"] => {
                if *self.is_leader.lock().unwrap() {
                    "YES".to_string()
                } else {
                    "NO".to_string()
                }
            }
            _ => "INVALID".to_string(),
        };
        
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }

    async fn set_peers(&self, peers: Vec<String>) {
        *self.peers.lock().unwrap() = peers;
    }
}

// Client helper for testing
async fn kv_client_request(addr: &str, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.write_all(request.as_bytes()).await?;
    
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf[..n]).to_string())
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_basic_kv_operations() {
    // Spawn a single node
    let addr = "10.0.0.1:8080";
    let node = KVNode::new(0, addr.to_string());
    
    let handle = task::spawn({
        let node = node.clone();
        async move {
            let _ = node.run().await;
        }
    });
    
    // Wait for startup
    sleep(Duration::from_millis(100)).await;
    
    // Test SET operation
    let response = kv_client_request(addr, "SET key1 value1").await.unwrap();
    assert_eq!(response, "OK");
    
    // Test GET operation
    let response = kv_client_request(addr, "GET key1").await.unwrap();
    assert_eq!(response, "OK value1");
    
    // Test missing key
    let response = kv_client_request(addr, "GET missing").await.unwrap();
    assert_eq!(response, "NOT_FOUND");
    
    handle.abort();
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_network_partition_recovery() {
    let net = NetSim::current();
    
    // Spawn 3-node cluster
    let nodes = vec![
        ("10.0.0.1:8080", 0),
        ("10.0.0.2:8080", 1),
        ("10.0.0.3:8080", 2),
    ];
    
    let mut handles = vec![];
    for (addr, id) in &nodes {
        let node = KVNode::new(*id, addr.to_string());
        
        // Set peers
        let peers: Vec<String> = nodes.iter()
            .filter(|(a, _)| a != addr)
            .map(|(a, _)| a.to_string())
            .collect();
        node.set_peers(peers).await;
        
        let handle = task::spawn({
            let node = node.clone();
            async move {
                let _ = node.run().await;
            }
        });
        handles.push(handle);
    }
    
    // Wait for cluster to stabilize
    sleep(Duration::from_millis(200)).await;
    
    // Verify leader exists
    let leader_check = kv_client_request("10.0.0.1:8080", "LEADER").await.unwrap();
    assert_eq!(leader_check, "YES");
    
    // Create network partition: isolate node 0 (the leader)
    println!("Creating network partition...");
    net.disconnect("10.0.0.1:8080", "10.0.0.2:8080");
    net.disconnect("10.0.0.1:8080", "10.0.0.3:8080");
    
    // Wait for partition to take effect
    sleep(Duration::from_secs(2)).await;
    
    // Try to write to isolated leader (should still work locally)
    let response = kv_client_request("10.0.0.1:8080", "SET key2 value2").await.unwrap();
    assert_eq!(response, "OK");
    
    // Heal the partition
    println!("Healing network partition...");
    net.connect("10.0.0.1:8080", "10.0.0.2:8080");
    net.connect("10.0.0.1:8080", "10.0.0.3:8080");
    
    // Wait for recovery
    sleep(Duration::from_secs(1)).await;
    
    // Verify data is still accessible
    let response = kv_client_request("10.0.0.1:8080", "GET key2").await.unwrap();
    assert_eq!(response, "OK value2");
    
    // Cleanup
    for handle in handles {
        handle.abort();
    }
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_latency_effects() {
    let net = NetSim::current();
    
    // Spawn 2 nodes
    let addr1 = "10.0.0.1:8080";
    let addr2 = "10.0.0.2:8080";
    
    let node1 = KVNode::new(0, addr1.to_string());
    let node2 = KVNode::new(1, addr2.to_string());
    
    let handle1 = task::spawn({
        let node = node1.clone();
        async move {
            let _ = node.run().await;
        }
    });
    
    let handle2 = task::spawn({
        let node = node2.clone();
        async move {
            let _ = node.run().await;
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Add significant latency between nodes
    println!("Adding 500ms latency between nodes");
    net.add_latency(addr1, addr2, Duration::from_millis(500));
    net.add_latency(addr2, addr1, Duration::from_millis(500));
    
    // Measure operation time
    let start = Instant::now();
    let response = kv_client_request(addr1, "SET latency_test value").await.unwrap();
    let elapsed = start.elapsed();
    
    assert_eq!(response, "OK");
    println!("Operation completed in {:?} (simulated time)", elapsed);
    
    // Remove latency
    net.add_latency(addr1, addr2, Duration::from_millis(0));
    net.add_latency(addr2, addr1, Duration::from_millis(0));
    
    // Verify faster operation
    let start = Instant::now();
    let response = kv_client_request(addr1, "GET latency_test").await.unwrap();
    let elapsed_fast = start.elapsed();
    
    assert_eq!(response, "OK value");
    println!("Fast operation completed in {:?}", elapsed_fast);
    
    // In simulation, the latency difference should be observable
    assert!(elapsed > elapsed_fast);
    
    handle1.abort();
    handle2.abort();
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_packet_loss_resilience() {
    let net = NetSim::current();
    
    let addr = "10.0.0.1:8080";
    let node = KVNode::new(0, addr.to_string());
    
    let handle = task::spawn({
        let node = node.clone();
        async move {
            let _ = node.run().await;
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Add 30% packet loss
    println!("Adding 30% packet loss");
    net.set_packet_loss("127.0.0.1:0", addr, 0.3);
    
    // Try multiple operations - some should fail due to packet loss
    let mut successes = 0;
    let mut failures = 0;
    
    for i in 0..10 {
        match kv_client_request(addr, &format!("SET key{} value{}", i, i)).await {
            Ok(response) if response == "OK" => successes += 1,
            _ => failures += 1,
        }
    }
    
    println!("With 30% packet loss: {} successes, {} failures", successes, failures);
    
    // With 30% loss, we expect some failures
    assert!(failures > 0);
    assert!(successes > 0);
    
    // Remove packet loss
    net.set_packet_loss("127.0.0.1:0", addr, 0.0);
    
    // All operations should succeed now
    let mut all_success = true;
    for i in 10..15 {
        match kv_client_request(addr, &format!("SET key{} value{}", i, i)).await {
            Ok(response) if response == "OK" => {},
            _ => all_success = false,
        }
    }
    
    assert!(all_success);
    
    handle.abort();
}

#[cfg(feature = "simulation")]
#[test]
fn test_deterministic_execution() {
    // Run the same test twice with the same seed
    let seed = 42;
    
    let rt1 = Runtime::with_seed(seed);
    let result1 = rt1.block_on(async {
        let mut values = vec![];
        
        // Generate some "random" values
        for _ in 0..5 {
            let value: u32 = madsim::rand::random();
            values.push(value);
        }
        
        // Simulate some timed operations
        let start = Instant::now();
        sleep(Duration::from_millis(100)).await;
        let elapsed = start.elapsed();
        
        (values, elapsed)
    });
    
    let rt2 = Runtime::with_seed(seed);
    let result2 = rt2.block_on(async {
        let mut values = vec![];
        
        // Generate the same "random" values
        for _ in 0..5 {
            let value: u32 = madsim::rand::random();
            values.push(value);
        }
        
        // Same timed operations
        let start = Instant::now();
        sleep(Duration::from_millis(100)).await;
        let elapsed = start.elapsed();
        
        (values, elapsed)
    });
    
    // Results should be identical
    assert_eq!(result1.0, result2.0, "Random values should be deterministic");
    assert_eq!(result1.1, result2.1, "Elapsed time should be deterministic");
    println!("Deterministic test passed: {:?}", result1.0);
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_node_crash_and_recovery() {
    let runtime_handle = madsim::runtime::Handle::current();
    
    // Spawn node
    let addr = "10.0.0.1:8080";
    let node = KVNode::new(0, addr.to_string());
    
    let handle = task::spawn({
        let node = node.clone();
        async move {
            let _ = node.run().await;
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Store some data
    let response = kv_client_request(addr, "SET important_data critical_value").await.unwrap();
    assert_eq!(response, "OK");
    
    // Simulate node crash
    println!("Simulating node crash...");
    runtime_handle.kill(addr);
    handle.abort();
    
    // Try to connect - should fail
    let result = kv_client_request(addr, "GET important_data").await;
    assert!(result.is_err(), "Should not be able to connect to crashed node");
    
    // "Restart" the node
    println!("Restarting node...");
    let new_node = KVNode::new(0, addr.to_string());
    let _new_handle = task::spawn(async move {
        let _ = new_node.run().await;
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Node is back but data is lost (no persistence in this simple example)
    let response = kv_client_request(addr, "GET important_data").await.unwrap();
    assert_eq!(response, "NOT_FOUND");
    
    // But we can write new data
    let response = kv_client_request(addr, "SET new_data new_value").await.unwrap();
    assert_eq!(response, "OK");
}

#[cfg(feature = "simulation")]
#[madsim::test] 
async fn test_time_manipulation() {
    // Test that we can manipulate time in simulation
    let start = Instant::now();
    
    // "Sleep" for 1 hour - in simulation this is instant
    println!("Sleeping for 1 hour (simulated)...");
    sleep(Duration::from_secs(3600)).await;
    
    let elapsed = start.elapsed();
    println!("Elapsed time: {:?}", elapsed);
    
    // Should be approximately 1 hour in simulated time
    assert!(elapsed >= Duration::from_secs(3600));
    assert!(elapsed < Duration::from_secs(3700)); // Some tolerance
    
    // Multiple quick operations
    let operation_start = Instant::now();
    for i in 0..1000 {
        if i % 100 == 0 {
            sleep(Duration::from_millis(10)).await;
        }
    }
    let operation_time = operation_start.elapsed();
    
    println!("1000 operations with periodic sleeps took: {:?}", operation_time);
    assert!(operation_time >= Duration::from_millis(100)); // At least 10 sleeps of 10ms
}

// Helper function to demonstrate chaos testing patterns
#[cfg(feature = "simulation")]
async fn chaos_scenario(addresses: Vec<&str>) {
    let net = NetSim::current();
    
    // Random network disruptions
    for _ in 0..5 {
        let i = madsim::rand::random::<usize>() % addresses.len();
        let j = madsim::rand::random::<usize>() % addresses.len();
        
        if i != j {
            // Random action
            match madsim::rand::random::<u8>() % 3 {
                0 => {
                    println!("Disconnecting {} from {}", addresses[i], addresses[j]);
                    net.disconnect(addresses[i], addresses[j]);
                }
                1 => {
                    let latency = Duration::from_millis(madsim::rand::random::<u64>() % 200);
                    println!("Adding {}ms latency between {} and {}", latency.as_millis(), addresses[i], addresses[j]);
                    net.add_latency(addresses[i], addresses[j], latency);
                }
                2 => {
                    let loss = (madsim::rand::random::<u8>() % 30) as f64 / 100.0;
                    println!("Setting {}% packet loss between {} and {}", (loss * 100.0) as u8, addresses[i], addresses[j]);
                    net.set_packet_loss(addresses[i], addresses[j], loss);
                }
                _ => unreachable!(),
            }
        }
        
        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(feature = "simulation")]
#[madsim::test]
async fn test_chaos_engineering() {
    // Spawn a small cluster
    let addresses = vec!["10.0.0.1:8080", "10.0.0.2:8080", "10.0.0.3:8080"];
    let mut handles = vec![];
    
    for (i, addr) in addresses.iter().enumerate() {
        let node = KVNode::new(i, addr.to_string());
        let handle = task::spawn({
            let node = node.clone();
            async move {
                let _ = node.run().await;
            }
        });
        handles.push(handle);
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // Run chaos scenario
    let chaos_handle = task::spawn(chaos_scenario(addresses.clone()));
    
    // Try to perform operations during chaos
    let mut successes = 0;
    let mut failures = 0;
    
    for i in 0..20 {
        // Try random nodes
        let node_idx = i % addresses.len();
        let addr = addresses[node_idx];
        
        match kv_client_request(addr, &format!("SET chaos_key{} value{}", i, i)).await {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    println!("During chaos: {} successes, {} failures", successes, failures);
    
    // System should handle some operations despite chaos
    assert!(successes > 0, "Should have some successful operations during chaos");
    
    // Wait for chaos to end
    chaos_handle.await.unwrap();
    
    // Cleanup
    for handle in handles {
        handle.abort();
    }
}