//! Example gRPC client for testing Blixard server

use blixard_core::proto::{
    cluster_service_client::ClusterServiceClient,
    HealthCheckRequest, CreateVmRequest, ListVmsRequest, ClusterStatusRequest,
    StartVmRequest, DeleteVmRequest,
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
    
    // Test delete VM if test-delete-vm exists
    println!("\nTesting VM deletion...");
    let delete_response = client.delete_vm(DeleteVmRequest {
        name: "test-delete-vm".to_string(),
    }).await?;
    let delete_result = delete_response.into_inner();
    println!("Delete VM response: {:?}", delete_result);
    
    // Wait a moment for the delete to be processed
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // List VMs again to see if it was deleted
    println!("\nListing VMs after delete:");
    let response = client.list_vms(ListVmsRequest {}).await?;
    let vms_after = response.into_inner();
    println!("VMs after delete: {:?}", vms_after);
    
    Ok(())
}