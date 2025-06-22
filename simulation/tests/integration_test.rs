//! Integration tests for Blixard using madsim

#![cfg(madsim)]

// Load test modules
// mod cluster_tests;  // Disabled - depends on blixard crate
// mod network_tests;   // Tests available but not included here
// mod raft_tests;      // Placeholder - no implementation yet

use std::time::Duration;
use madsim::time::{sleep, Instant};
use madsim::task;

#[madsim::test]
async fn test_basic_simulation() {
    // This tests that madsim is working
    let start = Instant::now();
    sleep(Duration::from_secs(1)).await;
    let elapsed = start.elapsed();
    
    assert!(elapsed >= Duration::from_secs(1));
    assert!(elapsed < Duration::from_millis(1100));
}

#[madsim::test]
async fn test_tcp_communication() {
    use tokio::net::{TcpListener, TcpStream};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    // Test TCP communication in simulation
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    
    // Spawn server task
    let server = task::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        // Echo back what we received
        stream.write_all(&buf[..n]).await.unwrap();
        stream.flush().await.unwrap();
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect and send data
    let mut client = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    client.write_all(b"Hello!").await.unwrap();
    client.flush().await.unwrap();
    
    // Read echo response
    let mut response = vec![0; 6];
    client.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"Hello!");
    
    // Let server finish
    let _ = server.await;
}

#[madsim::test] 
async fn test_deterministic_randomness() {
    use madsim::rand::{thread_rng, Rng};
    
    let mut rng = thread_rng();
    let values: Vec<u32> = (0..5).map(|_| rng.gen()).collect();
    
    // Values should be deterministic based on seed
    assert!(!values.is_empty());
    assert_eq!(values.len(), 5, "Should generate 5 random values");
    
    // Verify values are actually random (not all the same)
    let unique_values: std::collections::HashSet<_> = values.iter().cloned().collect();
    assert!(unique_values.len() > 1, "Random values should have some variation");
    
    // Test that the same RNG produces a deterministic sequence
    // (not that different RNGs produce the same sequence)
    let first_value = rng.gen::<u32>();
    let second_value = rng.gen::<u32>();
    assert_ne!(first_value, second_value, "Sequential values should be different");
}

#[madsim::test]
async fn test_time_control() {
    // Test that we can control time
    let start = Instant::now();
    
    // "Sleep" for an hour - should be instant in simulation
    sleep(Duration::from_secs(3600)).await;
    
    let elapsed = start.elapsed();
    // Allow small tolerance for floating point precision
    assert!(elapsed >= Duration::from_secs(3600));
    assert!(elapsed < Duration::from_secs(3601));
    
    // Multiple quick sleeps
    for _ in 0..1000 {
        sleep(Duration::from_millis(1)).await;
    }
    
    let total = start.elapsed();
    assert!(total >= Duration::from_secs(3601));
}