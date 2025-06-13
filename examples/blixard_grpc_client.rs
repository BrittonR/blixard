//! Example gRPC client for BlixardService
//!
//! This demonstrates how to use the BlixardService gRPC API.

use tonic::transport::Channel;
use blixard::proto::{
    blixard_service_client::BlixardServiceClient,
    GetRaftStatusRequest, ProposeTaskRequest, Task,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the gRPC server
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:7001".to_string());
    
    println!("Connecting to {}", addr);
    let channel = Channel::from_shared(addr)?.connect().await?;
    let mut client = BlixardServiceClient::new(channel);
    
    // Get Raft status
    println!("\n=== Getting Raft Status ===");
    let response = client.get_raft_status(GetRaftStatusRequest {}).await?;
    let status = response.into_inner();
    
    println!("Node ID: {}", status.node_id);
    println!("Is Leader: {}", status.is_leader);
    println!("Leader ID: {}", status.leader_id);
    println!("Term: {}", status.term);
    println!("State: {}", status.state);
    
    // Propose a task
    println!("\n=== Proposing Task ===");
    let task = Task {
        id: format!("example-task-{}", chrono::Utc::now().timestamp()),
        command: "echo".to_string(),
        args: vec!["Hello from BlixardService!".to_string()],
        cpu_cores: 1,
        memory_mb: 256,
    };
    
    let response = client.propose_task(ProposeTaskRequest {
        task: Some(task.clone()),
    }).await?;
    
    let result = response.into_inner();
    println!("Success: {}", result.success);
    println!("Message: {}", result.message);
    
    // Try proposing an invalid task
    println!("\n=== Testing Invalid Task ===");
    let invalid_task = Task {
        id: "".to_string(), // Empty ID
        command: "test".to_string(),
        args: vec![],
        cpu_cores: 1,
        memory_mb: 256,
    };
    
    let response = client.propose_task(ProposeTaskRequest {
        task: Some(invalid_task),
    }).await?;
    
    let result = response.into_inner();
    println!("Success: {}", result.success);
    println!("Message: {}", result.message);
    
    Ok(())
}