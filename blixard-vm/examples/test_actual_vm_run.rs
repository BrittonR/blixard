//! Test actually running the pre-built VM

use blixard_vm::process_manager::VmProcessManager;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing actual VM execution...");
    
    // Create process manager
    let runtime_dir = PathBuf::from("vm-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    
    let process_manager = VmProcessManager::new(runtime_dir);
    
    // Use the pre-built VM
    let vm_name = "example-vm";
    let flake_path = PathBuf::from("../../vms/example-vm");
    
    if !flake_path.exists() {
        println!("❌ Pre-built VM not found at: {}", flake_path.display());
        println!("💡 Run the vm_lifecycle example first to build a VM");
        return Ok(());
    }
    
    println!("📂 Using VM at: {}", flake_path.display());
    
    // Check if result exists (meaning it's already built)
    let result_path = flake_path.join("result");
    if !result_path.exists() {
        println!("❌ VM not built yet - missing 'result' symlink");
        return Ok(());
    }
    
    println!("✓ Found built VM at: {}", result_path.display());
    
    println!("🎯 Starting VM '{}' (timeout in 10 seconds)...", vm_name);
    
    // Start the VM with a timeout
    match tokio::time::timeout(
        Duration::from_secs(10), 
        process_manager.start_vm(vm_name, &flake_path)
    ).await {
        Ok(Ok(())) => {
            println!("🎉 VM started successfully!");
            
            // Let it run briefly
            println!("⏱️ Letting VM run for 3 seconds...");
            sleep(Duration::from_secs(3)).await;
            
            // Check if it's still running
            match process_manager.get_vm_status(vm_name).await {
                Ok(Some(status)) => println!("📊 VM status: {:?}", status),
                Ok(None) => println!("❓ VM status: Not found"),
                Err(e) => println!("⚠ Error getting status: {}", e),
            }
            
            // Stop it
            println!("🛑 Stopping VM...");
            match process_manager.stop_vm(vm_name).await {
                Ok(()) => println!("✓ VM stopped successfully"),
                Err(e) => println!("⚠ Error stopping VM: {}", e),
            }
        }
        Ok(Err(e)) => {
            println!("❌ Failed to start VM: {}", e);
        }
        Err(_) => {
            println!("⏰ VM start timed out (normal - VMs take time to build/start)");
        }
    }
    
    Ok(())
}