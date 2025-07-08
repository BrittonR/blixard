//! Example demonstrating VM lifecycle management with flake-parts templates
//!
//! This example shows how to use the flake-parts template for better modularity
//! and composition when defining VMs.

use blixard_vm::{
    nix_generator::NixFlakeGenerator,
    process_manager::VmProcessManager,
    types::{Hypervisor, NixModule, VmConfig},
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Blixard VM Lifecycle Example with Flake-Parts ===\n");

    // Create temporary directories
    let temp_dir = TempDir::new()?;
    let runtime_dir = temp_dir.path().join("runtime");
    let flakes_dir = temp_dir.path().join("generated-flakes");
    let modules_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("nix/modules");

    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::create_dir_all(&flakes_dir)?;

    println!("Runtime directory: {}", runtime_dir.display());
    println!("Flakes directory: {}", flakes_dir.display());
    println!("Modules directory: {}\n", modules_dir.display());

    // Initialize components
    let generator = NixFlakeGenerator::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("nix/templates"),
        modules_dir,
    )?;

    let process_manager = Arc::new(VmProcessManager::new(runtime_dir.clone()));

    // Create VM configurations demonstrating flake-parts features
    let configs = vec![
        // Simple web server using pre-built module
        VmConfig {
            name: "web-simple".to_string(),
            vm_index: 1,
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 2,
            memory: 1024,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            flake_modules: vec!["webserver".to_string()],
            kernel: None,
            init_command: Some(format!(
                "{}/bin/echo 'Simple web server started!'",
                "/run/current-system/sw"
            )),
        },
        // Database server with monitoring
        VmConfig {
            name: "db-monitored".to_string(),
            vm_index: 2,
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 4,
            memory: 2048,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![NixModule::Inline(
                r#"{
                    services.postgresql.settings = {
                        max_connections = 200;
                        shared_buffers = "512MB";
                    };
                }"#
                .to_string(),
            )],
            flake_modules: vec!["database".to_string(), "monitoring".to_string()],
            kernel: None,
            init_command: None,
        },
        // Full stack development environment
        VmConfig {
            name: "dev-stack".to_string(),
            vm_index: 3,
            hypervisor: Hypervisor::Qemu,
            vcpus: 8,
            memory: 4096,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![NixModule::Inline(
                r#"{
                    environment.systemPackages = with pkgs; [
                        git vim tmux htop
                    ];
                    
                    services.nginx.virtualHosts."dev.local" = {
                        locations."/" = {
                            proxyPass = "http://localhost:3000";
                        };
                    };
                }"#
                .to_string(),
            )],
            flake_modules: vec![
                "webserver".to_string(),
                "database".to_string(),
                "containerRuntime".to_string(),
                "monitoring".to_string(),
            ],
            kernel: None,
            init_command: None,
        },
    ];

    // Generate and display flakes for each configuration
    println!("=== Generating Flakes ===\n");

    for config in &configs {
        let vm_dir = flakes_dir.join(&config.name);
        std::fs::create_dir_all(&vm_dir)?;

        println!("Generating flake-parts flake for VM: {}", config.name);
        println!("  Modules: {:?}", config.flake_modules);

        // Generate using flake-parts template
        let flake_path = generator.write_flake_parts(config, &vm_dir)?;
        println!("  Generated at: {}", flake_path.display());

        // Show a snippet of the generated flake
        let content = std::fs::read_to_string(&flake_path)?;
        let lines: Vec<&str> = content.lines().take(20).collect();
        println!("  First 20 lines:");
        for line in lines {
            println!("    {}", line);
        }
        println!("  ...\n");
    }

    // Demonstrate the difference between standard and flake-parts templates
    println!("=== Template Comparison ===\n");

    let comparison_config = &configs[0];
    let standard_dir = flakes_dir.join("standard-template");
    let flake_parts_dir = flakes_dir.join("flake-parts-template");

    std::fs::create_dir_all(&standard_dir)?;
    std::fs::create_dir_all(&flake_parts_dir)?;

    println!(
        "Generating both template types for: {}",
        comparison_config.name
    );

    // Standard template
    let standard_path = generator.write_flake(comparison_config, &standard_dir)?;
    let standard_size = std::fs::metadata(&standard_path)?.len();
    println!("  Standard template size: {} bytes", standard_size);

    // Flake-parts template
    let flake_parts_path = generator.write_flake_parts(comparison_config, &flake_parts_dir)?;
    let flake_parts_size = std::fs::metadata(&flake_parts_path)?.len();
    println!("  Flake-parts template size: {} bytes", flake_parts_size);

    println!("\nKey differences:");
    println!("  - Flake-parts provides better module composition");
    println!("  - Standard template is simpler and more direct");
    println!("  - Flake-parts enables reusable VM profiles");
    println!("  - Both generate working microvm.nix configurations");

    // Note: Actual VM execution would require:
    // 1. Working Nix installation
    // 2. microvm.nix in NIX_PATH
    // 3. Proper permissions for VM execution
    // 4. Network setup (TAP interfaces)

    println!("\n=== Example Complete ===");
    println!("\nTo use these VMs in production:");
    println!("1. Ensure Nix is installed with flakes enabled");
    println!("2. Set up TAP network interfaces");
    println!("3. Build and run with: nix run <flake-dir>");
    println!("\nThe flake-parts approach is recommended for:");
    println!("- Multi-VM deployments");
    println!("- Shared configuration across VMs");
    println!("- Complex service compositions");
    println!("- Reusable VM profiles");

    Ok(())
}
