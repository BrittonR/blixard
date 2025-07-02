//! Demonstration of using failpoints for fault injection in Blixard
//! 
//! This example shows how to use failpoints for testing and debugging
//! distributed system failures in production-like scenarios.

use blixard_core::{
    error::{BlixardError, BlixardResult},
    failpoints::{self, scenarios},
    fail_point,
};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Simulated storage operations with failpoint injection
struct SimulatedStorage {
    write_count: Arc<AtomicU64>,
    read_count: Arc<AtomicU64>,
}

impl SimulatedStorage {
    fn new() -> Self {
        Self {
            write_count: Arc::new(AtomicU64::new(0)),
            read_count: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Write data with failpoint injection
    fn write(&self, key: &str, value: &[u8]) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("storage::write");
        
        self.write_count.fetch_add(1, Ordering::Relaxed);
        println!("Writing {} bytes to key: {}", value.len(), key);
        
        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");
        
        Ok(())
    }
    
    /// Read data with failpoint injection
    fn read(&self, key: &str) -> BlixardResult<Vec<u8>> {
        #[cfg(feature = "failpoints")]
        fail_point!("storage::read");
        
        self.read_count.fetch_add(1, Ordering::Relaxed);
        println!("Reading from key: {}", key);
        
        Ok(vec![1, 2, 3, 4]) // Dummy data
    }
}

/// Simulated network operations
struct SimulatedNetwork;

impl SimulatedNetwork {
    /// Send message with failpoint injection
    async fn send_message(&self, peer: &str, msg: &[u8]) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("network::send_message");
        
        println!("Sending {} bytes to peer: {}", msg.len(), peer);
        
        // Simulate network delay
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize failpoints system
    #[cfg(feature = "failpoints")]
    failpoints::init();
    
    println!("=== Failpoint Demo ===\n");
    
    // Example 1: Deterministic failures
    demo_deterministic_failures()?;
    
    // Example 2: Probabilistic failures
    demo_probabilistic_failures()?;
    
    // Example 3: Network failures
    demo_network_failures().await?;
    
    // Example 4: Production debugging configuration
    demo_production_debugging()?;
    
    Ok(())
}

fn demo_deterministic_failures() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Deterministic Failures Demo");
    println!("------------------------------");
    
    let storage = SimulatedStorage::new();
    
    // Configure to fail on the 3rd and 5th write
    #[cfg(feature = "failpoints")]
    fail::cfg("storage::write", "2*off->1*return->1*off->1*return->off")?;
    
    for i in 1..=7 {
        let result = storage.write(&format!("key{}", i), b"data");
        match result {
            Ok(_) => println!("  Write {} succeeded", i),
            Err(_) => println!("  Write {} FAILED (expected on 3rd and 5th)", i),
        }
    }
    
    #[cfg(feature = "failpoints")]
    fail::cfg("storage::write", "off")?;
    
    println!("Total writes attempted: {}\n", storage.write_count.load(Ordering::Relaxed));
    Ok(())
}

fn demo_probabilistic_failures() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Probabilistic Failures Demo");
    println!("-----------------------------");
    
    let storage = SimulatedStorage::new();
    
    // 30% failure rate
    #[cfg(feature = "failpoints")]
    scenarios::fail_with_probability("storage::commit_transaction", 0.3);
    
    let mut successes = 0;
    let mut failures = 0;
    
    for i in 1..=20 {
        let result = storage.write(&format!("prob_key{}", i), b"data");
        match result {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }
    
    println!("  Results with 30% failure rate:");
    println!("  Successes: {}, Failures: {}", successes, failures);
    println!("  Actual failure rate: {:.1}%\n", (failures as f64 / 20.0) * 100.0);
    
    #[cfg(feature = "failpoints")]
    scenarios::disable("storage::commit_transaction");
    
    Ok(())
}

async fn demo_network_failures() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Network Failures Demo");
    println!("-----------------------");
    
    let network = SimulatedNetwork;
    
    // Simulate network delays
    #[cfg(feature = "failpoints")]
    fail::cfg("network::send_message", "50%delay(100)")?;
    
    let start = std::time::Instant::now();
    for i in 1..=5 {
        let msg_start = std::time::Instant::now();
        let result = network.send_message(&format!("peer{}", i), b"hello").await;
        let duration = msg_start.elapsed();
        
        match result {
            Ok(_) => println!("  Message {} sent in {:?}", i, duration),
            Err(_) => println!("  Message {} failed", i),
        }
    }
    let total_duration = start.elapsed();
    println!("  Total time for 5 messages: {:?}\n", total_duration);
    
    #[cfg(feature = "failpoints")]
    fail::cfg("network::send_message", "off")?;
    
    Ok(())
}

fn demo_production_debugging() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Production Debugging Configuration");
    println!("------------------------------------");
    
    // Simulate setting failpoints via environment variable
    std::env::set_var("FAILPOINTS", "storage::read=10%return;network::send_message=5%delay(50)");
    
    println!("  Failpoints configured via environment:");
    println!("  - storage::read: 10% failure rate");
    println!("  - network::send_message: 5% with 50ms delay");
    
    // In production, you could also configure failpoints dynamically
    #[cfg(feature = "failpoints")]
    {
        // Enable detailed logging for specific failure
        fail::cfg("storage::write", "print(Storage write intercepted)")?;
        
        let storage = SimulatedStorage::new();
        storage.write("debug_key", b"debug_data").ok();
        
        // Disable after debugging
        fail::cfg("storage::write", "off")?;
    }
    
    println!("\n  Failpoints can be reconfigured at runtime for debugging!");
    
    // Clean up
    std::env::remove_var("FAILPOINTS");
    #[cfg(feature = "failpoints")]
    scenarios::disable_all();
    
    Ok(())
}