//! Nix-aware P2P VM Image Store
//!
//! This module provides specialized support for distributing Nix-built
//! microVM images, containers, and store paths across the cluster using Iroh.
//! It handles content-addressed storage, derivation tracking, and efficient
//! deduplication of Nix store paths.

use crate::error::{BlixardError, BlixardResult};
use crate::p2p_manager::P2pManager;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use iroh_blobs::Hash;
use tokio::sync::RwLock;
use tracing::{info, debug};
use std::sync::Arc;

/// Type of Nix artifact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NixArtifactType {
    /// MicroVM configuration and rootfs
    MicroVM {
        /// The microVM system name
        system_name: String,
        /// Whether this includes kernel
        has_kernel: bool,
    },
    /// OCI container image
    Container {
        /// Container name and tag
        image_ref: String,
        /// OCI manifest digest
        manifest_digest: String,
    },
    /// Raw Nix store path
    StorePath {
        /// Original store path (e.g., /nix/store/...)
        path: String,
        /// Whether this is a derivation
        is_derivation: bool,
    },
    /// Nix closure (multiple store paths)
    Closure {
        /// Root store path
        root_path: String,
        /// Total number of paths in closure
        path_count: usize,
    },
}

/// Metadata for a Nix-built image or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixImageMetadata {
    /// Unique identifier (content-addressed)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Nix derivation hash (if available)
    pub derivation_hash: Option<String>,
    /// Type of artifact
    pub artifact_type: NixArtifactType,
    /// Total size in bytes
    pub total_size: u64,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Iroh hash of the content
    pub content_hash: String,
    /// Chunked hashes for deduplication
    pub chunk_hashes: Vec<ChunkMetadata>,
    /// Dependencies (other Nix paths)
    pub dependencies: Vec<String>,
    /// Runtime requirements
    pub runtime: RuntimeRequirements,
    /// Compression used
    pub compression: CompressionType,
    /// Signature for verification
    pub signature: Option<String>,
    /// Node that uploaded this
    pub uploaded_by: u64,
    /// Nodes that have this cached
    pub cached_on_nodes: HashSet<u64>,
}

/// Chunk metadata for deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Offset in the file
    pub offset: u64,
    /// Size of this chunk
    pub size: u64,
    /// Iroh hash of the chunk
    pub hash: String,
    /// Whether this chunk is commonly shared
    pub is_common: bool,
}

/// Runtime requirements for the artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequirements {
    /// Minimum kernel version required
    pub min_kernel: Option<String>,
    /// Required CPU features
    pub cpu_features: Vec<String>,
    /// Minimum memory in MB
    pub min_memory_mb: u64,
    /// Required hypervisor features
    pub hypervisor_features: Vec<String>,
}

/// Compression type for images
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionType {
    None,
    Gzip,
    Zstd,
    Xz,
}

/// Transfer statistics
#[derive(Debug, Clone)]
pub struct TransferStats {
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Bytes deduplicated (not transferred)
    pub bytes_deduplicated: u64,
    /// Number of chunks transferred
    pub chunks_transferred: usize,
    /// Number of chunks deduplicated
    pub chunks_deduplicated: usize,
    /// Transfer duration
    pub duration: std::time::Duration,
}

/// Nix-aware P2P image store
pub struct NixImageStore {
    /// P2P manager
    p2p_manager: Arc<P2pManager>,
    /// Local cache directory
    cache_dir: PathBuf,
    /// Nix store directory (usually /nix/store)
    nix_store_dir: PathBuf,
    /// Node ID
    node_id: u64,
    /// Metadata cache
    metadata_cache: Arc<RwLock<HashMap<String, NixImageMetadata>>>,
    /// Chunk deduplication index
    chunk_index: Arc<RwLock<HashMap<String, ChunkLocation>>>,
}

/// Location of a chunk
#[derive(Debug, Clone)]
struct ChunkLocation {
    /// Which images contain this chunk
    pub image_ids: HashSet<String>,
    /// Local file path if cached
    pub local_path: Option<PathBuf>,
}

