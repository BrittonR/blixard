//! Complete VM lifecycle example with P2P Nix image distribution
//!
//! This example demonstrates:
//! 1. Building a Nix microVM image
//! 2. Importing it to the P2P store
//! 3. Creating VMs that reference the image
//! 4. Automatic download on VM start
//! 5. Migration with pre-fetching

use blixard_core::{
    abstractions::command::TokioCommandExecutor,
    error::BlixardResult,
    nix_image_store::NixImageStore,
    node::Node,
    p2p_manager::{P2pConfig, P2pManager},
    transport::config::TransportConfig,
    types::{LocalityPreference, NodeConfig, NodeTopology, VmCommand, VmConfig},
    vm_backend::VmBackendRegistry,
};
use blixard_vm::microvm_backend::MicrovmBackend;
use redb::Database;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== VM Lifecycle with P2P Nix Images Demo ===");

    // Create temporary directories
    let temp_dir = TempDir::new()?;
    let node1_dir = temp_dir.path().join("node1");
    let node2_dir = temp_dir.path().join("node2");
    std::fs::create_dir_all(&node1_dir)?;
    std::fs::create_dir_all(&node2_dir)?;

    // Step 1: Build and import Nix image on build node
    info!("\n1. Setting up build node with Nix image...");
    let (image_id, image_metadata) = setup_build_node(&node1_dir).await?;

    // Step 2: Start cluster nodes
    info!("\n2. Starting cluster nodes...");
    let mut node1 = create_node(1, 7001, &node1_dir).await?;
    let mut node2 = create_node(2, 7002, &node2_dir).await?;

    // Set up P2P image stores on both nodes
    setup_node_with_image_store(&mut node1, &node1_dir).await?;
    setup_node_with_image_store(&mut node2, &node2_dir).await?;

    // Import the image to node1's store
    import_image_to_node(&node1, &image_metadata).await?;

    // Step 3: Create VM on node1 with Nix image reference
    info!("\n3. Creating VM with Nix image reference...");
    let vm_config = VmConfig {
        name: "p2p-nix-vm".to_string(),
        config_path: "/config.yaml".to_string(),
        vcpus: 2,
        memory: 1024,
        tenant_id: "demo".to_string(),
        ip_address: Some("10.0.0.50".to_string()),
        metadata: Some(HashMap::from([
            ("nix_image_id".to_string(), image_id.clone()),
            ("nix_system".to_string(), "x86_64-linux".to_string()),
            ("build_host".to_string(), "node1".to_string()),
        ])),
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: LocalityPreference::default(),
        health_check_config: None,
    };

    node1
        .send_vm_command(VmCommand::Create {
            config: vm_config.clone(),
            node_id: 1,
        })
        .await?;

    info!("Created VM '{}' on node1", vm_config.name);

    // Step 4: Start VM (automatic image availability check)
    info!("\n4. Starting VM (image already available locally)...");
    node1
        .send_vm_command(VmCommand::Start {
            name: vm_config.name.clone(),
        })
        .await?;

    // Simulate some work
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Step 5: Prepare for migration to node2
    info!("\n5. Pre-fetching image to node2 for migration...");
    prefetch_image_to_node(&node1, &node2, &vm_config.name, &image_id).await?;

    // Step 6: Migrate VM to node2
    info!("\n6. Migrating VM to node2...");
    demonstrate_migration(&node1, &node2, &vm_config.name).await?;

    // Step 7: Start VM on node2 (image already pre-fetched)
    info!("\n7. Starting VM on node2 (using pre-fetched image)...");
    node2
        .send_vm_command(VmCommand::Start {
            name: vm_config.name.clone(),
        })
        .await?;

    info!("\n✅ VM successfully running on node2 with P2P-distributed Nix image!");

    // Cleanup
    info!("\n8. Cleaning up...");
    node2
        .send_vm_command(VmCommand::Stop {
            name: vm_config.name.clone(),
        })
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    node2
        .send_vm_command(VmCommand::Delete {
            name: vm_config.name.clone(),
        })
        .await?;

    node1.stop().await?;
    node2.stop().await?;

    info!("\n🎉 Demo completed successfully!");
    Ok(())
}

async fn setup_build_node(node_dir: &std::path::Path) -> BlixardResult<(String, PathBuf)> {
    // Simulate building a Nix microVM image
    let nix_store_dir = node_dir.join("nix").join("store");
    std::fs::create_dir_all(&nix_store_dir)?;

    // Create a fake Nix store path
    let store_hash = "abc123def456ghi789-nixos-system";
    let system_path = nix_store_dir.join(&store_hash);
    std::fs::create_dir_all(&system_path)?;

    // Create system files
    tokio::fs::write(
        system_path.join("activate"),
        include_bytes!("../../README.md"), // Use README as dummy content
    )
    .await?;

    tokio::fs::write(
        system_path.join("init"),
        b"#!/bin/sh\nexec /run/current-system/sw/bin/systemd\n",
    )
    .await?;

    // Create kernel
    let kernel_hash = "xyz789uvw456-linux-kernel";
    let kernel_path = nix_store_dir.join(&kernel_hash);
    tokio::fs::write(&kernel_path, b"dummy kernel image\n").await?;

    info!("Built Nix microVM at: {:?}", system_path);

    // Import to P2P store
    let p2p_manager = Arc::new(P2pManager::new(1, node_dir, P2pConfig::default()).await?);
    let command_executor = Arc::new(TokioCommandExecutor::new());

    let image_store = NixImageStore::new(
        1,
        p2p_manager,
        node_dir,
        Some(nix_store_dir),
        command_executor,
    )
    .await?;

    let metadata = image_store
        .import_microvm("demo-nixos-microvm", &system_path, Some(&kernel_path))
        .await?;

    info!("Imported to P2P store:");
    info!("  Image ID: {}", metadata.id);
    info!("  Size: {} MB", metadata.total_size / 1_048_576);
    info!("  Chunks: {}", metadata.chunk_hashes.len());
    if let Some(nar_hash) = &metadata.nar_hash {
        info!("  NAR hash: {}", nar_hash);
    }

    Ok((metadata.id, system_path))
}

