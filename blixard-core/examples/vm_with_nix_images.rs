//! Example of using Nix VM images with Blixard VM management
//!
//! This demonstrates the complete workflow of building, distributing,
//! and running Nix-built microVMs with P2P image distribution.

use blixard_core::{
    error::BlixardResult,
    node::Node,
    transport::config::TransportConfig,
    types::{NodeConfig, VmCommand, VmConfig},
    vm_backend::VmBackendRegistry,
};
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== Nix-based VM with P2P Image Distribution ===");

    // Step 1: Build a Nix microVM image
    info!("\n1. Building Nix microVM image...");
    let (system_path, kernel_path) = build_nix_microvm().await?;
    info!("Built microVM at: {:?}", system_path);

    // Step 2: Start a Blixard node with Iroh transport
    info!("\n2. Starting Blixard node with P2P support...");
    let temp_dir = TempDir::new()?;
    let mut node = create_node_with_p2p(temp_dir.path()).await?;

    // Step 3: Import the Nix image to P2P store
    info!("\n3. Importing Nix image to P2P store...");
    let image_id = import_nix_image(&node, &system_path, Some(&kernel_path)).await?;
    info!("Imported image with ID: {}", image_id);

    // Step 4: Create a VM configuration using the Nix image
    info!("\n4. Creating VM from Nix image...");
    let vm_config = create_vm_config_from_nix(image_id.clone());

    // Submit VM creation command
    node.send_vm_command(VmCommand::Create { config: vm_config.clone(), node_id: 1 })
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Step 5: Start the VM
    info!("\n5. Starting VM...");
    node.send_vm_command(VmCommand::Start { name: vm_config.name.clone() })
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check VM status
    if let Ok(Some((config, status))) = node.get_vm_status(&vm_config.name).await {
        info!("VM Status: {:?}", status);
        if let Some(ip) = node.get_vm_ip(&vm_config.name).await? {
            info!("VM IP Address: {}", ip);
        }
    }

    // Step 6: Demonstrate P2P transfer to another node
    info!("\n6. Simulating P2P transfer to another node...");
    simulate_p2p_transfer(&node, &image_id).await?;

    // Step 7: Pre-fetch for migration
    info!("\n7. Pre-fetching image for migration...");
    prefetch_for_migration(&node, &vm_config.name).await?;

    // Cleanup
    info!("\n8. Cleaning up...");
    node.send_vm_command(VmCommand::Stop { name: vm_config.name.clone() })
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    node.send_vm_command(VmCommand::Delete { name: vm_config.name })
        .await?;

    node.stop().await?;
    info!("\n✅ Demo completed successfully!");

    Ok(())
}

/// Build a Nix microVM image (simulated)
async fn build_nix_microvm() -> BlixardResult<(PathBuf, PathBuf)> {
    // In a real scenario, this would run:
    // nix build .#nixosConfigurations.my-microvm.config.system.build.toplevel
    // nix build .#nixosConfigurations.my-microvm.config.boot.kernelPackages.kernel

    let temp_dir = TempDir::new()?;
    let system_path = temp_dir.path().join("system");
    let kernel_path = temp_dir.path().join("kernel");

    // Create dummy files for demo
    tokio::fs::write(&system_path, include_bytes!("../../README.md")).await?;
    tokio::fs::write(&kernel_path, b"dummy kernel image").await?;

    // Keep temp dir alive
    std::mem::forget(temp_dir);

    Ok((system_path, kernel_path))
}

/// Create a node with P2P support
async fn create_node_with_p2p(data_dir: &std::path::Path) -> BlixardResult<Node> {
    let config = NodeConfig {
        id: 1,
        data_dir: data_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "microvm".to_string(), // Use microvm backend
        transport_config: Some(TransportConfig::default()), // Iroh by default
        topology: Default::default(), // Use default topology
    };

    let mut node = Node::new(config);

    // Register VM backend (in production, microvm.nix backend would be registered)
    let registry = VmBackendRegistry::default();
    node.initialize_with_vm_registry(registry).await?;
    node.start().await?;

    Ok(node)
}

/// Import a Nix image to the P2P store
async fn import_nix_image(
    node: &Node,
    system_path: &PathBuf,
    kernel_path: Option<&PathBuf>,
) -> BlixardResult<String> {
    use blixard_core::transport::services::nix_vm_image::NixVmImageServiceImpl;

    // Get the shared node state
    let shared = node.shared();

    // Create image service
    let service = NixVmImageServiceImpl::new(shared).await?;

    // Import the microVM
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("build_host".to_string(), "local".to_string());
    metadata.insert("nixpkgs_rev".to_string(), "nixos-23.11".to_string());

    let response = service
        .import_microvm("demo-microvm", system_path, kernel_path.map(|p| p.as_path()), metadata)
        .await?;

    info!("Import stats:");
    info!("  Size: {} MB", response.total_size as f64 / 1_048_576.0);
    info!("  Chunks: {}", response.chunk_count);
    info!(
        "  Deduplication: {:.1}%",
        response.deduplication_ratio * 100.0
    );

    Ok(response.image_id)
}

