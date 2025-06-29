//! Test Iroh connection establishment and basic performance

use std::time::{Duration, Instant};
use blixard_core::error::BlixardResult;
use iroh::{Endpoint, NodeAddr, SecretKey};
use rand;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Iroh Connection Performance Test ===\n");
    
    // Test 1: Endpoint creation time
    test_endpoint_creation().await?;
    
    // Test 2: Connection establishment time
    test_connection_establishment().await?;
    
    // Test 3: Multiple connections to same endpoint
    test_connection_reuse().await?;
    
    // Test 4: Concurrent connections
    test_concurrent_connections().await?;
    
    println!("\n✅ All tests completed!");
    Ok(())
}

async fn test_endpoint_creation() -> BlixardResult<()> {
    println!("1. Testing endpoint creation time...");
    
    let mut creation_times = Vec::new();
    
    for i in 0..10 {
        let start = Instant::now();
        
        let secret = SecretKey::generate(rand::thread_rng());
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to create endpoint: {}", e)
            })?;
        
        let creation_time = start.elapsed();
        creation_times.push(creation_time);
        
        if i == 0 {
            println!("   Node ID: {}", endpoint.node_id());
            println!("   First endpoint: {:?}", creation_time);
        }
        
        // Cleanup
        endpoint.close().await;
    }
    
    let avg = creation_times.iter().sum::<Duration>() / creation_times.len() as u32;
    println!("   Average creation time: {:?}\n", avg);
    
    Ok(())
}

async fn test_connection_establishment() -> BlixardResult<()> {
    println!("2. Testing connection establishment...");
    
    // Create two endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let secret2 = SecretKey::generate(rand::thread_rng());
    
    let endpoint1 = create_endpoint(secret1).await?;
    let endpoint2 = create_endpoint(secret2).await?;
    
    // Start accepting connections on endpoint2
    let endpoint2_clone = endpoint2.clone();
    let accept_task = tokio::spawn(async move {
        while let Some(incoming) = endpoint2_clone.accept().await {
            match incoming.accept() {
                Ok(_conn) => {
                    // Connection accepted
                }
                Err(e) => eprintln!("Failed to accept connection: {}", e),
            }
        }
    });
    
    // Give the accept task time to start
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    let node_addr = NodeAddr::new(endpoint2.node_id());
    let mut connection_times = Vec::new();
    
    // Test multiple connections
    for i in 0..20 {
        let start = Instant::now();
        
        match endpoint1.connect(node_addr.clone(), b"test").await {
            Ok(conn) => {
                let connect_time = start.elapsed();
                connection_times.push(connect_time);
                
                if i == 0 {
                    println!("   First connection: {:?} (includes handshake)", connect_time);
                    println!("   Connection established");
                }
                
                // Close connection
                conn.close(0u32.into(), b"done");
            }
            Err(e) => {
                eprintln!("   Connection {} failed: {}", i, e);
            }
        }
        
        // Small delay between connections
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    
    if !connection_times.is_empty() {
        connection_times.sort();
        let avg = connection_times.iter().sum::<Duration>() / connection_times.len() as u32;
        let p50 = connection_times[connection_times.len() / 2];
        let p99 = connection_times[connection_times.len() * 99 / 100];
        
        println!("   Connection times - Avg: {:?}, P50: {:?}, P99: {:?}", avg, p50, p99);
    }
    
    // Cleanup
    accept_task.abort();
    endpoint1.close().await;
    endpoint2.close().await;
    
    println!();
    Ok(())
}

async fn test_connection_reuse() -> BlixardResult<()> {
    println!("3. Testing connection reuse...");
    
    let secret1 = SecretKey::generate(rand::thread_rng());
    let secret2 = SecretKey::generate(rand::thread_rng());
    
    let endpoint1 = create_endpoint(secret1).await?;
    let endpoint2 = create_endpoint(secret2).await?;
    
    // Accept connections
    let endpoint2_clone = endpoint2.clone();
    tokio::spawn(async move {
        while let Some(incoming) = endpoint2_clone.accept().await {
            let _ = incoming.accept();
        }
    });
    
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    let node_addr = NodeAddr::new(endpoint2.node_id());
    
    // Establish initial connection
    let start = Instant::now();
    let connection = endpoint1.connect(node_addr, b"test").await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to connect: {}", e)
        })?;
    let initial_time = start.elapsed();
    
    println!("   Initial connection: {:?}", initial_time);
    
    // Open multiple streams on same connection
    let mut stream_times = Vec::new();
    
    for i in 0..50 {
        let start = Instant::now();
        
        match connection.open_bi().await {
            Ok((send, recv)) => {
                let stream_time = start.elapsed();
                stream_times.push(stream_time);
                
                if i == 0 {
                    println!("   First stream: {:?}", stream_time);
                }
                
                // Close streams
                drop(send);
                drop(recv);
            }
            Err(e) => {
                eprintln!("   Stream {} failed: {}", i, e);
            }
        }
    }
    
    if !stream_times.is_empty() {
        let avg = stream_times.iter().sum::<Duration>() / stream_times.len() as u32;
        println!("   Average stream creation: {:?} (much faster than new connections)", avg);
    }
    
    endpoint1.close().await;
    endpoint2.close().await;
    
    println!();
    Ok(())
}

async fn test_concurrent_connections() -> BlixardResult<()> {
    println!("4. Testing concurrent connections...");
    
    let secret = SecretKey::generate(rand::thread_rng());
    let server_endpoint = create_endpoint(secret).await?;
    let server_addr = NodeAddr::new(server_endpoint.node_id());
    
    // Accept connections
    let endpoint_clone = server_endpoint.clone();
    tokio::spawn(async move {
        while let Some(incoming) = endpoint_clone.accept().await {
            let _ = incoming.accept();
        }
    });
    
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Create multiple client endpoints
    let mut tasks = Vec::new();
    let start = Instant::now();
    
    for i in 0..10 {
        let server_addr = server_addr.clone();
        let task = tokio::spawn(async move {
            let secret = SecretKey::generate(rand::thread_rng());
            let endpoint = create_endpoint(secret).await?;
            
            let conn_start = Instant::now();
            let _conn = endpoint.connect(server_addr, b"test").await
                .map_err(|e| blixard_core::error::BlixardError::Internal {
                    message: format!("Client {} failed to connect: {}", i, e)
                })?;
            let conn_time = conn_start.elapsed();
            
            endpoint.close().await;
            Ok::<Duration, blixard_core::error::BlixardError>(conn_time)
        });
        
        tasks.push(task);
    }
    
    // Wait for all connections
    let mut results = Vec::new();
    for task in tasks {
        if let Ok(Ok(time)) = task.await {
            results.push(time);
        }
    }
    
    let total_time = start.elapsed();
    
    if !results.is_empty() {
        let avg = results.iter().sum::<Duration>() / results.len() as u32;
        println!("   {} concurrent connections in {:?}", results.len(), total_time);
        println!("   Average per connection: {:?}", avg);
        println!("   Parallelism speedup: {:.1}x", 
            (avg.as_micros() as f64 * results.len() as f64) / total_time.as_micros() as f64);
    }
    
    server_endpoint.close().await;
    
    Ok(())
}

async fn create_endpoint(secret: SecretKey) -> BlixardResult<Endpoint> {
    Endpoint::builder()
        .secret_key(secret)
        .bind()
        .await
        .map_err(|e| blixard_core::error::BlixardError::Internal {
            message: format!("Failed to create endpoint: {}", e)
        })
}