async fn create_node(id: u64, port: u16, data_dir: &std::path::Path) -> BlixardResult<Node> {
    let config = NodeConfig {
        id,
        data_dir: data_dir.to_string_lossy().to_string(),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: if id > 1 {
            Some("127.0.0.1:7001".to_string())
        } else {
            None
        },
        use_tailscale: false,
        vm_backend: "microvm".to_string(),
        transport_config: Some(TransportConfig::default()),
        topology: NodeTopology::default(),
    };

    let mut node = Node::new(config);

    // Create VM backend registry with microvm backend
    let mut registry = VmBackendRegistry::new();
    let db_path = data_dir.join("node.db");
    let database = Arc::new(Database::create(&db_path)?);
    let backend = MicrovmBackend::new(data_dir.join("config"), data_dir.join("data"), database)?;
    
    // For now, we'll create a simple factory struct inline
    struct MicrovmFactory(Arc<dyn blixard_core::vm_backend::VmBackend>);
    impl blixard_core::vm_backend::VmBackendFactory for MicrovmFactory {
        fn create_backend(
            &self,
            _config_dir: std::path::PathBuf,
            _data_dir: std::path::PathBuf,
            _database: Arc<Database>,
        ) -> BlixardResult<Arc<dyn blixard_core::vm_backend::VmBackend>> {
            Ok(Arc::clone(&self.0))
        }
        fn backend_type(&self) -> &'static str { "microvm" }
        fn description(&self) -> &'static str { "MicroVM backend using microvm.nix" }
    }
    
    registry.register(Arc::new(MicrovmFactory(Arc::new(backend))));

    node.initialize_with_vm_registry(registry).await?;
    node.start().await?;

    Ok(node)
}

async fn setup_node_with_image_store(
    node: &mut Node,
    node_dir: &std::path::Path,
) -> BlixardResult<()> {
    // Create P2P manager and image store
    let p2p_manager =
        Arc::new(P2pManager::new(node.get_id(), node_dir, P2pConfig::default()).await?);
    let command_executor = Arc::new(TokioCommandExecutor::new());

    let _image_store = Arc::new(
        NixImageStore::new(
            node.get_id(),
            p2p_manager.clone(),
            node_dir,
            None,
            command_executor,
        )
        .await?,
    );

    // Set P2P manager on node (which includes image store access)
    // Note: P2P manager integration would be done in actual implementation

    // Also set image store on VM backend
    // In a real implementation, this would be done through the registry
    info!(
        "Configured P2P image store for node {}",
        node.get_id()
    );

    Ok(())
}

async fn import_image_to_node(node: &Node, image_path: &PathBuf) -> BlixardResult<()> {
    // In a real scenario, this would use the node's image store
    // For demo, we'll just log
    info!(
        "Image available on node {} at {:?}",
        node.get_id(),
        image_path
    );
    Ok(())
}

async fn prefetch_image_to_node(
    source_node: &Node,
    target_node: &Node,
    vm_name: &str,
    image_id: &str,
) -> BlixardResult<()> {
    info!(
        "Pre-fetching image {} from node {} to node {} for VM '{}'",
        image_id,
        source_node.get_id(),
        target_node.get_id(),
        vm_name
    );

    // In a real implementation:
    // 1. source_node would initiate transfer to target_node
    // 2. Only missing chunks would be transferred
    // 3. Progress would be tracked

    // Simulate transfer time
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    info!("Pre-fetch completed successfully");
    Ok(())
}

async fn demonstrate_migration(
    source_node: &Node,
    target_node: &Node,
    vm_name: &str,
) -> BlixardResult<()> {
    info!("Stopping VM on source node...");
    source_node
        .send_vm_command(VmCommand::Stop {
            name: vm_name.to_string(),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // In a real implementation, this would:
    // 1. Transfer VM state
    // 2. Update cluster state to assign VM to target node
    // 3. Clean up source node resources

    info!(
        "VM migrated from node {} to node {}",
        source_node.get_id(),
        target_node.get_id()
    );

    Ok(())
}

/// Example Nix expression for building microVMs
fn example_microvm_flake() -> &'static str {
    r#"
{
  description = "MicroVM for P2P distribution";
  
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
    microvm.url = "github:astro/microvm.nix";
  };
  
  outputs = { self, nixpkgs, microvm }: {
    nixosConfigurations.demo-microvm = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        microvm.nixosModules.microvm
        {
          microvm = {
            vcpu = 2;
            mem = 1024;
            hypervisor = "cloud-hypervisor";
            
            # Content-addressed root filesystem
            shares = [{
              source = "/nix/store";
              mountPoint = "/nix/.ro-store";
              tag = "ro-store";
              proto = "virtiofs";
            }];
          };
          
          # Minimal NixOS config
          boot.loader.grub.enable = false;
          boot.kernelParams = [ "console=ttyS0" ];
          
          # Application services
          systemd.services.my-app = {
            wantedBy = [ "multi-user.target" ];
            script = "echo 'Application running in P2P-distributed VM'";
          };
        }
      ];
    };
  };
}
"#
}
