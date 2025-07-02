//! Example demonstrating actually RUNNING a VM from Rust code

use blixard_vm::MicrovmBackend;
use blixard_core::{
    vm_backend::VmBackend,
    types::VmConfig,
};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    // Create directories for the backend
    let config_dir = PathBuf::from("vm-configs");
    let data_dir = PathBuf::from("vm-data");
    
    println!("🚀 Creating MicrovmBackend...");
    let backend = MicrovmBackend::new(config_dir, data_dir)?;
    
    // Define a VM configuration
    let vm_config = VmConfig {
        name: "run-example-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 512, // Smaller for faster startup
        tenant_id: "default".to_string(),
        ip_address: Some("10.0.0.100".to_string()),
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };
    
    println!("📦 Creating VM '{}'...", vm_config.name);
    backend.create_vm(&vm_config, 1).await?;
    println!("✓ VM configuration created");
    
    println!("🔨 Building and starting VM '{}'...", vm_config.name);
    println!("⏳ This will take a moment to build with Nix...");
    
    // This should actually BUILD and RUN the VM
    match backend.start_vm(&vm_config.name).await {
        Ok(()) => {
            println!("🎉 VM '{}' started successfully!", vm_config.name);
            
            // Let it run for a few seconds
            println!("⏱️ Letting VM run for 5 seconds...");
            sleep(Duration::from_secs(5)).await;
            
            // Check status
            match backend.get_vm_status(&vm_config.name).await? {
                Some(status) => println!("📊 VM status: {:?}", status),
                None => println!("❓ VM status: Unknown (might be starting)"),
            }
            
            // Stop the VM
            println!("🛑 Stopping VM...");
            match backend.stop_vm(&vm_config.name).await {
                Ok(()) => println!("✓ VM stopped successfully"),
                Err(e) => println!("⚠ Error stopping VM: {}", e),
            }
        }
        Err(e) => {
            println!("❌ Failed to start VM: {}", e);
            println!("💡 This might be because:");
            println!("   - Nix is not available");
            println!("   - VM build failed");
            println!("   - Insufficient permissions");
        }
    }
    
    // Clean up
    println!("🧹 Cleaning up...");
    backend.delete_vm(&vm_config.name).await?;
    println!("✓ VM deleted");
    
    Ok(())
}