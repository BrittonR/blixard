// Example demonstrating gRPC with madsim deterministic testing
// Run with: RUSTFLAGS="--cfg madsim" cargo run --example grpc_madsim_demo --features simulation

use blixard::grpc_client::GrpcClient;
use blixard::grpc_server::{blixard::*, GrpcServer};
use blixard::node::Node;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::storage::Storage;
use blixard::types::NodeConfig;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tonic::transport::Server;

#[madsim::main]
async fn main() {
    println!("MadSim + Tonic gRPC Demo");
    println!("=========================");

    // Create temp directory for storage
    let temp_dir = TempDir::new().unwrap();
    
    // Create simulated runtime with deterministic seed
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    println!("\n1. Setting up gRPC server with simulated runtime...");
    
    // Configure server node
    let server_config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().join("server").to_str().unwrap().to_string(),
        bind_addr: "127.0.0.1:50051".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    // Create storage and node
    let storage = Arc::new(Storage::new(temp_dir.path().join("server.db")).unwrap());
    let node = Node::new_with_storage(server_config, HashMap::new(), storage)
        .await
        .unwrap();
    let node = Arc::new(RwLock::new(node));
    
    // Create gRPC server
    let server = GrpcServer::new(node.clone(), runtime.clone());
    
    // Start server in background
    madsim::task::spawn(async move {
        println!("   Server starting on 127.0.0.1:50051");
        let service = cluster_service_server::ClusterServiceServer::new(server);
        Server::builder()
            .add_service(service)
            .serve("127.0.0.1:50051".parse().unwrap())
            .await
            .unwrap();
    });
    
    // Wait for server to start (in simulation, this is instant)
    madsim::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n2. Creating gRPC client...");
    
    // Create client
    let mut client = GrpcClient::connect("http://127.0.0.1:50051".to_string(), runtime.clone())
        .await
        .unwrap();
    
    println!("   Client connected!");
    
    println!("\n3. Testing RPC calls...");
    
    // Test health check
    println!("   - Health check...");
    let start = madsim::time::Instant::now();
    let healthy = client.health_check().await.unwrap();
    let elapsed = start.elapsed();
    println!("     Result: {} (took {:?})", if healthy { "✓ Healthy" } else { "✗ Unhealthy" }, elapsed);
    
    // Test cluster status
    println!("   - Cluster status...");
    let start = madsim::time::Instant::now();
    let status = client.get_cluster_status().await.unwrap();
    let elapsed = start.elapsed();
    println!("     Leader ID: {} (took {:?})", status.leader_id, elapsed);
    
    // Test VM operations
    println!("   - VM operations...");
    let vm_list = client.list_vms().await.unwrap();
    println!("     Current VMs: {}", vm_list.len());
    
    println!("\n4. Demonstrating deterministic time...");
    
    // Show that operations happen instantly in simulation
    let start = madsim::time::Instant::now();
    
    // Make multiple RPC calls
    for i in 0..5 {
        client.health_check().await.unwrap();
        println!("   RPC {} completed at {:?}", i + 1, start.elapsed());
    }
    
    println!("\n5. Testing concurrent clients...");
    
    // Create multiple clients
    let mut handles = vec![];
    for i in 0..3 {
        let runtime_clone = runtime.clone();
        let handle = madsim::task::spawn(async move {
            let mut client = GrpcClient::connect(
                "http://127.0.0.1:50051".to_string(),
                runtime_clone,
            )
            .await
            .unwrap();
            
            for j in 0..3 {
                client.health_check().await.unwrap();
                println!("   Client {} completed request {}", i + 1, j + 1);
            }
        });
        handles.push(handle);
    }
    
    // Wait for all clients
    for handle in handles {
        handle.await.unwrap();
    }
    
    println!("\nDemo completed!");
    println!("\nKey takeaways:");
    println!("- gRPC works seamlessly with madsim's deterministic runtime");
    println!("- All network operations are simulated and deterministic");
    println!("- Time advances only when explicitly waited on");
    println!("- Perfect for testing distributed systems with gRPC");
}