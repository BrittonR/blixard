//! Example to create and start a MicroVM using gRPC

use blixard_core::iroh_types::{
    cluster_service_client::ClusterServiceClient,
    CreateVmRequest, StartVmRequest, ListVmsRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vm_name = std::env::args().nth(1).unwrap_or_else(|| "test-microvm".to_string());
    
    println!("🚀 Blixard MicroVM Creation Example");
    println!("====================================");
    println!("Connecting to Blixard node at 127.0.0.1:7001...");
    
    // Connect to the server
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:7001").await?;
    println!("✓ Connected successfully");
    
    // Create VM
    println!("\n📦 Creating VM '{}'...", vm_name);
    let response = client.create_vm(CreateVmRequest {
        name: vm_name.clone(),
        config_path: "./example-microvm.nix".to_string(),
        vcpus: 1,
        memory_mb: 512,
    }).await?;
    
    let result = response.into_inner();
    if result.success {
        println!("✓ VM created successfully: {}", result.message);
    } else {
        println!("✗ Failed to create VM: {}", result.message);
        return Ok(());
    }
    
    // Start the VM
    println!("\n▶️  Starting VM '{}'...", vm_name);
    let response = client.start_vm(StartVmRequest {
        name: vm_name.clone(),
    }).await?;
    
    let result = response.into_inner();
    if result.success {
        println!("✓ VM started successfully: {}", result.message);
    } else {
        println!("✗ Failed to start VM: {}", result.message);
    }
    
    // List all VMs
    println!("\n📋 Current VMs:");
    let response = client.list_vms(ListVmsRequest {}).await?;
    let vms = response.into_inner().vms;
    
    if vms.is_empty() {
        println!("  No VMs found");
    } else {
        for vm in vms {
            println!("  - {} (vcpus={}, memory={}MB, state={})", 
                vm.name, vm.vcpus, vm.memory_mb, vm.state);
        }
    }
    
    println!("\n✅ Done! The MicroVM backend will now:");
    println!("   1. Generate a Nix flake from your configuration");
    println!("   2. Build the MicroVM using Nix");
    println!("   3. Start the VM with QEMU");
    println!("\nCheck the logs in your terminal running the node for details.");
    println!("VM data will be in: ./data/node1/vm-data/{}/", vm_name);
    
    Ok(())
}