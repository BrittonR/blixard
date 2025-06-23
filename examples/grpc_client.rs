//! Example gRPC client for testing Blixard server

use blixard_core::proto::{
    cluster_service_client::ClusterServiceClient,
    HealthCheckRequest, CreateVmRequest, ListVmsRequest, ClusterStatusRequest,
    StartVmRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the server
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:7001").await?;
    
    // Test health check
    println!("Testing health check...");
    let response = client.health_check(HealthCheckRequest {}).await?;
    println!("Health check response: {:?}", response.into_inner());
    
    // Test create VM
    println!("\nTesting VM creation...");
    let response = client.create_vm(CreateVmRequest {
        name: "example-vm".to_string(),
        config_path: "./example-microvm.nix".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    }).await?;
    let create_result = response.into_inner();
    println!("Create VM response: {:?}", create_result);
    
    // If VM was created successfully, start it
    if create_result.success {
        println!("\nStarting VM...");
        let response = client.start_vm(StartVmRequest {
            name: "example-vm".to_string(),
        }).await?;
        println!("Start VM response: {:?}", response.into_inner());
    }
    
    // Test list VMs
    println!("\nTesting VM listing...");
    let response = client.list_vms(ListVmsRequest {}).await?;
    println!("List VMs response: {:?}", response.into_inner());
    
    // Test cluster status
    println!("\nTesting cluster status...");
    let response = client.get_cluster_status(ClusterStatusRequest {}).await?;
    println!("Cluster status response: {:?}", response.into_inner());
    
    Ok(())
}