impl NixImageStore {
    /// Create a new Nix image store
    pub async fn new(
        node_id: u64,
        p2p_manager: Arc<P2pManager>,
        data_dir: &Path,
        nix_store_dir: Option<PathBuf>,
    ) -> BlixardResult<Self> {
        let cache_dir = data_dir.join("nix-image-cache");
        std::fs::create_dir_all(&cache_dir)?;

        let nix_store_dir = nix_store_dir.unwrap_or_else(|| PathBuf::from("/nix/store"));

        Ok(Self {
            p2p_manager,
            cache_dir,
            nix_store_dir,
            node_id,
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
            chunk_index: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Import a Nix-built microVM image
    pub async fn import_microvm(
        &self,
        name: &str,
        system_path: &Path,
        kernel_path: Option<&Path>,
    ) -> BlixardResult<NixImageMetadata> {
        info!("Importing microVM {} from {:?}", name, system_path);

        // Create artifact type
        let artifact_type = NixArtifactType::MicroVM {
            system_name: name.to_string(),
            has_kernel: kernel_path.is_some(),
        };

        // Prepare paths to import
        let mut paths_to_import = vec![system_path];
        if let Some(kernel) = kernel_path {
            paths_to_import.push(kernel);
        }

        // Import with chunking
        self.import_nix_paths(name, artifact_type, &paths_to_import).await
    }

    /// Import a container image from Nix store
    pub async fn import_container(
        &self,
        image_ref: &str,
        tar_path: &Path,
    ) -> BlixardResult<NixImageMetadata> {
        info!("Importing container {} from {:?}", image_ref, tar_path);

        // Extract manifest digest from tar
        let manifest_digest = self.extract_oci_manifest_digest(tar_path).await?;

        let artifact_type = NixArtifactType::Container {
            image_ref: image_ref.to_string(),
            manifest_digest,
        };

        self.import_nix_paths(image_ref, artifact_type, &[tar_path]).await
    }

    /// Import a Nix closure (with all dependencies)
    pub async fn import_closure(
        &self,
        name: &str,
        root_path: &Path,
    ) -> BlixardResult<NixImageMetadata> {
        info!("Importing Nix closure for {:?}", root_path);

        // Get all paths in the closure
        let closure_paths = self.get_nix_closure(root_path).await?;
        let path_count = closure_paths.len();

        let artifact_type = NixArtifactType::Closure {
            root_path: root_path.to_string_lossy().to_string(),
            path_count,
        };

        // Convert to Path references
        let path_refs: Vec<&Path> = closure_paths.iter().map(|p| p.as_path()).collect();
        self.import_nix_paths(name, artifact_type, &path_refs).await
    }

    /// Core import function with chunking and deduplication
    async fn import_nix_paths(
        &self,
        name: &str,
        artifact_type: NixArtifactType,
        paths: &[&Path],
    ) -> BlixardResult<NixImageMetadata> {
        let mut total_size = 0u64;
        let mut all_chunks = Vec::new();
        let mut dependencies = Vec::new();

        // Process each path
        for path in paths {
            // Get file size
            let file_metadata = std::fs::metadata(path)?;
            total_size += file_metadata.len();

            // Extract dependencies if it's a store path
            if path.starts_with(&self.nix_store_dir) {
                let deps = self.get_nix_dependencies(path).await?;
                dependencies.extend(deps);
            }

            // Chunk the file for deduplication
            let chunks = self.chunk_file(path).await?;
            all_chunks.extend(chunks);
        }

        // Deduplicate chunks across existing images
        let (unique_chunks, dedup_stats) = self.deduplicate_chunks(&all_chunks).await?;
        
        info!(
            "Chunking complete: {} total chunks, {} unique ({:.1}% deduplication)",
            all_chunks.len(),
            unique_chunks.len(),
            (1.0 - (unique_chunks.len() as f64 / all_chunks.len() as f64)) * 100.0
        );

        // Upload unique chunks
        let content_hash = self.upload_chunks(&unique_chunks).await?;

        // Create metadata
        let metadata = NixImageMetadata {
            id: content_hash.to_string(),
            name: name.to_string(),
            derivation_hash: self.extract_derivation_hash(paths[0]),
            artifact_type,
            total_size,
            created_at: Utc::now(),
            content_hash: content_hash.to_string(),
            chunk_hashes: all_chunks,
            dependencies,
            runtime: self.detect_runtime_requirements(paths).await?,
            compression: CompressionType::Zstd,
            signature: None, // TODO: Add Nix signature support
            uploaded_by: self.node_id,
            cached_on_nodes: HashSet::from([self.node_id]),
        };

        // Store metadata
        self.store_metadata(&metadata).await?;

        // Update chunk index
        self.update_chunk_index(&metadata).await?;

        info!("Successfully imported {} with {} chunks", name, all_chunks.len());
        Ok(metadata)
    }

    /// Download and materialize a Nix image
    pub async fn download_image(
        &self,
        image_id: &str,
        target_dir: Option<&Path>,
    ) -> BlixardResult<(PathBuf, TransferStats)> {
        info!("Downloading Nix image {}", image_id);

        // Get metadata
        let metadata = self.get_metadata(image_id).await?
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Nix image {}", image_id),
            })?;

        // Determine target directory
        let target_dir = target_dir.unwrap_or(&self.cache_dir);
        let target_path = match &metadata.artifact_type {
            NixArtifactType::MicroVM { system_name, .. } => {
                target_dir.join(format!("microvm-{}", system_name))
            }
            NixArtifactType::Container { image_ref, .. } => {
                target_dir.join(format!("container-{}", image_ref.replace('/', "-")))
            }
            NixArtifactType::StorePath { path, .. } => {
                // Try to maintain Nix store structure if possible
                if target_dir.starts_with("/nix/store") {
                    PathBuf::from(path)
                } else {
                    target_dir.join(Path::new(path).file_name().unwrap())
                }
            }
            NixArtifactType::Closure { root_path, .. } => {
                target_dir.join(format!("closure-{}", Path::new(root_path).file_name().unwrap().to_string_lossy()))
            }
        };

        // Check what chunks we already have
        let (chunks_needed, chunks_have) = self.analyze_chunks_needed(&metadata.chunk_hashes).await?;
        
        info!(
            "Need to download {} chunks, already have {} chunks",
            chunks_needed.len(),
            chunks_have.len()
        );

        let start_time = std::time::Instant::now();
        let mut bytes_transferred = 0u64;
        let mut bytes_deduplicated = 0u64;

        // Download missing chunks
        for chunk in &chunks_needed {
            let chunk_data = self.download_chunk(&chunk.hash).await?;
            bytes_transferred += chunk_data.len() as u64;
            
            // Store chunk for future deduplication
            self.store_chunk(&chunk.hash, &chunk_data).await?;
        }

        // Calculate deduplicated bytes
        for chunk in &chunks_have {
            bytes_deduplicated += chunk.size;
        }

        // Reassemble the image from chunks
        self.reassemble_image(&metadata, &target_path).await?;

        let stats = TransferStats {
            bytes_transferred,
            bytes_deduplicated,
            chunks_transferred: chunks_needed.len(),
            chunks_deduplicated: chunks_have.len(),
            duration: start_time.elapsed(),
        };

        info!(
            "Downloaded {} in {:?} ({:.2} MB/s, {:.1}% deduplicated)",
            image_id,
            stats.duration,
            (bytes_transferred as f64 / stats.duration.as_secs_f64()) / 1_048_576.0,
            (bytes_deduplicated as f64 / (bytes_transferred + bytes_deduplicated) as f64) * 100.0
        );

        Ok((target_path, stats))
    }

    /// Pre-fetch images for upcoming migrations
    pub async fn prefetch_for_migration(
        &self,
        vm_name: &str,
        target_node: u64,
    ) -> BlixardResult<()> {
        info!("Pre-fetching images for VM {} migration to node {}", vm_name, target_node);

        // Get current VM image metadata
        let images = self.list_images_for_vm(vm_name).await?;
        
        for image in images {
            // Check if target node already has this image
            if image.cached_on_nodes.contains(&target_node) {
                debug!("Node {} already has image {}", target_node, image.id);
                continue;
            }

            // Initiate background transfer to target node
            self.initiate_background_transfer(&image.id, target_node).await?;
        }

        Ok(())
    }

    /// Garbage collect unused images
    pub async fn garbage_collect(
        &self,
        keep_recent: chrono::Duration,
        keep_min_copies: usize,
    ) -> BlixardResult<u64> {
        info!("Running garbage collection (keep_recent: {:?}, keep_min_copies: {})", 
               keep_recent, keep_min_copies);

        let mut total_freed = 0u64;
        let cutoff_time = Utc::now() - keep_recent;

        let metadata_cache = self.metadata_cache.read().await;
        let images_to_check: Vec<_> = metadata_cache.values()
            .filter(|m| m.created_at < cutoff_time)
            .cloned()
            .collect();
        drop(metadata_cache);

        for metadata in images_to_check {
            // Check if this image is still in use
            if self.is_image_in_use(&metadata.id).await? {
                continue;
            }

            // Check if we have minimum copies across cluster
            if metadata.cached_on_nodes.len() <= keep_min_copies {
                continue;
            }

            // Safe to remove from this node
            let freed = self.remove_local_cache(&metadata).await?;
            total_freed += freed;
        }

        info!("Garbage collection freed {} bytes", total_freed);
        Ok(total_freed)
    }

    // Helper methods

    async fn chunk_file(&self, path: &Path) -> BlixardResult<Vec<ChunkMetadata>> {
        const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB chunks
        let mut chunks = Vec::new();
        let file = tokio::fs::File::open(path).await?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut offset = 0u64;
        let mut buffer = vec![0u8; CHUNK_SIZE];

        use tokio::io::AsyncReadExt;
        loop {
            let n = reader.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            let chunk_data = &buffer[..n];
            let hash = self.p2p_manager.hash_data(chunk_data).await?;
            
            chunks.push(ChunkMetadata {
                offset,
                size: n as u64,
                hash: hash.to_string(),
                is_common: false, // Will be determined during deduplication
            });

            offset += n as u64;
        }

        Ok(chunks)
    }

    async fn deduplicate_chunks(
        &self,
        chunks: &[ChunkMetadata],
    ) -> BlixardResult<(Vec<ChunkMetadata>, HashMap<String, usize>)> {
        let mut unique_chunks = Vec::new();
        let mut dedup_stats = HashMap::new();
        let chunk_index = self.chunk_index.read().await;

        for chunk in chunks {
            if chunk_index.contains_key(&chunk.hash) {
                *dedup_stats.entry(chunk.hash.clone()).or_insert(0) += 1;
            } else {
                unique_chunks.push(chunk.clone());
            }
        }

        Ok((unique_chunks, dedup_stats))
    }

    async fn upload_chunks(&self, chunks: &[ChunkMetadata]) -> BlixardResult<Hash> {
        // For now, create a manifest of chunks and upload that
        let manifest = serde_json::to_vec(chunks)?;
        let hash = self.p2p_manager.share_data(&manifest).await?;
        Ok(hash)
    }

    async fn get_nix_closure(&self, path: &Path) -> BlixardResult<Vec<PathBuf>> {
        use tokio::process::Command;
        
        let output = Command::new("nix-store")
            .args(&["-qR", &path.to_string_lossy()])
            .output()
            .await?;

        if !output.status.success() {
            return Err(BlixardError::Internal {
                message: format!("Failed to get Nix closure: {}", String::from_utf8_lossy(&output.stderr)),
            });
        }

        let paths = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(PathBuf::from)
            .collect();

        Ok(paths)
    }

    async fn get_nix_dependencies(&self, path: &Path) -> BlixardResult<Vec<String>> {
        use tokio::process::Command;
        
        let output = Command::new("nix-store")
            .args(&["-q", "--references", &path.to_string_lossy()])
            .output()
            .await?;

        if !output.status.success() {
            // Not all paths have references, that's OK
            return Ok(Vec::new());
        }

        let deps = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(String::from)
            .collect();

        Ok(deps)
    }

    fn extract_derivation_hash(&self, path: &Path) -> Option<String> {
        // Extract hash from Nix store path
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.split('-').next())
            .map(String::from)
    }

