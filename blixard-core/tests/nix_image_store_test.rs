use blixard_core::{
    error::BlixardResult,
    nix_image_store::{NixArtifactType, NixImageStore},
    p2p_manager::{P2pConfig, P2pManager},
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_microvm_import_and_download() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Create dummy microVM files
    let system_path = temp_dir.path().join("nixos-system");
    let kernel_path = temp_dir.path().join("kernel");
    tokio::fs::write(&system_path, b"dummy nixos system content").await?;
    tokio::fs::write(&kernel_path, b"dummy kernel content").await?;

    // Import the microVM
    let metadata = store
        .import_microvm("test-microvm", &system_path, Some(&kernel_path))
        .await?;

    assert_eq!(metadata.name, "test-microvm");
    assert!(matches!(
        metadata.artifact_type,
        NixArtifactType::MicroVM { .. }
    ));
    assert!(metadata.total_size > 0);
    assert!(metadata.chunk_hashes.len() > 0);

    // Check that NAR hash was extracted (simulated in test)
    if system_path.to_string_lossy().contains("nixos-system") {
        assert!(metadata.nar_hash.is_some(), "NAR hash should be present");
        assert!(metadata.nar_size.is_some(), "NAR size should be present");
    }

    // Test downloading the image
    let download_dir = temp_dir.path().join("downloads");
    let (downloaded_path, stats) = store
        .download_image(&metadata.id, Some(&download_dir))
        .await?;

    assert!(downloaded_path.exists());
    assert_eq!(stats.bytes_transferred, metadata.total_size);

    // Verify the downloaded image
    let is_valid = store.verify_image(&metadata.id).await?;
    assert!(is_valid, "Downloaded image should be valid");

    Ok(())
}

#[tokio::test]
async fn test_container_import() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Create dummy container tar
    let tar_path = temp_dir.path().join("container.tar");
    tokio::fs::write(&tar_path, b"dummy container tar content").await?;

    // Import the container
    let metadata = store.import_container("myapp:latest", &tar_path).await?;

    assert_eq!(metadata.name, "myapp:latest");
    assert!(matches!(
        metadata.artifact_type,
        NixArtifactType::Container { .. }
    ));

    Ok(())
}

#[tokio::test]
async fn test_chunk_deduplication() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Create two files with some shared content
    let file1_path = temp_dir.path().join("file1");
    let file2_path = temp_dir.path().join("file2");

    // Shared content to enable deduplication
    let shared_content = vec![0u8; 4 * 1024 * 1024]; // 4MB of zeros
    let unique_content1 = vec![1u8; 1024 * 1024]; // 1MB of ones
    let unique_content2 = vec![2u8; 1024 * 1024]; // 1MB of twos

    // File 1: shared + unique1
    let mut content1 = shared_content.clone();
    content1.extend(&unique_content1);
    tokio::fs::write(&file1_path, &content1).await?;

    // File 2: shared + unique2
    let mut content2 = shared_content.clone();
    content2.extend(&unique_content2);
    tokio::fs::write(&file2_path, &content2).await?;

    // Import both files
    let metadata1 = store.import_microvm("vm1", &file1_path, None).await?;
    let metadata2 = store.import_microvm("vm2", &file2_path, None).await?;

    // Check that deduplication occurred
    // Both files have the same shared chunk, so total unique chunks should be less
    // than the sum of individual chunks
    let total_chunks = metadata1.chunk_hashes.len() + metadata2.chunk_hashes.len();
    let unique_chunks: std::collections::HashSet<_> = metadata1
        .chunk_hashes
        .iter()
        .chain(metadata2.chunk_hashes.iter())
        .map(|c| &c.hash)
        .collect();

    assert!(
        unique_chunks.len() < total_chunks,
        "Expected deduplication, but got {} unique chunks from {} total chunks",
        unique_chunks.len(),
        total_chunks
    );

    Ok(())
}

#[tokio::test]
async fn test_garbage_collection() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Import an image
    let file_path = temp_dir.path().join("test-file");
    tokio::fs::write(&file_path, b"test content for gc").await?;
    let metadata = store.import_microvm("gc-test", &file_path, None).await?;

    // Download it to create local cache
    let (_, _) = store.download_image(&metadata.id, None).await?;

    // Run garbage collection with 0 duration (should collect everything old)
    let freed = store
        .garbage_collect(chrono::Duration::seconds(0), 0)
        .await?;

    // Since the image was just created, it shouldn't be collected
    assert_eq!(
        freed, 0,
        "Newly created image should not be garbage collected"
    );

    Ok(())
}

#[tokio::test]
async fn test_list_images() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Initially empty
    let images = store.list_images().await?;
    assert_eq!(images.len(), 0);

    // Import some images
    let file_path = temp_dir.path().join("test-file");
    tokio::fs::write(&file_path, b"test content").await?;

    store.import_microvm("vm1", &file_path, None).await?;
    store.import_microvm("vm2", &file_path, None).await?;
    store.import_container("app:v1", &file_path).await?;

    // List should now have 3 images
    let images = store.list_images().await?;
    assert_eq!(images.len(), 3);

    // Check names
    let names: Vec<_> = images.iter().map(|i| &i.name).collect();
    assert!(names.contains(&&"vm1".to_string()));
    assert!(names.contains(&&"vm2".to_string()));
    assert!(names.contains(&&"app:v1".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_prefetch_for_migration() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;

    // Import a VM image
    let file_path = temp_dir.path().join("vm-image");
    tokio::fs::write(&file_path, b"vm image content").await?;
    store
        .import_microvm("migration-test-vm", &file_path, None)
        .await?;

    // Prefetch for migration to node 2
    // This should not fail even though we don't have a real cluster
    store.prefetch_for_migration("migration-test-vm", 2).await?;

    Ok(())
}

#[tokio::test]
async fn test_nix_verification() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let nix_store_dir = temp_dir.path().join("nix").join("store");
    std::fs::create_dir_all(&nix_store_dir)?;

    let p2p_manager = Arc::new(P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?);

    let store =
        NixImageStore::new(1, p2p_manager, temp_dir.path(), Some(nix_store_dir.clone())).await?;

    // Create a Nix store path
    let store_hash = "abc123def456";
    let nixos_path = nix_store_dir.join(format!("{}-nixos-system", store_hash));
    std::fs::create_dir_all(&nixos_path)?;
    tokio::fs::write(nixos_path.join("init"), b"system init content").await?;

    // Import with Nix metadata
    let metadata = store
        .import_microvm("nix-verified-vm", &nixos_path, None)
        .await?;

    // Should have extracted derivation hash from path
    assert_eq!(metadata.derivation_hash, Some(store_hash.to_string()));

    // Should have NAR hash (simulated)
    assert!(metadata.nar_hash.is_some());
    assert!(metadata.nar_size.is_some());

    // Verify the image
    let is_valid = store.verify_image(&metadata.id).await?;
    assert!(is_valid, "Image with NAR hash should verify");

    // Test verification of non-existent image
    let result = store.verify_image("non-existent-id").await;
    assert!(result.is_err(), "Non-existent image should error");

    Ok(())
}
