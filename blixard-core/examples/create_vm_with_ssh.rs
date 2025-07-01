//! Example to create a MicroVM with SSH access

use blixard_core::iroh_types::{
    cluster_service_client::ClusterServiceClient,
    CreateVmRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vm_name = std::env::args().nth(1).unwrap_or_else(|| "test-ssh-vm".to_string());
    
    println!("🚀 Creating MicroVM with SSH access");
    println!("===================================");
    
    // Connect to the server
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:7001").await?;
    
    // Create VM with SSH configuration
    println!("\n📦 Creating VM '{}'...", vm_name);
    let response = client.create_vm(CreateVmRequest {
        name: vm_name.clone(),
        config_path: "./ssh-enabled-vm.nix".to_string(),
        vcpus: 1,
        memory_mb: 512,
    }).await?;
    
    let result = response.into_inner();
    if !result.success {
        println!("✗ Failed to create VM: {}", result.message);
        return Ok(());
    }
    
    println!("✓ VM created successfully");
    
    // The VM would need a custom flake with:
    // 1. Bridge networking instead of user networking
    // 2. SSH service enabled
    // 3. Known root password or SSH key
    
    println!("\nTo access the VM serial console:");
    println!("1. Find the terminal where blixard node is running (pts/8)");
    println!("2. You should see the VM boot messages there");
    println!("3. Press Enter to get a login prompt");
    println!("4. Login as 'root' (no password by default)");
    
    Ok(())
}