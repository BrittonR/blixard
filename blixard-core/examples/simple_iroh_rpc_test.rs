//! Simple test of the Iroh RPC implementation
//! 
//! This example demonstrates the basic Iroh RPC functionality
//! without requiring the full node infrastructure.

use blixard_core::transport::iroh_protocol::*;
use blixard_core::transport::iroh_service::{IrohService, IrohRpcServer, IrohRpcClient, ServiceRegistry};
use blixard_core::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use bytes::Bytes;
use iroh::{Endpoint, NodeAddr};
use std::sync::Arc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
struct PingRequest {
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PingResponse {
    echo: String,
    node_id: String,
}

/// Simple echo service
struct EchoService {
    node_id: String,
}

#[async_trait]
impl IrohService for EchoService {
    fn name(&self) -> &'static str {
        "echo"
    }
    
    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        match method {
            "ping" => {
                let request: PingRequest = deserialize_payload(&payload)?;
                let response = PingResponse {
                    echo: request.message,
                    node_id: self.node_id.clone(),
                };
                serialize_payload(&response)
            }
            _ => Err(BlixardError::NotFound {
                resource: format!("Method '{}' not found", method),
            }),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("🚀 Starting Simple Iroh RPC Test");
    println!("================================\n");
    
    // Create server endpoint
    println!("Creating server endpoint...");
    let server_endpoint = Endpoint::builder()
        .bind()
        .await?;
    
    let server_node_id = server_endpoint.node_id();
    let server_addr = NodeAddr::new(server_node_id);
    println!("✅ Server node ID: {}", server_node_id);
    
    // Create and configure server
    let server = Arc::new(IrohRpcServer::new(server_endpoint));
    let echo_service = EchoService {
        node_id: server_node_id.to_string(),
    };
    server.register_service(echo_service).await;
    println!("✅ Echo service registered");
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        println!("🔄 Server listening for connections...");
        if let Err(e) = server.serve().await {
            eprintln!("❌ Server error: {}", e);
        }
    });
    
    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client endpoint
    println!("\nCreating client endpoint...");
    let client_endpoint = Endpoint::builder()
        .bind()
        .await?;
    println!("✅ Client endpoint created");
    
    // Create RPC client
    let client = IrohRpcClient::new(client_endpoint);
    
    // Test echo service
    println!("\n📡 Testing Echo Service");
    println!("=======================");
    
    let test_messages = vec![
        "Hello, Iroh!",
        "Testing P2P RPC",
        "🚀 Blazing fast communication",
    ];
    
    for msg in test_messages {
        println!("\n→ Sending: '{}'", msg);
        
        let request = PingRequest {
            message: msg.to_string(),
        };
        
        let start = std::time::Instant::now();
        let response: PingResponse = client
            .call(server_addr.clone(), "echo", "ping", request)
            .await?;
        let latency = start.elapsed();
        
        println!("← Response: '{}'", response.echo);
        println!("  From node: {}", response.node_id);
        println!("  Latency: {:?}", latency);
    }
    
    // Test error handling
    println!("\n🧪 Testing Error Handling");
    println!("========================");
    
    println!("\n→ Calling non-existent method...");
    let request = PingRequest {
        message: "test".to_string(),
    };
    
    match client.call::<_, PingResponse>(
        server_addr.clone(), 
        "echo", 
        "nonexistent", 
        request
    ).await {
        Ok(_) => println!("❌ Expected error but got success"),
        Err(e) => println!("✅ Got expected error: {}", e),
    }
    
    println!("\n→ Calling non-existent service...");
    let request = PingRequest {
        message: "test".to_string(),
    };
    
    match client.call::<_, PingResponse>(
        server_addr.clone(), 
        "nonexistent", 
        "ping", 
        request
    ).await {
        Ok(_) => println!("❌ Expected error but got success"),
        Err(e) => println!("✅ Got expected error: {}", e),
    }
    
    // Performance test
    println!("\n⚡ Performance Test");
    println!("==================");
    
    println!("Running 100 round-trip calls...");
    let mut latencies = Vec::new();
    
    for i in 0..100 {
        let request = PingRequest {
            message: format!("Message {}", i),
        };
        
        let start = std::time::Instant::now();
        let _: PingResponse = client
            .call(server_addr.clone(), "echo", "ping", request)
            .await?;
        latencies.push(start.elapsed());
    }
    
    // Calculate statistics
    latencies.sort();
    let total: std::time::Duration = latencies.iter().sum();
    let avg = total / latencies.len() as u32;
    let p50 = latencies[latencies.len() / 2];
    let p99 = latencies[latencies.len() * 99 / 100];
    
    println!("\n📊 Results:");
    println!("  Total calls: {}", latencies.len());
    println!("  Average latency: {:?}", avg);
    println!("  P50 latency: {:?}", p50);
    println!("  P99 latency: {:?}", p99);
    
    // Cleanup
    println!("\n🧹 Cleaning up...");
    server_handle.abort();
    
    println!("\n✅ Test completed successfully!");
    
    Ok(())
}