    async fn detect_runtime_requirements(&self, paths: &[&Path]) -> BlixardResult<RuntimeRequirements> {
        // Simple detection based on common patterns
        // In production, would parse VM configs or container manifests
        Ok(RuntimeRequirements {
            min_kernel: Some("5.10".to_string()),
            cpu_features: vec![],
            min_memory_mb: 256,
            hypervisor_features: vec!["virtio".to_string()],
        })
    }

    async fn extract_oci_manifest_digest(&self, tar_path: &Path) -> BlixardResult<String> {
        // In a real implementation, would extract and parse OCI manifest
        // For now, return a placeholder
        Ok(format!("sha256:{:x}", md5::compute(tar_path.to_string_lossy().as_bytes())))
    }

    async fn store_metadata(&self, metadata: &NixImageMetadata) -> BlixardResult<()> {
        let mut cache = self.metadata_cache.write().await;
        cache.insert(metadata.id.clone(), metadata.clone());
        
        // Also persist to P2P network
        let key = format!("nix-image:{}", metadata.id);
        let data = serde_json::to_vec(metadata)?;
        self.p2p_manager.store_metadata(&key, &data).await?;
        
        Ok(())
    }

    async fn get_metadata(&self, image_id: &str) -> BlixardResult<Option<NixImageMetadata>> {
        // Check local cache first
        {
            let cache = self.metadata_cache.read().await;
            if let Some(metadata) = cache.get(image_id) {
                return Ok(Some(metadata.clone()));
            }
        }

        // Try to fetch from P2P network
        let key = format!("nix-image:{}", image_id);
        match self.p2p_manager.get_metadata(&key).await {
            Ok(data) => {
                let metadata: NixImageMetadata = serde_json::from_slice(&data)?;
                
                // Update local cache
                let mut cache = self.metadata_cache.write().await;
                cache.insert(image_id.to_string(), metadata.clone());
                
                Ok(Some(metadata))
            }
            Err(_) => Ok(None),
        }
    }

