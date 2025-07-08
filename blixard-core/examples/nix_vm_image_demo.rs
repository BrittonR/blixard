//! Demo of Nix VM image P2P distribution
//!
//! This example demonstrates how to import and distribute Nix-built
//! microVM images using content-addressed storage and chunked transfers.

use blixard_core::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    transport::{
        config::{IrohConfig, TransportConfig},
        iroh_service::{IrohRpcClient, IrohRpcServer},
        services::nix_vm_image::{NixVmImageClient, NixVmImageServiceImpl},
    },
    types::NodeConfig,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tracing::{error, info};

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
    server1.register_service(service1.clone()).await;
    server2.register_service(service2.clone()).await;

    // Start servers
    let server1_handle = tokio::spawn(async move { server1.serve().await });
    let server2_handle = tokio::spawn(async move { server2.serve().await });

    // Give servers time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Demo 1: Import a microVM on node 1
    info!("\n--- Demo 1: Import MicroVM ---");
    demo_import_microvm(&service1).await?;

    // Demo 2: Import a container on node 1
    info!("\n--- Demo 2: Import Container ---");
    demo_import_container(&service1).await?;

    // Demo 3: Import a Nix closure
    info!("\n--- Demo 3: Import Nix Closure ---");
    demo_import_closure(&service1).await?;

    // Demo 4: P2P transfer from node 1 to node 2
    info!("\n--- Demo 4: P2P Image Transfer ---");
    demo_p2p_transfer(&endpoint1, &endpoint2).await?;

    // Demo 5: Pre-fetch for migration
    info!("\n--- Demo 5: Pre-fetch for Migration ---");
    demo_prefetch_migration(&service1).await?;

    // Demo 6: Garbage collection
    info!("\n--- Demo 6: Garbage Collection ---");
    demo_garbage_collection(&service1).await?;

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
        transport_config: Some(TransportConfig::Iroh(IrohConfig::default())),
    };

    let node = Arc::new(SharedNodeState::new(config));

    // Initialize P2P manager
    let p2p_config = blixard_core::p2p_manager::P2pConfig::default();
    let p2p_manager = Arc::new(
        blixard_core::p2p_manager::P2pManager::new(id, temp_dir.path(), p2p_config).await?,
    );
    node.set_p2p_manager(p2p_manager).await;

    // Create RPC server
    let server = Arc::new(IrohRpcServer::new(endpoint.clone()));

    // Keep temp dir alive
    std::mem::forget(temp_dir);

    Ok((node, server, endpoint))
}

async fn demo_import_microvm(service: &NixVmImageServiceImpl) -> BlixardResult<()> {
    // Create a dummy microVM system path
    let temp_dir = TempDir::new()?;
    let system_path = temp_dir.path().join("nixos-system");
    let kernel_path = temp_dir.path().join("kernel");

    // Create dummy files
    tokio::fs::write(&system_path, b"dummy nixos system").await?;
    tokio::fs::write(&kernel_path, b"dummy kernel").await?;

    let mut metadata = HashMap::new();
    metadata.insert("arch".to_string(), "x86_64".to_string());
    metadata.insert("nixos_version".to_string(), "23.11".to_string());

    let response = service
        .import_microvm("test-microvm", &system_path, Some(&kernel_path), metadata)
        .await?;

    info!("Imported microVM:");
    info!("  ID: {}", response.image_id);
    info!("  Hash: {}", response.content_hash);
    info!("  Size: {} MB", response.total_size as f64 / 1_048_576.0);
    info!("  Chunks: {}", response.chunk_count);
    info!(
        "  Deduplication: {:.1}%",
        response.deduplication_ratio * 100.0
    );

    Ok(())
}

async fn demo_import_container(service: &NixVmImageServiceImpl) -> BlixardResult<()> {
    // Create a dummy container tar
    let temp_dir = TempDir::new()?;
    let tar_path = temp_dir.path().join("container.tar");

    // Create dummy tar file
    tokio::fs::write(&tar_path, b"dummy container tar").await?;

    let mut metadata = HashMap::new();
    metadata.insert("registry".to_string(), "docker.io".to_string());
    metadata.insert("tag".to_string(), "latest".to_string());

    let response = service
        .import_container("test-app:latest", &tar_path, metadata)
        .await?;

    info!("Imported container:");
    info!("  ID: {}", response.image_id);
    info!("  Hash: {}", response.content_hash);
    info!("  Size: {} MB", response.total_size as f64 / 1_048_576.0);
    info!("  Chunks: {}", response.chunk_count);
    info!(
        "  Deduplication: {:.1}%",
        response.deduplication_ratio * 100.0
    );

    Ok(())
}

async fn demo_import_closure(service: &NixVmImageServiceImpl) -> BlixardResult<()> {
    // Create a dummy Nix store path structure
    let temp_dir = TempDir::new()?;
    let store_dir = temp_dir.path().join("nix").join("store");
    std::fs::create_dir_all(&store_dir)?;

    let root_path = store_dir.join("abc123-test-closure");
    std::fs::create_dir(&root_path)?;
    tokio::fs::write(root_path.join("bin"), b"dummy binary").await?;

    let mut metadata = HashMap::new();
    metadata.insert(
        "description".to_string(),
        "Test closure with dependencies".to_string(),
    );

    let response = service
        .import_closure("test-closure", &root_path, metadata)
        .await?;

    info!("Imported Nix closure:");
    info!("  ID: {}", response.image_id);
    info!("  Hash: {}", response.content_hash);
    info!("  Size: {} MB", response.total_size as f64 / 1_048_576.0);
    info!("  Chunks: {}", response.chunk_count);
    info!(
        "  Deduplication: {:.1}%",
        response.deduplication_ratio * 100.0
    );

    Ok(())
}

async fn demo_p2p_transfer(
    endpoint1: &iroh::Endpoint,
    endpoint2: &iroh::Endpoint,
) -> BlixardResult<()> {
    // Get node addresses
    let node1_addr = endpoint1.node_addr().get().unwrap().unwrap();
    let node2_addr = endpoint2.node_addr().get().unwrap().unwrap();

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

async fn demo_prefetch_migration(service: &NixVmImageServiceImpl) -> BlixardResult<()> {
    info!("Pre-fetching images for VM migration...");

    // Simulate pre-fetching for a VM migration
    match service.prefetch_for_migration("my-important-vm", 3).await {
        Ok(_) => info!("Pre-fetch initiated for VM 'my-important-vm' to node 3"),
        Err(e) => info!("Pre-fetch failed (expected in demo): {}", e),
    }

    Ok(())
}

async fn demo_garbage_collection(service: &NixVmImageServiceImpl) -> BlixardResult<()> {
    info!("Running garbage collection...");

    // Run GC keeping images from last 7 days and minimum 2 copies
    match service.garbage_collect(7, 2).await {
        Ok(bytes_freed) => {
            info!("Garbage collection completed:");
            info!("  Freed: {} MB", bytes_freed as f64 / 1_048_576.0);
        }
        Err(e) => info!("GC failed (expected in demo): {}", e),
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
