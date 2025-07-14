//! Demo of Nix VM image P2P distribution
//!
//! This example demonstrates how to import and distribute Nix-built
//! microVM images using content-addressed storage and chunked transfers.

use blixard_core::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    transport::{
        config::IrohConfig,
        iroh_service::{IrohRpcClient, IrohRpcServer},
        services::nix_vm_image::{NixVmImageClient, NixVmImageServiceImpl},
    },
    types::NodeConfig,
};
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== Nix VM Image P2P Demo ===");

    // Create two nodes to demonstrate P2P transfer
    let (node1, server1, endpoint1) = create_node(1, 7101).await?;
    let (node2, server2, endpoint2) = create_node(2, 7102).await?;

    // Create image services
    let service1 = NixVmImageServiceImpl::new(node1.clone()).await?;
    let service2 = NixVmImageServiceImpl::new(node2.clone()).await?;

    // Register services
    server1.register_service(service1).await;
    server2.register_service(service2).await;

    // Start servers
    let server1_handle = tokio::spawn(async move { server1.serve().await });
    let server2_handle = tokio::spawn(async move { server2.serve().await });

    // Give servers time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Demo 1: P2P transfer from node 1 to node 2
    info!("\n--- Demo 1: P2P Image Transfer ---");
    demo_p2p_transfer(&endpoint1, &endpoint2).await?;

    // Demo 2: Show API usage examples
    info!("\n--- Demo 2: API Examples ---");
    demo_api_examples().await?;

    info!("\n✅ All demos completed successfully!");

    // Cleanup
    server1_handle.abort();
    server2_handle.abort();

    Ok(())
}

async fn create_node(
    id: u64,
    port: u16,
) -> BlixardResult<(Arc<SharedNodeState>, Arc<IrohRpcServer>, iroh::Endpoint)> {
    let temp_dir = TempDir::new()?;

    // Create Iroh endpoint
    let endpoint = iroh::Endpoint::builder().bind().await.map_err(|e| {
        blixard_core::error::BlixardError::Internal {
            message: e.to_string(),
        }
    })?;

    // Create node config
    let config = NodeConfig {
        id,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: Some(IrohConfig::default()),
        topology: blixard_core::types::NodeTopology::default(),
    };

    let node = Arc::new(SharedNodeState::new(config));

    // Initialize P2P manager (for now just set the endpoint)
    node.set_iroh_endpoint(Some(endpoint.clone())).await;

    // Create RPC server
    let server = Arc::new(IrohRpcServer::new(endpoint.clone()));

    // Keep temp dir alive
    std::mem::forget(temp_dir);

    Ok((node, server, endpoint))
}

async fn demo_api_examples() -> BlixardResult<()> {
    info!("Demonstrating Nix VM Image service API:");
    
    info!("1. Import MicroVM:");
    info!("   service.import_microvm(name, system_path, kernel_path, metadata)");
    info!("   -> Returns ImportNixImageResponse with content hash and deduplication ratio");
    
    info!("2. Import Container:");
    info!("   service.import_container(image_ref, tar_path, metadata)");
    info!("   -> Handles Docker/OCI container images with efficient chunking");
    
    info!("3. Import Nix Closure:");
    info!("   service.import_closure(name, root_path, metadata)");
    info!("   -> Imports Nix store paths with dependency tracking");
    
    info!("4. Download Image:");
    info!("   service.download_image(image_id, target_dir)");
    info!("   -> P2P download with automatic deduplication");
    
    info!("5. Pre-fetch for Migration:");
    info!("   service.prefetch_for_migration(vm_name, target_node)");
    info!("   -> Prepares images for VM migration");
    
    info!("6. Garbage Collection:");
    info!("   service.garbage_collect(keep_recent_days, keep_min_copies)");
    info!("   -> Cleans up unused images while preserving recent/important ones");
    
    Ok(())
}

async fn demo_p2p_transfer(
    endpoint1: &iroh::Endpoint,
    endpoint2: &iroh::Endpoint,
) -> BlixardResult<()> {
    // Get node addresses
    let node1_addr = iroh::NodeAddr::new(endpoint1.node_id());
    let node2_addr = iroh::NodeAddr::new(endpoint2.node_id());

    info!("Node 1 address: {}", node1_addr.node_id);
    info!("Node 2 address: {}", node2_addr.node_id);

    // Create client on node 2 to download from node 1
    let client = IrohRpcClient::new(endpoint2.clone());
    let image_client = NixVmImageClient::new(&client, node1_addr);

    // Simulate downloading an image
    info!("Simulating P2P transfer from node 1 to node 2...");

    // In a real scenario, we would download an actual image
    // For demo, we'll just show the API
    match image_client
        .download_image(
            "test-image-id".to_string(),
            Some("/tmp/downloads".to_string()),
        )
        .await
    {
        Ok((path, transferred, deduped, duration_ms)) => {
            info!("Download completed:");
            info!("  Path: {}", path);
            info!("  Transferred: {} MB", transferred as f64 / 1_048_576.0);
            info!("  Deduplicated: {} MB", deduped as f64 / 1_048_576.0);
            info!("  Duration: {} ms", duration_ms);
            info!(
                "  Speed: {:.2} MB/s",
                (transferred as f64 / 1_048_576.0) / (duration_ms as f64 / 1000.0)
            );
        }
        Err(e) => {
            // Expected in demo since we don't have a real image
            info!("Download failed (expected in demo): {}", e);
        }
    }

    Ok(())
}


// Example output structure for documentation
fn example_nix_flake() -> &'static str {
    r#"
# Example Nix flake for building a microVM image
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, microvm }: {
    nixosConfigurations.my-microvm = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        microvm.nixosModules.microvm
        {
          microvm = {
            vcpu = 2;
            mem = 1024;
            hypervisor = "cloud-hypervisor";
            shares = [{
              source = "/nix/store";
              mountPoint = "/nix/.ro-store";
              tag = "ro-store";
              proto = "virtiofs";
            }];
          };
          
          # Minimal NixOS configuration
          boot.loader.grub.enable = false;
          fileSystems."/".device = "rootfs";
          
          # Your application services here
          systemd.services.my-app = {
            wantedBy = [ "multi-user.target" ];
            script = "exec my-application";
          };
        }
      ];
    };
  };
}
"#
}