    async fn update_chunk_index(&self, metadata: &NixImageMetadata) -> BlixardResult<()> {
        let mut index = self.chunk_index.write().await;
        
        for chunk in &metadata.chunk_hashes {
            let location = index.entry(chunk.hash.clone()).or_insert_with(|| ChunkLocation {
                image_ids: HashSet::new(),
                local_path: None,
            });
            location.image_ids.insert(metadata.id.clone());
        }
        
        Ok(())
    }

    async fn analyze_chunks_needed(
        &self,
        chunks: &[ChunkMetadata],
    ) -> BlixardResult<(Vec<ChunkMetadata>, Vec<ChunkMetadata>)> {
        let index = self.chunk_index.read().await;
        let mut needed = Vec::new();
        let mut have = Vec::new();

        for chunk in chunks {
            if index.get(&chunk.hash).and_then(|loc| loc.local_path.as_ref()).is_some() {
                have.push(chunk.clone());
            } else {
                needed.push(chunk.clone());
            }
        }

        Ok((needed, have))
    }

    async fn download_chunk(&self, hash: &str) -> BlixardResult<Vec<u8>> {
        // Convert string hash to Iroh hash
        let iroh_hash = Hash::from_str(hash)
            .map_err(|e| BlixardError::Internal { message: e.to_string() })?;
        
        self.p2p_manager.download_data(iroh_hash).await
    }

