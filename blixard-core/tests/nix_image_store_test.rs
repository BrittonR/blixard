use blixard_core::{
    error::BlixardResult,
    nix_image_store::{NixImageStore, NixArtifactType},
    p2p_manager::{P2pManager, P2pConfig},
};
use std::sync::Arc;
use tempfile::TempDir;
use std::path::PathBuf;

#[tokio::test]
async fn test_microvm_import_and_download() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;
    
    // Create dummy microVM files
    let system_path = temp_dir.path().join("nixos-system");
    let kernel_path = temp_dir.path().join("kernel");
    tokio::fs::write(&system_path, b"dummy nixos system content").await?;
    tokio::fs::write(&kernel_path, b"dummy kernel content").await?;
    
    // Import the microVM
    let metadata = store.import_microvm(
        "test-microvm",
        &system_path,
        Some(&kernel_path),
    ).await?;
    
    assert_eq!(metadata.name, "test-microvm");
    assert!(matches!(metadata.artifact_type, NixArtifactType::MicroVM { .. }));
    assert!(metadata.total_size > 0);
    assert!(metadata.chunk_hashes.len() > 0);
    
    // Test downloading the image
    let download_dir = temp_dir.path().join("downloads");
    let (downloaded_path, stats) = store.download_image(
        &metadata.id,
        Some(&download_dir),
    ).await?;
    
    assert!(downloaded_path.exists());
    assert_eq!(stats.bytes_transferred, metadata.total_size);
    
    Ok(())
}

#[tokio::test]
async fn test_container_import() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;
    
    // Create dummy container tar
    let tar_path = temp_dir.path().join("container.tar");
    tokio::fs::write(&tar_path, b"dummy container tar content").await?;
    
    // Import the container
    let metadata = store.import_container(
        "myapp:latest",
        &tar_path,
    ).await?;
    
    assert_eq!(metadata.name, "myapp:latest");
    assert!(matches!(metadata.artifact_type, NixArtifactType::Container { .. }));
    
    Ok(())
}

#[tokio::test]
async fn test_chunk_deduplication() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
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
    let unique_chunks: std::collections::HashSet<_> = 
        metadata1.chunk_hashes.iter()
        .chain(metadata2.chunk_hashes.iter())
        .map(|c| &c.hash)
        .collect();
    
    assert!(unique_chunks.len() < total_chunks, 
            "Expected deduplication, but got {} unique chunks from {} total chunks",
            unique_chunks.len(), total_chunks);
    
    Ok(())
}

#[tokio::test]
async fn test_garbage_collection() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;
    
    // Import an image
    let file_path = temp_dir.path().join("test-file");
    tokio::fs::write(&file_path, b"test content for gc").await?;
    let metadata = store.import_microvm("gc-test", &file_path, None).await?;
    
    // Download it to create local cache
    let (_, _) = store.download_image(&metadata.id, None).await?;
    
    // Run garbage collection with 0 duration (should collect everything old)
    let freed = store.garbage_collect(chrono::Duration::seconds(0), 0).await?;
    
    // Since the image was just created, it shouldn't be collected
    assert_eq!(freed, 0, "Newly created image should not be garbage collected");
    
    Ok(())
}

#[tokio::test]
async fn test_list_images() -> BlixardResult<()> {
    let temp_dir = TempDir::new()?;
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
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
    let p2p_manager = Arc::new(
        P2pManager::new(1, temp_dir.path(), P2pConfig::default()).await?
    );
    
    let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None).await?;
    
    // Import a VM image
    let file_path = temp_dir.path().join("vm-image");
    tokio::fs::write(&file_path, b"vm image content").await?;
    store.import_microvm("migration-test-vm", &file_path, None).await?;
    
    // Prefetch for migration to node 2
    // This should not fail even though we don't have a real cluster
    store.prefetch_for_migration("migration-test-vm", 2).await?;
    
    Ok(())
}