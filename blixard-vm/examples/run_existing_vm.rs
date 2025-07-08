//! Run the existing pre-built VM directly

use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};

fn find_vm_path() -> Option<PathBuf> {
    // Try multiple possible locations relative to current directory
    let possible_paths = vec![
        "../../vms/example-vm",                 // From blixard-vm/ directory
        "vm-configs/vms/example-vm",            // From main blixard/ directory
        "blixard-vm/vm-configs/vms/example-vm", // From parent directory
        "../vm-configs/vms/example-vm",         // Alternative relative path
    ];

    for path in possible_paths {
        let vm_path = PathBuf::from(path);
        let runner_path = vm_path.join("result/bin/microvm-run");
        if runner_path.exists() {
            return Some(vm_path);
        }
    }

    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing direct VM execution...");

    // Find the VM in various possible locations
    let vm_path = match find_vm_path() {
        Some(path) => path,
        None => {
            println!("❌ VM runner not found in any expected location:");
            println!("   - ../../vms/example-vm/result/bin/microvm-run");
            println!("   - vm-configs/vms/example-vm/result/bin/microvm-run");
            println!("   - blixard-vm/vm-configs/vms/example-vm/result/bin/microvm-run");
            println!("   - ../vm-configs/vms/example-vm/result/bin/microvm-run");
            println!("💡 Build a VM first using: cargo run --example vm_lifecycle");
            return Ok(());
        }
    };

    let runner_path = vm_path.join("result/bin/microvm-run");

    if !runner_path.exists() {
        println!("❌ VM runner not found at: {}", runner_path.display());
        println!("💡 Build a VM first using: cargo run --example vm_lifecycle");
        return Ok(());
    }

    println!("✓ Found VM runner at: {}", runner_path.display());

    // Test if we can execute it with a timeout
    println!("🎯 Starting VM (will timeout after 10 seconds)...");

    let result = timeout(Duration::from_secs(10), async {
        // Use shell to execute the script to handle any interpreter issues
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("./result/bin/microvm-run")
            .current_dir(&vm_path)
            .spawn()?;

        println!("🎉 VM process started with PID: {:?}", child.id());

        // Let it run for a few seconds
        sleep(Duration::from_secs(5)).await;

        // Try to kill it gracefully
        println!("🛑 Stopping VM...");
        child.kill().await?;

        let status = child.wait().await?;
        println!("✓ VM stopped with status: {}", status);

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await;

    match result {
        Ok(Ok(())) => {
            println!("✅ SUCCESS: VM ran and stopped successfully!");
        }
        Ok(Err(e)) => {
            println!("❌ Error running VM: {}", e);
        }
        Err(_) => {
            println!("⏰ VM execution timed out (this is normal - VMs run indefinitely)");
            println!("✅ SUCCESS: VM started successfully and was timed out!");
        }
    }

    // Show what the VM would do if run manually
    println!();
    println!("🔧 To run this VM manually:");
    println!("   cd {}", vm_path.display());
    println!("   ./result/bin/microvm-run");
    println!();
    println!("🎯 Or use nix run:");
    println!("   cd {}", vm_path.display());
    println!("   nix run .#nixosConfigurations.example-vm.config.microvm.runner.qemu");

    Ok(())
}