    async fn store_chunk(&self, hash: &str, data: &[u8]) -> BlixardResult<()> {
        let chunk_path = self.cache_dir.join("chunks").join(hash);
        std::fs::create_dir_all(chunk_path.parent().unwrap())?;
        tokio::fs::write(&chunk_path, data).await?;

        // Update index
        let mut index = self.chunk_index.write().await;
        if let Some(location) = index.get_mut(hash) {
            location.local_path = Some(chunk_path);
        }

        Ok(())
    }

    async fn reassemble_image(
        &self,
        metadata: &NixImageMetadata,
        target_path: &Path,
    ) -> BlixardResult<()> {
        std::fs::create_dir_all(target_path.parent().unwrap())?;
        
        let mut output = tokio::fs::File::create(target_path).await?;
        use tokio::io::AsyncWriteExt;

        for chunk in &metadata.chunk_hashes {
            let chunk_data = self.read_chunk(&chunk.hash).await?;
            output.write_all(&chunk_data).await?;
        }

        output.flush().await?;
        Ok(())
    }

    async fn read_chunk(&self, hash: &str) -> BlixardResult<Vec<u8>> {
        let index = self.chunk_index.read().await;
        
        if let Some(location) = index.get(hash) {
            if let Some(path) = &location.local_path {
                return Ok(tokio::fs::read(path).await?);
            }
        }

        // Need to download
        self.download_chunk(hash).await
    }

