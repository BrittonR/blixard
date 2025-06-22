//! Simple gRPC client to check cluster status

use tonic::transport::Channel;
use blixard_core::proto::{
    cluster_service_client::ClusterServiceClient,
    ClusterStatusRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get server address from command line
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:7001".to_string());
    
    // Connect to the gRPC server
    let channel = Channel::from_shared(addr.clone())?.connect().await?;
    let mut client = ClusterServiceClient::new(channel);
    
    // Get cluster status
    let response = client.get_cluster_status(ClusterStatusRequest {}).await?;
    let status = response.into_inner();
    
    println!("Cluster Status from {}", addr);
    println!("Leader ID: {}", status.leader_id);
    println!("Current Term: {}", status.term);
    println!("Nodes in cluster: {}", status.nodes.len());
    
    for node in status.nodes {
        let state = match node.state {
            0 => "Unknown",
            1 => "Leader",
            2 => "Follower", 
            3 => "Candidate",
            _ => "Invalid",
        };
        println!("  Node {}: {} ({})", node.id, node.address, state);
    }
    
    Ok(())
}