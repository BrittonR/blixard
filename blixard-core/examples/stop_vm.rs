//! Example to stop a VM

use blixard_core::proto::{
    cluster_service_client::ClusterServiceClient,
    StopVmRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vm_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: stop_vm <vm-name>");
        std::process::exit(1);
    });
    
    println!("Stopping VM '{}'...", vm_name);
    
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:7001").await?;
    
    let response = client.stop_vm(StopVmRequest {
        name: vm_name.clone(),
    }).await?;
    
    let result = response.into_inner();
    if result.success {
        println!("✓ VM stopped successfully: {}", result.message);
    } else {
        println!("✗ Failed to stop VM: {}", result.message);
    }
    
    Ok(())
}