/// Create VM configuration using Nix image
fn create_vm_config_from_nix(image_id: String) -> VmConfig {
    VmConfig {
        name: "nix-demo-vm".to_string(),
        config_path: "/tmp/nix-demo.nix".to_string(),
        vcpus: 2,
        memory: 1024,
        ip_address: Some("10.0.0.100".to_string()),
        tenant_id: "demo-tenant".to_string(),
        // Store image ID in metadata
        metadata: Some(std::collections::HashMap::from([
            ("nix_image_id".to_string(), image_id),
            ("hypervisor".to_string(), "cloud-hypervisor".to_string()),
        ])),
        anti_affinity: None,
        priority: 500,
        preemptible: false,
        locality_preference: Default::default(),
        health_check_config: None,
    }
}

/// Simulate P2P transfer to another node
async fn simulate_p2p_transfer(node: &Node, image_id: &str) -> BlixardResult<()> {
    use blixard_core::transport::services::nix_vm_image::NixVmImageServiceImpl;

    // In a real scenario, this would:
    // 1. Connect to target node via Iroh
    // 2. Check which chunks the target already has
    // 3. Transfer only missing chunks
    // 4. Reassemble image on target

    let shared = node.shared();
    let service = NixVmImageServiceImpl::new(shared).await?;

    // Simulate download
    match service
        .download_image(image_id, Some(&PathBuf::from("/tmp/vm-images")))
        .await
    {
        Ok((path, stats)) => {
            info!("Transfer completed:");
            info!("  Local path: {:?}", path);
            info!(
                "  Transferred: {} MB",
                stats.bytes_transferred as f64 / 1_048_576.0
            );
            info!(
                "  Deduplicated: {} MB ({:.1}% saved)",
                stats.bytes_deduplicated as f64 / 1_048_576.0,
                (stats.bytes_deduplicated as f64
                    / (stats.bytes_transferred + stats.bytes_deduplicated) as f64)
                    * 100.0
            );
            info!("  Duration: {:?}", stats.duration);
        }
        Err(e) => {
            info!("Transfer simulation failed (expected): {}", e);
        }
    }

    Ok(())
}

/// Pre-fetch image for upcoming migration
async fn prefetch_for_migration(node: &Node, vm_name: &str) -> BlixardResult<()> {
    use blixard_core::transport::services::nix_vm_image::NixVmImageServiceImpl;

    let shared = node.shared();
    let service = NixVmImageServiceImpl::new(shared).await?;

    // Target node 2 for migration
    service.prefetch_for_migration(vm_name, 2).await?;
    info!("Pre-fetch initiated for migration of {} to node 2", vm_name);

    Ok(())
}

/// Example Nix expression for building microVMs
fn example_nix_expression() -> &'static str {
    r#"
# Example: Building a microVM with Nix
{ pkgs, lib, microvm, ... }:

{
  # Define the microVM
  microvm = {
    vcpu = 2;
    mem = 1024;
    hypervisor = "cloud-hypervisor";
    
    # Use virtiofs for efficient file sharing
    shares = [{
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
      tag = "ro-store";
      proto = "virtiofs";
    }];
    
    # Network configuration
    interfaces = [{
      type = "tap";
      id = "vm-eth0";
      mac = "02:00:00:00:00:01";
    }];
  };
  
  # System configuration
  boot.loader.grub.enable = false;
  boot.initrd.enable = false;
  boot.kernelParams = [ "console=ttyS0" ];
  
  # Minimal filesystem
  fileSystems."/" = {
    device = "rootfs";
    fsType = "tmpfs";
    options = [ "size=1G" "mode=755" ];
  };
  
  # Your application
  systemd.services.my-app = {
    description = "My Application";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "${pkgs.my-app}/bin/my-app";
      Restart = "always";
    };
  };
  
  # Content addressing for efficient distribution
  system.extraSystemBuilderCmds = ''
    # Create content-addressed store paths
    echo "Optimizing for P2P distribution..."
  '';
}
"#
}

/// Example of using Nix flakes for reproducible builds
fn example_flake() -> &'static str {
    r#"
{
  description = "Blixard microVM images";
  
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };
  
  outputs = { self, nixpkgs, microvm }: 
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      # Define multiple microVM variants
      nixosConfigurations = {
        # Basic web server
        web-server = nixpkgs.lib.nixosSystem {
          inherit system;
          modules = [
            microvm.nixosModules.microvm
            ./vms/web-server.nix
          ];
        };
        
        # Database server with persistent storage
        database = nixpkgs.lib.nixosSystem {
          inherit system;
          modules = [
            microvm.nixosModules.microvm
            ./vms/database.nix
          ];
        };
        
        # Application container
        app-container = nixpkgs.lib.nixosSystem {
          inherit system;
          modules = [
            microvm.nixosModules.microvm
            ./vms/app-container.nix
          ];
        };
      };
      
      # Hydra jobs for CI/CD
      hydraJobs = {
        inherit (self) nixosConfigurations;
      };
    };
}
"#
}
