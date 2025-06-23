//! Example demonstrating VM lifecycle management with microvm.nix integration

use blixard_vm::MicrovmBackend;
use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig, VmStatus},
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    // Create directories for the backend
    let config_dir = PathBuf::from("vm-configs");
    let data_dir = PathBuf::from("vm-data");
    
    println!("Creating MicrovmBackend with:");
    println!("  Config directory: {}", config_dir.display());
    println!("  Data directory: {}", data_dir.display());
    
    // Create the backend
    let backend = MicrovmBackend::new(config_dir, data_dir)?;
    
    // Define a VM configuration
    let vm_config = VmConfig {
        name: "example-vm".to_string(),
        config_path: "".to_string(), // Will be set by backend
        vcpus: 2,
        memory: 1024,
    };
    
    println!("\nCreating VM '{}'...", vm_config.name);
    backend.create_vm(&vm_config, 1).await?;
    println!("✓ VM created successfully");
    
    // List VMs
    println!("\nListing VMs:");
    let vms = backend.list_vms().await?;
    for (config, status) in &vms {
        println!("  - {} ({}vcpu, {}MB): {:?}", 
            config.name, config.vcpus, config.memory, status);
    }
    
    // Get VM status
    let status = backend.get_vm_status(&vm_config.name).await?;
    println!("\nVM '{}' status: {:?}", vm_config.name, status);
    
    // Note: Starting the VM would require a valid Nix environment
    // and microvm.nix to be properly set up
    println!("\nTo start the VM, you would run:");
    println!("  backend.start_vm(\"{}\").await?;", vm_config.name);
    println!("\nThis would:");
    println!("  1. Build the VM using: nix build <flake>#nixosConfigurations.{}.config.microvm.runner", vm_config.name);
    println!("  2. Run the resulting VM runner executable");
    
    // Clean up
    println!("\nDeleting VM '{}'...", vm_config.name);
    backend.delete_vm(&vm_config.name).await?;
    println!("✓ VM deleted successfully");
    
    // Verify deletion
    let vms = backend.list_vms().await?;
    println!("\nVMs after deletion: {}", vms.len());
    
    Ok(())
}