//! Example of generating a VM flake using the NixFlakeGenerator

use blixard_vm::{NixFlakeGenerator, types::*};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a generator with paths to templates and modules
    let generator = NixFlakeGenerator::new(
        PathBuf::from("nix/templates"),
        PathBuf::from("nix/modules"),
    )?;
    
    // Define a simple VM configuration
    let config = VmConfig {
        name: "example-vm".to_string(),
        hypervisor: Hypervisor::CloudHypervisor,
        vcpus: 2,
        memory: 1024,
        networks: vec![
            NetworkConfig::Tap {
                name: "eth0".to_string(),
                bridge: Some("br0".to_string()),
                mac: None,
            }
        ],
        volumes: vec![
            VolumeConfig::RootDisk { size: 10240 },
        ],
        nixos_modules: vec![],
        flake_modules: vec![],
        kernel: None,
        init_command: Some("echo 'VM started successfully!'".to_string()),
    };
    
    // Generate the flake content
    let flake_content = generator.generate_vm_flake(&config)?;
    
    println!("Generated VM flake for '{}':", config.name);
    println!("=====================================");
    println!("{}", flake_content);
    println!("=====================================");
    
    // Example of writing to a file
    let output_dir = PathBuf::from("generated-flakes/example-vm");
    let flake_path = generator.write_flake(&config, &output_dir)?;
    println!("\nFlake written to: {}", flake_path.display());
    
    Ok(())
}