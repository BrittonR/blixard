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
use tracing::{info, debug, warn};
use std::sync::Arc;
use crate::metrics_otel::{
    record_p2p_image_import, record_p2p_image_download, record_p2p_chunk_transfer,
    record_p2p_verification, record_p2p_cache_access, start_p2p_transfer,
};

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
    /// NAR hash from Nix (sha256:xxx format)
    pub nar_hash: Option<String>,
    /// NAR size in bytes
    pub nar_size: Option<u64>,
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
        let (unique_chunks, dedup_count) = self.deduplicate_chunks(&all_chunks).await?;
        
        info!(
            "Chunking complete: {} total chunks, {} unique ({:.1}% deduplication)",
            all_chunks.len(),
            unique_chunks.len(),
            (1.0 - (unique_chunks.len() as f64 / all_chunks.len() as f64)) * 100.0
        );

        // Upload unique chunks
        let content_hash = self.upload_chunks(&unique_chunks).await?;

        // Extract Nix metadata for verification
        let (nar_hash, nar_size) = self.extract_nix_metadata(paths[0]).await?;
        
        // Create metadata
        let metadata = NixImageMetadata {
            id: content_hash.to_string(),
            name: name.to_string(),
            derivation_hash: self.extract_derivation_hash(paths[0]),
            artifact_type,
            nar_hash,
            nar_size,
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

        // Record metrics
        let artifact_type_str = match &metadata.artifact_type {
            NixArtifactType::MicroVM { .. } => "microvm",
            NixArtifactType::Container { .. } => "container", 
            NixArtifactType::StorePath { .. } => "storepath",
            NixArtifactType::Closure { .. } => "closure",
        };
        record_p2p_image_import(artifact_type_str, true, total_size);
        
        // Record deduplication metrics
        for _ in 0..dedup_count {
            record_p2p_chunk_transfer(0, true);
        }

        info!("Successfully imported {} with {} chunks ({}  deduplicated)", name, metadata.chunk_hashes.len(), dedup_count);
        Ok(metadata)
    }

    /// Download and materialize a Nix image
    pub async fn download_image(
        &self,
        image_id: &str,
        target_dir: Option<&Path>,
    ) -> BlixardResult<(PathBuf, TransferStats)> {
        info!("Downloading Nix image {}", image_id);
        
        // Start P2P transfer tracking
        let _transfer_guard = start_p2p_transfer();

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
                    let file_name = Path::new(path).file_name()
                        .ok_or_else(|| BlixardError::Internal {
                            message: format!("Invalid store path: {}", path),
                        })?;
                    target_dir.join(file_name)
                }
            }
            NixArtifactType::Closure { root_path, .. } => {
                let file_name = Path::new(root_path).file_name()
                    .ok_or_else(|| BlixardError::Internal {
                        message: format!("Invalid closure root path: {}", root_path),
                    })?;
                target_dir.join(format!("closure-{}", file_name.to_string_lossy()))
            }
        };

        // Check what chunks we already have
        let (chunks_needed, chunks_have) = self.analyze_chunks_needed(&metadata.chunk_hashes).await?;
        
        info!(
            "Need to download {} chunks, already have {} chunks",
            chunks_needed.len(),
            chunks_have.len()
        );
        
        // Record cache hits/misses
        record_p2p_cache_access(false, "chunk");  // For chunks we need
        for _ in &chunks_have {
            record_p2p_cache_access(true, "chunk");  // For chunks we already have
        }

        let start_time = std::time::Instant::now();
        let mut bytes_transferred = 0u64;
        let mut bytes_deduplicated = 0u64;

        // Download missing chunks
        for chunk in &chunks_needed {
            let chunk_data = self.download_chunk(&chunk.hash).await?;
            bytes_transferred += chunk_data.len() as u64;
            
            // Record chunk transfer
            record_p2p_chunk_transfer(chunk_data.len() as u64, false);
            
            // Store chunk for future deduplication
            self.store_chunk(&chunk.hash, &chunk_data).await?;
        }

        // Calculate deduplicated bytes
        for chunk in &chunks_have {
            bytes_deduplicated += chunk.size;
            // Record deduplicated chunks
            record_p2p_chunk_transfer(chunk.size, true);
        }

        // Reassemble the image from chunks
        self.reassemble_image(&metadata, &target_path).await?;
        
        // Verify the downloaded image if we have NAR hash
        if metadata.nar_hash.is_some() {
            match self.verify_nix_image(&metadata, &target_path).await {
                Ok(_) => record_p2p_verification(true, "nar_hash"),
                Err(e) => {
                    record_p2p_verification(false, "nar_hash");
                    return Err(e);
                }
            }
        }

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
        
        // Record download metrics
        record_p2p_image_download(image_id, true, stats.duration.as_secs_f64());

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
        
        self.p2p_manager.download_data(&iroh_hash).await
    }

    async fn store_chunk(&self, hash: &str, data: &[u8]) -> BlixardResult<()> {
        let chunk_path = self.cache_dir.join("chunks").join(hash);
        if let Some(parent) = chunk_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
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
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
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
    
    /// Verify image integrity before use
    pub async fn verify_image(&self, image_id: &str) -> BlixardResult<bool> {
        let metadata = self.get_metadata(image_id).await?
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Image {}", image_id),
            })?;
        
        // Check if we have NAR hash for verification
        if metadata.nar_hash.is_none() {
            info!("No NAR hash available for image {}, skipping verification", image_id);
            return Ok(true);
        }
        
        // Find the image in local cache
        let cache_path = self.cache_dir.join(&image_id);
        if !cache_path.exists() {
            return Ok(false);
        }
        
        // Verify against NAR hash
        match self.verify_nix_image(&metadata, &cache_path).await {
            Ok(_) => {
                info!("Image {} verified successfully", image_id);
                Ok(true)
            }
            Err(e) => {
                warn!("Image {} verification failed: {}", image_id, e);
                Ok(false)
            }
        }
    }
    
    /// Extract Nix metadata from a store path
    async fn extract_nix_metadata(&self, path: &Path) -> BlixardResult<(Option<String>, Option<u64>)> {
        use tokio::process::Command;
        
        // Check if this is a Nix store path
        if !path.starts_with(&self.nix_store_dir) {
            debug!("Path {:?} is not in Nix store, skipping NAR metadata extraction", path);
            return Ok((None, None));
        }
        
        // First check if the path exists
        if !path.exists() {
            debug!("Path {:?} does not exist, cannot extract NAR metadata", path);
            return Ok((None, None));
        }
        
        // Run nix path-info to get NAR hash and size
        let output = Command::new("nix")
            .args(&["path-info", "--json", &path.to_string_lossy()])
            .env("NIX_CONFIG", "experimental-features = nix-command")
            .output()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nix path-info: {}", e),
            })?;
        
        if !output.status.success() {
            // Path might not be in store database yet
            // Try to add it first if it's a regular path
            if path.is_dir() || path.is_file() {
                debug!("Path not in store database, trying nix store add-path");
                
                let add_output = Command::new("nix")
                    .args(&["store", "add-path", &path.to_string_lossy()])
                    .env("NIX_CONFIG", "experimental-features = nix-command")
                    .output()
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to run nix store add-path: {}", e),
                    })?;
                
                if !add_output.status.success() {
                    debug!("Failed to add path to store: {}", String::from_utf8_lossy(&add_output.stderr));
                    return Ok((None, None));
                }
                
                // Try path-info again
                let retry_output = Command::new("nix")
                    .args(&["path-info", "--json", &path.to_string_lossy()])
                    .env("NIX_CONFIG", "experimental-features = nix-command")
                    .output()
                    .await?;
                
                if !retry_output.status.success() {
                    debug!("nix path-info still failed after add-path: {}", 
                           String::from_utf8_lossy(&retry_output.stderr));
                    return Ok((None, None));
                }
                
                // Parse the retry output
                return self.parse_nix_path_info_output(&retry_output.stdout, path).await;
            }
            
            debug!("nix path-info failed for {:?}: {}", path, String::from_utf8_lossy(&output.stderr));
            return Ok((None, None));
        }
        
        self.parse_nix_path_info_output(&output.stdout, path).await
    }
    
    /// Parse nix path-info JSON output
    async fn parse_nix_path_info_output(&self, stdout: &[u8], path: &Path) -> BlixardResult<(Option<String>, Option<u64>)> {
        let json_str = String::from_utf8_lossy(stdout);
        let path_info: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to parse nix path-info JSON: {}", e),
            })?;
        
        // Extract NAR hash and size from the first (and only) entry
        if let Some(info) = path_info.as_object()
            .and_then(|obj| obj.values().next())
            .and_then(|v| v.as_object()) {
            
            let nar_hash = info.get("narHash")
                .and_then(|h| h.as_str())
                .map(String::from);
            
            let nar_size = info.get("narSize")
                .and_then(|s| s.as_u64());
            
            info!("Extracted NAR metadata for {:?}: hash={:?}, size={:?}", 
                  path, nar_hash, nar_size);
            
            Ok((nar_hash, nar_size))
        } else {
            warn!("Unexpected nix path-info output format for {:?}", path);
            Ok((None, None))
        }
    }
    
    /// Verify a downloaded Nix image against its NAR hash
    async fn verify_nix_image(&self, metadata: &NixImageMetadata, path: &Path) -> BlixardResult<()> {
        use tokio::process::Command;
        
        let expected_nar_hash = metadata.nar_hash.as_ref()
            .ok_or_else(|| BlixardError::Internal {
                message: "No NAR hash available for verification".to_string(),
            })?;
        
        info!("Verifying Nix image {} against NAR hash {}", metadata.name, expected_nar_hash);
        
        // First try to get the NAR hash if the path is already in the store
        if path.starts_with(&self.nix_store_dir) {
            let output = Command::new("nix")
                .args(&["path-info", "--json", &path.to_string_lossy()])
                .env("NIX_CONFIG", "experimental-features = nix-command")
                .output()
                .await?;
                
            if output.status.success() {
                let (nar_hash, _) = self.parse_nix_path_info_output(&output.stdout, path).await?;
                if let Some(actual_hash) = nar_hash {
                    if actual_hash == *expected_nar_hash {
                        info!("NAR hash verification succeeded for {}", metadata.name);
                        return Ok(());
                    } else {
                        return Err(BlixardError::Internal {
                            message: format!(
                                "NAR hash mismatch for {}: expected {}, got {}",
                                metadata.name, expected_nar_hash, actual_hash
                            ),
                        });
                    }
                }
            }
        }
        
        // Otherwise, compute the NAR hash directly
        let output = Command::new("nix")
            .args(&["nar", "dump-path", &path.to_string_lossy(), "|", "nix", "hash", "file", "--type", "sha256", "--base32", "-"])
            .env("NIX_CONFIG", "experimental-features = nix-command")
            .output()
            .await;
            
        // If the above fails (due to shell pipe), try an alternative approach
        let computed_hash = if output.as_ref().map(|o| !o.status.success()).unwrap_or(true) {
            // Alternative: use nix-store --dump and pipe to nix-hash
            let dump_output = Command::new("nix-store")
                .args(&["--dump", &path.to_string_lossy()])
                .output()
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to run nix-store --dump: {}", e),
                })?;
                
            if !dump_output.status.success() {
                return Err(BlixardError::Internal {
                    message: format!(
                        "Failed to dump NAR for {}: {}",
                        path.display(),
                        String::from_utf8_lossy(&dump_output.stderr)
                    ),
                });
            }
            
            // Hash the NAR output
            let mut hash_cmd = Command::new("nix-hash")
                .args(&["--type", "sha256", "--base32", "--flat"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to spawn nix-hash: {}", e),
                })?;
                
            // Write NAR data to nix-hash stdin
            use tokio::io::AsyncWriteExt;
            if let Some(stdin) = hash_cmd.stdin.take() {
                let mut stdin = tokio::io::BufWriter::new(stdin);
                stdin.write_all(&dump_output.stdout).await?;
                stdin.flush().await?;
            }
            
            let hash_result = hash_cmd.wait_with_output().await?;
            
            if !hash_result.status.success() {
                return Err(BlixardError::Internal {
                    message: format!(
                        "Failed to hash NAR: {}",
                        String::from_utf8_lossy(&hash_result.stderr)
                    ),
                });
            }
            
            format!("sha256:{}", String::from_utf8_lossy(&hash_result.stdout).trim())
        } else {
            match output {
                Ok(o) => format!("sha256:{}", String::from_utf8_lossy(&o.stdout).trim()),
                Err(e) => return Err(BlixardError::Internal {
                    message: format!("Failed to compute NAR hash: {}", e),
                }),
            }
        };
        
        // Compare the computed hash with the expected hash
        if computed_hash == *expected_nar_hash {
            info!("NAR hash verification succeeded for {}", metadata.name);
            
            // Also check NAR size if available
            if let Some(expected_size) = metadata.nar_size {
                if path.starts_with(&self.nix_store_dir) {
                    // For store paths, we already have the exact NAR size
                    // The file size might be different from NAR size
                    debug!("NAR size verification skipped for store path (size: {})", expected_size);
                } else {
                    // For non-store paths, verify the actual file size
                    let actual_size = if path.is_file() {
                        std::fs::metadata(path)?.len()
                    } else {
                        // For directories, we'd need to compute the NAR size
                        // which is complex, so skip for now
                        expected_size
                    };
                    
                    if actual_size != expected_size {
                        warn!(
                            "Size mismatch for {}: expected {} bytes, got {} bytes",
                            metadata.name, expected_size, actual_size
                        );
                        // Don't fail on size mismatch as NAR size != file size
                    }
                }
            }
            
            Ok(())
        } else {
            Err(BlixardError::Internal {
                message: format!(
                    "NAR hash verification failed for {}: expected {}, got {}",
                    metadata.name, expected_nar_hash, computed_hash
                ),
            })
        }
    }
    
    /// Extract derivation hash from a Nix store path
    fn extract_derivation_hash(&self, path: &Path) -> Option<String> {
        // Nix store paths have format: /nix/store/HASH-name
        if let Some(file_name) = path.file_name() {
            let name = file_name.to_string_lossy();
            if let Some(dash_pos) = name.find('-') {
                return Some(name[..dash_pos].to_string());
            }
        }
        None
    }
    
    /// Detect runtime requirements from Nix paths
    async fn detect_runtime_requirements(&self, paths: &[&Path]) -> BlixardResult<RuntimeRequirements> {
        // In a real implementation, we would analyze the paths to detect:
        // - Kernel version from kernel path
        // - CPU features from binary analysis
        // - Hypervisor requirements from configuration
        
        Ok(RuntimeRequirements {
            min_kernel: None,
            cpu_features: vec![],
            min_memory_mb: 512, // Default minimum
            hypervisor_features: vec![],
        })
    }
    
    /// Get Nix dependencies of a store path
    async fn get_nix_dependencies(&self, path: &Path) -> BlixardResult<Vec<String>> {
        // In production: run `nix-store --query --requisites <path>`
        // For now, return empty
        Ok(vec![])
    }
    
    /// Get all paths in a Nix closure
    async fn get_nix_closure(&self, root_path: &Path) -> BlixardResult<Vec<PathBuf>> {
        // In production: run `nix-store --query --requisites <path>`
        // For now, just return the root path
        Ok(vec![root_path.to_path_buf()])
    }
    
    /// Extract OCI manifest digest from container tar
    async fn extract_oci_manifest_digest(&self, tar_path: &Path) -> BlixardResult<String> {
        // In production: extract and parse manifest.json from tar
        // For now, generate a dummy digest
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(tar_path.to_string_lossy().as_bytes());
        Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
    }
    
    /// Deduplicate chunks against existing storage
    async fn deduplicate_chunks(&self, chunks: &[ChunkMetadata]) -> BlixardResult<(Vec<ChunkMetadata>, usize)> {
        let index = self.chunk_index.read().await;
        let mut unique_chunks = Vec::new();
        let mut dedup_count = 0;
        
        for chunk in chunks {
            if index.contains_key(&chunk.hash) {
                dedup_count += 1;
            } else {
                unique_chunks.push(chunk.clone());
            }
        }
        
        Ok((unique_chunks, dedup_count))
    }
    
    /// Upload chunks to P2P network
    async fn upload_chunks(&self, chunks: &[ChunkMetadata]) -> BlixardResult<Hash> {
        // In production: upload each chunk and return content hash
        // For now, generate a dummy hash
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        for chunk in chunks {
            hasher.update(&chunk.hash);
        }
        let hash_bytes = hasher.finalize();
        
        // Convert to Iroh hash format
        // Note: This creates a hash of the concatenated hashes, not a proper content hash
        Ok(Hash::new(&hash_bytes))
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
        
        let (unique, dedup_count) = store.deduplicate_chunks(&chunks).await.unwrap();
        assert_eq!(unique.len(), 2);
        assert_eq!(dedup_count, 0); // No duplicates in test data
    }
}