    async fn list_images_for_vm(&self, vm_name: &str) -> BlixardResult<Vec<NixImageMetadata>> {
        let cache = self.metadata_cache.read().await;
        let images: Vec<_> = cache.values()
            .filter(|m| m.name.contains(vm_name))
            .cloned()
            .collect();
        Ok(images)
    }

    async fn initiate_background_transfer(
        &self,
        image_id: &str,
        target_node: u64,
    ) -> BlixardResult<()> {
        // In a real implementation, would send transfer request to target node
        info!("Initiating background transfer of {} to node {}", image_id, target_node);
        Ok(())
    }

    async fn is_image_in_use(&self, image_id: &str) -> BlixardResult<bool> {
        // Check if any running VMs are using this image
        // For now, return false (not implemented)
        Ok(false)
    }

    async fn remove_local_cache(&self, metadata: &NixImageMetadata) -> BlixardResult<u64> {
        let mut freed = 0u64;

        // Remove chunks that are only used by this image
        let mut index = self.chunk_index.write().await;
        for chunk in &metadata.chunk_hashes {
            if let Some(location) = index.get_mut(&chunk.hash) {
                location.image_ids.remove(&metadata.id);
                
                if location.image_ids.is_empty() {
                    // This chunk is no longer needed
                    if let Some(path) = &location.local_path {
                        if path.exists() {
                            let size = std::fs::metadata(path)?.len();
                            std::fs::remove_file(path)?;
                            freed += size;
                        }
                    }
                    index.remove(&chunk.hash);
                }
            }
        }

        // Remove from metadata cache
        let mut cache = self.metadata_cache.write().await;
        cache.remove(&metadata.id);

        Ok(freed)
    }
    
    /// List all available images
    pub async fn list_images(&self) -> BlixardResult<Vec<NixImageMetadata>> {
        let cache = self.metadata_cache.read().await;
        Ok(cache.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_nix_image_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let p2p_manager = Arc::new(
            P2pManager::new(1, temp_dir.path(), Default::default())
                .await
                .unwrap()
        );
        
        let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None)
            .await
            .unwrap();
        
        assert!(temp_dir.path().join("nix-image-cache").exists());
    }

    #[tokio::test]
    async fn test_chunk_deduplication() {
        // Test that common chunks are properly deduplicated
        let temp_dir = TempDir::new().unwrap();
        let p2p_manager = Arc::new(
            P2pManager::new(1, temp_dir.path(), Default::default())
                .await
                .unwrap()
        );
        
        let store = NixImageStore::new(1, p2p_manager, temp_dir.path(), None)
            .await
            .unwrap();
        
        // Create some test chunks
        let chunks = vec![
            ChunkMetadata {
                offset: 0,
                size: 1024,
                hash: "hash1".to_string(),
                is_common: false,
            },
            ChunkMetadata {
                offset: 1024,
                size: 1024,
                hash: "hash2".to_string(),
                is_common: false,
            },
            ChunkMetadata {
                offset: 2048,
                size: 1024,
                hash: "hash1".to_string(), // Duplicate
                is_common: false,
            },
        ];
        
        let (unique, stats) = store.deduplicate_chunks(&chunks).await.unwrap();
        assert_eq!(unique.len(), 2);
        assert_eq!(stats.get("hash1"), None); // First occurrence not in stats
    }
}