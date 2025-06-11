// This example demonstrates how madsim can be used for deterministic testing
// Run with: cargo run --example madsim_demo --features simulation

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[madsim::main]
async fn main() {
    println!("MadSim Deterministic Simulation Demo");
    println!("=====================================");
    
    // Demonstrate deterministic time
    println!("\n1. Deterministic Time Control:");
    let start = madsim::time::Instant::now();
    println!("   Start time: {:?}", start);
    
    // Sleep for 5 seconds (happens instantly in simulation)
    madsim::time::sleep(Duration::from_secs(5)).await;
    let elapsed = start.elapsed();
    println!("   After sleep(5s), elapsed: {:?}", elapsed);
    
    // Demonstrate deterministic task spawning
    println!("\n2. Deterministic Task Spawning:");
    let counter = Arc::new(AtomicU64::new(0));
    
    let mut handles = vec![];
    for i in 0..5 {
        let counter_clone = counter.clone();
        let handle = madsim::task::spawn(async move {
            // Each task sleeps for a different duration
            madsim::time::sleep(Duration::from_millis(i * 100)).await;
            counter_clone.fetch_add(1, Ordering::SeqCst);
            println!("   Task {} completed", i);
        });
        handles.push(handle);
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }
    
    println!("   Final counter value: {}", counter.load(Ordering::SeqCst));
    
    // Demonstrate network simulation capabilities
    println!("\n3. Network Simulation (TCP Echo Server):");
    
    // Start echo server
    madsim::task::spawn(async {
        use tokio::net::TcpListener;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        
        let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
        println!("   Echo server listening on 127.0.0.1:8080");
        
        if let Ok((mut socket, addr)) = listener.accept().await {
            println!("   Accepted connection from: {}", addr);
            let mut buf = [0; 1024];
            if let Ok(n) = socket.read(&mut buf).await {
                println!("   Received: {}", String::from_utf8_lossy(&buf[..n]));
                socket.write_all(&buf[..n]).await.unwrap();
            }
        }
    });
    
    // Give server time to start
    madsim::time::sleep(Duration::from_millis(100)).await;
    
    // Connect as client
    use tokio::net::TcpStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    let mut stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    println!("   Client connected");
    
    let message = b"Hello, MadSim!";
    stream.write_all(message).await.unwrap();
    println!("   Client sent: {}", String::from_utf8_lossy(message));
    
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    println!("   Client received: {}", String::from_utf8_lossy(&buf[..n]));
    
    println!("\nDemo completed!");
}

// Additional tests to show madsim features
#[cfg(test)]
mod tests {
    use super::*;
    
    #[madsim::test]
    async fn test_deterministic_execution() {
        // Run the same test multiple times - results should be identical
        let result1 = simulate_with_seed(42).await;
        let result2 = simulate_with_seed(42).await;
        
        assert_eq!(result1, result2, "Results should be deterministic");
    }
    
    async fn simulate_with_seed(seed: u64) -> Vec<u64> {
        let mut results = vec![];
        
        // Simulate some async operations
        for i in 0..5 {
            madsim::time::sleep(Duration::from_millis(i * 10)).await;
            results.push(seed + i as u64);
        }
        
        results
    }
}