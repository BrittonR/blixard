//! Demonstrates real Nix NAR hash extraction and verification
//!
//! This example shows how Blixard uses Nix commands to:
//! - Extract NAR hashes from store paths
//! - Verify image integrity using NAR hashes
//! - Handle both store paths and regular paths

use blixard_core::{
    error::BlixardResult,
    nix_image_store::NixImageStore,
    p2p_manager::{P2pConfig, P2pManager},
};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tracing::info;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== Nix NAR Hash Extraction Demo ===");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path();

    // Set up P2P manager and image store
    let p2p_manager = Arc::new(P2pManager::new(1, data_dir, P2pConfig::default()).await?);

    let image_store = NixImageStore::new(
        1,
        p2p_manager,
        data_dir,
        None, // Use default /nix/store
    )
    .await?;

    // Test 1: Create a test file and import it
    info!("\n1. Creating test content...");
    let test_file = data_dir.join("test-content.txt");
    tokio::fs::write(&test_file, b"Hello from Blixard P2P!\n").await?;

    info!("\n2. Importing test file (will extract NAR metadata if in Nix store)...");
    let metadata = image_store
        .import_microvm("nar-hash-test", &test_file, None)
        .await?;

    info!("Import complete:");
    info!("  - Image ID: {}", metadata.id);
    info!("  - NAR hash: {:?}", metadata.nar_hash);
    info!("  - NAR size: {:?}", metadata.nar_size);

    // Test 2: Try with an actual Nix store path if available
    info!("\n3. Checking for existing Nix store paths...");
    let nix_hello = PathBuf::from("/nix/store").join("*-hello-*");

    // Use glob to find hello package if it exists
    if let Ok(entries) = glob::glob(&nix_hello.to_string_lossy()) {
        for entry in entries.filter_map(Result::ok).take(1) {
            info!("\n4. Found Nix store path: {:?}", entry);
            info!("   Importing to extract real NAR metadata...");

            let nix_metadata = image_store
                .import_microvm("nix-hello-test", &entry, None)
                .await?;

            info!("Nix store import complete:");
            info!("  - Image ID: {}", nix_metadata.id);
            info!("  - NAR hash: {:?}", nix_metadata.nar_hash);
            info!("  - NAR size: {:?}", nix_metadata.nar_size);
            info!("  - Derivation hash: {:?}", nix_metadata.derivation_hash);

            // Test verification
            if nix_metadata.nar_hash.is_some() {
                info!("\n5. Testing NAR hash verification...");
                match image_store.verify_image(&nix_metadata.id).await? {
                    true => info!("✅ Verification passed!"),
                    false => info!("❌ Verification failed"),
                }
            }
        }
    } else {
        info!("No hello package found in Nix store");
        info!("To test with real Nix paths, try:");
        info!("  nix-build '<nixpkgs>' -A hello");
    }

    // Test 3: Demonstrate NAR hash command directly
    info!("\n6. Direct Nix command examples:");
    info!("   To get NAR info: nix path-info --json /nix/store/...");
    info!(
        "   To compute NAR hash: nix-store --dump /path | nix-hash --type sha256 --base32 --flat"
    );
    info!("   To verify store path: nix store verify /nix/store/...");

    // Test 4: Show how to add a path to store
    let new_test = data_dir.join("add-to-store-test");
    tokio::fs::write(&new_test, b"Content to add to Nix store\n").await?;

    info!("\n7. Example: Adding path to Nix store");
    info!("   Command: nix store add-path {}", new_test.display());
    info!("   This would return a store path with NAR hash metadata");

    info!("\n✅ Demo completed!");
    info!("   Real Nix commands are now integrated for NAR hash extraction and verification");

    Ok(())
}
