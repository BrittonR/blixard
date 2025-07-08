//! Complete VM management demonstration using blixard

use blixard_core::{
    types::{VmConfig, VmStatus},
    vm_backend::VmBackend,
};
use blixard_vm::{
    types::{Hypervisor, NetworkConfig},
    MicrovmBackend,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use redb::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 Blixard VM Management Demo");
    println!("=============================");

    // Create directories for the backend
    let config_dir = PathBuf::from("demo-vms");
    let data_dir = PathBuf::from("demo-data");

    // Create the backend
    let db_path = PathBuf::from("demo.db");
    let database = Arc::new(Database::create(&db_path)?);
    let backend = MicrovmBackend::new(config_dir, data_dir, database)?;

    // Define a VM configuration
    let vm_config = VmConfig {
        name: "demo-vm".to_string(),
        config_path: "".to_string(), // Will be set by backend
        vcpus: 1,
        memory: 512,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    };

    println!("\n📦 Creating VM '{}'...", vm_config.name);
    backend.create_vm(&vm_config, 1).await?;
    println!("✅ VM created successfully");

    // List VMs
    println!("\n📋 Current VMs:");
    let vms = backend.list_vms().await?;
    for (config, status) in &vms {
        println!(
            "  🖥️  {} ({}vcpu, {}MB): {:?}",
            config.name, config.vcpus, config.memory, status
        );
    }

    // Start the VM
    println!("\n🟢 Starting VM '{}'...", vm_config.name);
    match backend.start_vm(&vm_config.name).await {
        Ok(_) => {
            println!("✅ VM started successfully!");

            // Wait a moment for startup
            println!("⏳ Waiting for VM to initialize...");
            sleep(Duration::from_secs(3)).await;

            // Check status
            let status = backend.get_vm_status(&vm_config.name).await?;
            println!("📊 VM status: {:?}", status);

            // Show connection info
            if status == Some(VmStatus::Running) {
                println!("\n🌐 VM is running! You can:");
                // Basic connection info (without network details)
                println!(
                    "  • Console: socat - UNIX-CONNECT:/tmp/{}-console.sock",
                    vm_config.name
                );
            }

            // Wait before stopping
            println!("\n⏳ VM will run for 10 seconds...");
            sleep(Duration::from_secs(10)).await;

            // Stop the VM
            println!("\n🔴 Stopping VM '{}'...", vm_config.name);
            backend.stop_vm(&vm_config.name).await?;
            println!("✅ VM stopped successfully");
        }
        Err(e) => {
            println!("❌ Failed to start VM: {}", e);
            println!("💡 This might happen if:");
            println!("  • microvm command is not available");
            println!("  • Nix flake has syntax errors");
            println!("  • Insufficient permissions");
        }
    }

    // Final status check
    let final_status = backend.get_vm_status(&vm_config.name).await?;
    println!("\n📊 Final VM status: {:?}", final_status);

    // Clean up
    println!("\n🧹 Cleaning up...");
    backend.delete_vm(&vm_config.name).await?;
    println!("✅ VM deleted successfully");

    println!("\n✨ Demo completed!");

    Ok(())
}
