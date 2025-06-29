//! Test Iroh networking capabilities and performance
//!
//! This test focuses on local performance characteristics that don't require
//! actual network connectivity.

use std::time::{Duration, Instant};
use blixard_core::error::BlixardResult;
use iroh::{Endpoint, SecretKey};
use rand;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Iroh Networking Test ===\n");
    
    // Test 1: Endpoint creation performance
    test_endpoint_creation().await?;
    
    // Test 2: NodeId generation and parsing
    test_node_id_operations().await?;
    
    // Test 3: Multiple endpoints on same machine
    test_local_endpoints().await?;
    
    // Test 4: Message codec performance (our custom protocol)
    test_message_codec().await?;
    
    println!("\n✅ All tests completed!");
    Ok(())
}

async fn test_endpoint_creation() -> BlixardResult<()> {
    println!("1. Testing endpoint creation performance...");
    
    let mut creation_times = Vec::new();
    let mut node_ids = Vec::new();
    
    for i in 0..20 {
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
        node_ids.push(endpoint.node_id());
        
        if i == 0 {
            println!("   First endpoint: {:?}", creation_time);
            println!("   Node ID: {}", endpoint.node_id());
        }
        
        endpoint.close().await;
    }
    
    creation_times.sort();
    let avg = creation_times.iter().sum::<Duration>() / creation_times.len() as u32;
    let p50 = creation_times[creation_times.len() / 2];
    let p99 = creation_times[creation_times.len() * 99 / 100];
    
    println!("   Created {} endpoints", creation_times.len());
    println!("   Times - Avg: {:?}, P50: {:?}, P99: {:?}", avg, p50, p99);
    
    // Check node ID uniqueness
    let unique_count = node_ids.iter().collect::<std::collections::HashSet<_>>().len();
    println!("   All {} node IDs are unique: {}", node_ids.len(), unique_count == node_ids.len());
    
    println!();
    Ok(())
}

async fn test_node_id_operations() -> BlixardResult<()> {
    println!("2. Testing NodeId operations...");
    
    // Generate and parse node IDs
    let mut parse_times = Vec::new();
    
    for _ in 0..1000 {
        let secret = SecretKey::generate(rand::thread_rng());
        let node_id = secret.public();
        let id_string = node_id.to_string();
        
        let start = Instant::now();
        let _parsed = id_string.parse::<iroh::NodeId>()
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to parse NodeId: {}", e)
            })?;
        let parse_time = start.elapsed();
        
        parse_times.push(parse_time);
    }
    
    let avg = parse_times.iter().sum::<Duration>() / parse_times.len() as u32;
    println!("   Average NodeId parse time: {:?}", avg);
    println!("   NodeId string length: {} bytes", SecretKey::generate(rand::thread_rng()).public().to_string().len());
    
    println!();
    Ok(())
}

async fn test_local_endpoints() -> BlixardResult<()> {
    println!("3. Testing multiple local endpoints...");
    
    let start = Instant::now();
    let mut endpoints = Vec::new();
    
    // Create 10 endpoints
    for i in 0..10 {
        let secret = SecretKey::generate(rand::thread_rng());
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to create endpoint {}: {}", i, e)
            })?;
        
        endpoints.push(endpoint);
    }
    
    let creation_time = start.elapsed();
    println!("   Created {} endpoints in {:?}", endpoints.len(), creation_time);
    println!("   Average per endpoint: {:?}", creation_time / endpoints.len() as u32);
    
    // Get local addresses (if available)
    for (i, endpoint) in endpoints.iter().enumerate() {
        if i < 3 {  // Just show first 3
            println!("   Endpoint {}: {}", i, endpoint.node_id());
        }
    }
    
    // Cleanup
    for endpoint in endpoints {
        endpoint.close().await;
    }
    
    println!();
    Ok(())
}

async fn test_message_codec() -> BlixardResult<()> {
    use blixard_core::transport::iroh_protocol::{
        MessageType, MessageHeader, generate_request_id
    };
    use std::mem::size_of;
    
    println!("4. Testing our custom message codec...");
    
    // Test header size
    println!("   MessageHeader size: {} bytes", size_of::<MessageHeader>());
    
    // Test request ID generation
    let mut id_times = Vec::new();
    for _ in 0..10000 {
        let start = Instant::now();
        let _id = generate_request_id();
        let time = start.elapsed();
        id_times.push(time);
    }
    
    let avg = id_times.iter().sum::<Duration>() / id_times.len() as u32;
    println!("   Request ID generation: {:?} avg", avg);
    
    // Test different message types
    println!("\n   Message type encoding:");
    let types = vec![
        (MessageType::Request, "Request"),
        (MessageType::Response, "Response"),
        (MessageType::Error, "Error"),
        (MessageType::StreamData, "StreamData"),
        (MessageType::StreamEnd, "StreamEnd"),
        // Note: Heartbeat is for internal use
    ];
    
    for (msg_type, name) in types {
        println!("     {} = {}", name, msg_type as u8);
    }
    
    // Raft message priorities are internal to the transport
    println!("\n   Our protocol supports prioritized Raft messages:");
    println!("     - Election messages (highest priority)");
    println!("     - Heartbeat messages");
    println!("     - Log append messages");
    println!("     - Snapshot transfers (lowest priority)");
    
    Ok(())
}