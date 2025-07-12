//! Example demonstrating Nix image verification using NAR hashes
//!
//! This example shows how Blixard uses Nix's cryptographic guarantees
//! to ensure image integrity during P2P distribution.

use blixard_core::{
    abstractions::command::TokioCommandExecutor,
    error::BlixardResult,
    nix_image_store::NixImageStore,
    p2p_manager::{P2pConfig, P2pManager},
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

    info!("=== Nix Image Verification Demo ===");

    // Create temporary directories
    let temp_dir = TempDir::new()?;
    let nix_store_dir = temp_dir.path().join("nix").join("store");
    std::fs::create_dir_all(&nix_store_dir)?;

    // Create P2P manager and image store
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);
    let command_executor = Arc::new(TokioCommandExecutor::new());

    let store = NixImageStore::new(
        1,
        p2p_manager,
        temp_dir.path(),
        Some(nix_store_dir.clone()),
        command_executor,
    )
    .await?;

    // Demo 1: Import with NAR hash verification
    info!("\n--- Demo 1: Import with NAR Hash ---");
    demo_import_with_verification(&store, &nix_store_dir).await?;

    // Demo 2: Verify existing image
    info!("\n--- Demo 2: Verify Existing Image ---");
    demo_verify_existing(&store).await?;

    // Demo 3: Detect tampering
    info!("\n--- Demo 3: Detect Tampering ---");
    demo_detect_tampering(&store).await?;

    // Demo 4: Show Nix integration
    info!("\n--- Demo 4: Nix Integration ---");
    demo_nix_integration();

    info!("\n✅ All verification demos completed successfully!");
    Ok(())
}

async fn demo_import_with_verification(
    store: &NixImageStore,
    nix_store_dir: &std::path::Path,
) -> BlixardResult<()> {
    // Create a fake Nix store path
    let store_hash = "abc123def456ghi789jkl012mno345p";
    let nixos_path = nix_store_dir.join(format!("{}-nixos-system", store_hash));
    std::fs::create_dir_all(&nixos_path)?;

    // Create some content
    let system_file = nixos_path.join("activate");
    tokio::fs::write(&system_file, b"#!/bin/sh\necho 'Activating NixOS system'\n").await?;

    // Import the system
    let metadata = store
        .import_microvm("verified-system", &nixos_path, None)
        .await?;

    info!("Imported system with metadata:");
    info!("  ID: {}", metadata.id);
    info!("  Derivation hash: {:?}", metadata.derivation_hash);
    info!("  NAR hash: {:?}", metadata.nar_hash);
    info!("  NAR size: {:?}", metadata.nar_size);
    info!("  Total size: {} bytes", metadata.total_size);

    Ok(())
}

async fn demo_verify_existing(store: &NixImageStore) -> BlixardResult<()> {
    // Get list of images
    let images = store.list_images().await?;

    for image in images {
        info!("Verifying image: {}", image.name);

        let is_valid = store.verify_image(&image.id).await?;

        if is_valid {
            info!("  ✓ Image {} is valid", image.name);
        } else {
            info!("  ✗ Image {} verification failed", image.name);
        }

        if let Some(nar_hash) = &image.nar_hash {
            info!("  NAR hash: {}", nar_hash);
        }
    }

    Ok(())
}

async fn demo_detect_tampering(store: &NixImageStore) -> BlixardResult<()> {
    info!("Simulating image tampering detection...");

    // In a real scenario, if someone modified the image after download,
    // the verification would fail because the NAR hash wouldn't match

    // This demonstrates the security model:
    info!("Security properties:");
    info!("  1. NAR hash ensures bit-for-bit reproducibility");
    info!("  2. Derivation hash links to exact build instructions");
    info!("  3. P2P transfer preserves cryptographic verification");
    info!("  4. Any tampering is immediately detectable");

    Ok(())
}

fn demo_nix_integration() {
    info!("Nix commands for verification:");
    info!("");
    info!("  # Get NAR hash and size:");
    info!("  nix path-info --json /nix/store/...-nixos-system");
    info!("");
    info!("  # Verify a path:");
    info!("  nix store verify /nix/store/...-nixos-system");
    info!("");
    info!("  # Calculate NAR hash:");
    info!("  nix hash path --type sha256 --base32 /nix/store/...");
    info!("");
    info!("  # Show signatures:");
    info!("  nix path-info --sigs /nix/store/...");
    info!("");
    info!("Integration points:");
    info!("  - Import: Extract NAR hash during import");
    info!("  - Transfer: Preserve hash in metadata");
    info!("  - Verify: Check hash after download");
    info!("  - Trust: Use Nix binary cache signatures");
}

// Example output showing real Nix metadata
fn example_nix_path_info() -> &'static str {
    r#"
{
  "/nix/store/abc123...-nixos-system": {
    "narHash": "sha256:1i8xdp2n9r9v4n4fz1p3xrp1h3nfgfxz9v6z2vqz3x4xwrfx6xvx",
    "narSize": 104857600,
    "references": [
      "/nix/store/def456...-glibc-2.35",
      "/nix/store/ghi789...-systemd-253.1"
    ],
    "deriver": "/nix/store/jkl012...-nixos-system.drv",
    "signatures": [
      "cache.nixos.org-1:XYZ123..."
    ],
    "ultimate": false,
    "ca": null
  }
}
"#
}
