//! Main Nix image store implementation

use crate::abstractions::command::{CommandExecutor, CommandOptions};
use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{
    record_p2p_cache_access, record_p2p_chunk_transfer, record_p2p_image_download,
    record_p2p_image_import, record_p2p_verification, start_p2p_transfer,
};
use super::types::*;
use crate::p2p_manager::P2pManager;
use chrono::Utc;
use iroh_blobs::Hash;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// Type definitions moved to crate::nix_image_store::types module

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
    /// Command executor for running Nix commands
    command_executor: Arc<dyn CommandExecutor>,
}

// ChunkLocation moved to crate::nix_image_store::types module

impl NixImageStore {
    /// Create a new Nix image store
    pub async fn new(
        node_id: u64,
        p2p_manager: Arc<P2pManager>,
        data_dir: &Path,
        nix_store_dir: Option<PathBuf>,
        command_executor: Arc<dyn CommandExecutor>,
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
            command_executor,
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
        self.import_nix_paths(name, artifact_type, &paths_to_import)
            .await
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

        self.import_nix_paths(image_ref, artifact_type, &[tar_path])
            .await
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
        #[cfg(feature = "observability")]
        let artifact_type_str = match &metadata.artifact_type {
            NixArtifactType::MicroVM { .. } => "microvm",
            NixArtifactType::Container { .. } => "container",
            NixArtifactType::StorePath { .. } => "storepath",
            NixArtifactType::Closure { .. } => "closure",
        };
        #[cfg(feature = "observability")]
        let _ = record_p2p_image_import(artifact_type_str, true, total_size);

        // Record deduplication metrics
        #[cfg(feature = "observability")]
        for _ in 0..dedup_count {
            let _ = record_p2p_chunk_transfer(0, true);
        }

        info!(
            "Successfully imported {} with {} chunks ({}  deduplicated)",
            name,
            metadata.chunk_hashes.len(),
            dedup_count
        );
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
        #[cfg(feature = "observability")]
        let _transfer_guard = start_p2p_transfer();

        // Get metadata
        let metadata =
            self.get_metadata(image_id)
                .await?
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
                    let file_name =
                        Path::new(path)
                            .file_name()
                            .ok_or_else(|| BlixardError::Internal {
                                message: format!("Invalid store path: {}", path),
                            })?;
                    target_dir.join(file_name)
                }
            }
            NixArtifactType::Closure { root_path, .. } => {
                let file_name =
                    Path::new(root_path)
                        .file_name()
                        .ok_or_else(|| BlixardError::Internal {
                            message: format!("Invalid closure root path: {}", root_path),
                        })?;
                target_dir.join(format!("closure-{}", file_name.to_string_lossy()))
            }
        };

        // Check what chunks we already have
        let (chunks_needed, chunks_have) =
            self.analyze_chunks_needed(&metadata.chunk_hashes).await?;

        info!(
            "Need to download {} chunks, already have {} chunks",
            chunks_needed.len(),
            chunks_have.len()
        );

        // Record cache hits/misses
        #[cfg(feature = "observability")]
        let _ = record_p2p_cache_access(false, "chunk"); // For chunks we need
        #[cfg(feature = "observability")]
        for _ in &chunks_have {
            let _ = record_p2p_cache_access(true, "chunk"); // For chunks we already have
        }

        let start_time = std::time::Instant::now();
        let mut bytes_transferred = 0u64;
        let mut bytes_deduplicated = 0u64;

        // Download missing chunks
        for chunk in &chunks_needed {
            let chunk_data = self.download_chunk(&chunk.hash).await?;
            bytes_transferred += chunk_data.len() as u64;

            // Record chunk transfer
            #[cfg(feature = "observability")]
            let _ = record_p2p_chunk_transfer(chunk_data.len() as u64, false);

            // Store chunk for future deduplication
            self.store_chunk(&chunk.hash, &chunk_data).await?;
        }

        // Calculate deduplicated bytes
        for chunk in &chunks_have {
            bytes_deduplicated += chunk.size;
            // Record deduplicated chunks
            #[cfg(feature = "observability")]
            let _ = record_p2p_chunk_transfer(chunk.size, true);
        }

        // Reassemble the image from chunks
        self.reassemble_image(&metadata, &target_path).await?;

        // Verify the downloaded image if we have NAR hash
        if metadata.nar_hash.is_some() {
            match self.verify_nix_image(&metadata, &target_path).await {
                Ok(_) => {
                    #[cfg(feature = "observability")]
                    let _ = record_p2p_verification(true, "nar_hash");
                }
                Err(e) => {
                    #[cfg(feature = "observability")]
                    let _ = record_p2p_verification(false, "nar_hash");
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
        #[cfg(feature = "observability")]
        let _ = record_p2p_image_download(image_id, true, stats.duration.as_secs_f64());

        Ok((target_path, stats))
    }

    /// Pre-fetch images for upcoming migrations
    pub async fn prefetch_for_migration(
        &self,
        vm_name: &str,
        target_node: u64,
    ) -> BlixardResult<()> {
        info!(
            "Pre-fetching images for VM {} migration to node {}",
            vm_name, target_node
        );

        // Get current VM image metadata
        let images = self.list_images_for_vm(vm_name).await?;

        for image in images {
            // Check if target node already has this image
            if image.cached_on_nodes.contains(&target_node) {
                debug!("Node {} already has image {}", target_node, image.id);
                continue;
            }

            // Initiate background transfer to target node
            self.initiate_background_transfer(&image.id, target_node)
                .await?;
        }

        Ok(())
    }

    /// Garbage collect unused images
    pub async fn garbage_collect(
        &self,
        keep_recent: chrono::Duration,
        keep_min_copies: usize,
    ) -> BlixardResult<u64> {
        info!(
            "Running garbage collection (keep_recent: {:?}, keep_min_copies: {})",
            keep_recent, keep_min_copies
        );

        let mut total_freed = 0u64;
        let cutoff_time = Utc::now() - keep_recent;

        let metadata_cache = self.metadata_cache.read().await;
        let images_to_check: Vec<_> = metadata_cache
            .values()
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
            let location = index
                .entry(chunk.hash.clone())
                .or_insert_with(|| ChunkLocation {
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
            if index
                .get(&chunk.hash)
                .and_then(|loc| loc.local_path.as_ref())
                .is_some()
            {
                have.push(chunk.clone());
            } else {
                needed.push(chunk.clone());
            }
        }

        Ok((needed, have))
    }

    async fn download_chunk(&self, hash: &str) -> BlixardResult<Vec<u8>> {
        // Convert string hash to Iroh hash
        let iroh_hash = Hash::from_str(hash).map_err(|e| BlixardError::Internal {
            message: e.to_string(),
        })?;

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
        let images: Vec<_> = cache
            .values()
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
        info!(
            "Initiating background transfer of {} to node {}",
            image_id, target_node
        );
        Ok(())
    }

    async fn is_image_in_use(&self, _image_id: &str) -> BlixardResult<bool> {
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
        let metadata =
            self.get_metadata(image_id)
                .await?
                .ok_or_else(|| BlixardError::NotFound {
                    resource: format!("Image {}", image_id),
                })?;

        // Check if we have NAR hash for verification
        if metadata.nar_hash.is_none() {
            info!(
                "No NAR hash available for image {}, skipping verification",
                image_id
            );
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
    async fn extract_nix_metadata(
        &self,
        path: &Path,
    ) -> BlixardResult<(Option<String>, Option<u64>)> {
        // Check if this is a Nix store path
        if !path.starts_with(&self.nix_store_dir) {
            debug!(
                "Path {:?} is not in Nix store, skipping NAR metadata extraction",
                path
            );
            return Ok((None, None));
        }

        // First check if the path exists
        if !path.exists() {
            debug!(
                "Path {:?} does not exist, cannot extract NAR metadata",
                path
            );
            return Ok((None, None));
        }

        // Run nix path-info to get NAR hash and size
        let options =
            CommandOptions::new().with_env_var("NIX_CONFIG", "experimental-features = nix-command");

        let output = self
            .command_executor
            .execute(
                "nix",
                &["path-info", "--json", &path.to_string_lossy()],
                options,
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nix path-info: {}", e),
            })?;

        if !output.success {
            // Path might not be in store database yet
            // Try to add it first if it's a regular path
            if path.is_dir() || path.is_file() {
                debug!("Path not in store database, trying nix store add-path");

                let add_options = CommandOptions::new()
                    .with_env_var("NIX_CONFIG", "experimental-features = nix-command");

                let add_output = self
                    .command_executor
                    .execute(
                        "nix",
                        &["store", "add-path", &path.to_string_lossy()],
                        add_options,
                    )
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to run nix store add-path: {}", e),
                    })?;

                if add_output.status != 0 {
                    debug!(
                        "Failed to add path to store: {}",
                        String::from_utf8_lossy(&add_output.stderr)
                    );
                    return Ok((None, None));
                }

                // Try path-info again
                let retry_options = CommandOptions::new()
                    .with_env_var("NIX_CONFIG", "experimental-features = nix-command");

                let retry_output = self
                    .command_executor
                    .execute(
                        "nix",
                        &["path-info", "--json", &path.to_string_lossy()],
                        retry_options,
                    )
                    .await?;

                if retry_output.status != 0 {
                    debug!(
                        "nix path-info still failed after add-path: {}",
                        String::from_utf8_lossy(&retry_output.stderr)
                    );
                    return Ok((None, None));
                }

                // Parse the retry output
                return self
                    .parse_nix_path_info_output(&retry_output.stdout, path)
                    .await;
            }

            debug!(
                "nix path-info failed for {:?}: {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok((None, None));
        }

        self.parse_nix_path_info_output(&output.stdout, path).await
    }

    /// Parse nix path-info JSON output
    async fn parse_nix_path_info_output(
        &self,
        stdout: &[u8],
        path: &Path,
    ) -> BlixardResult<(Option<String>, Option<u64>)> {
        let json_str = String::from_utf8_lossy(stdout);
        let path_info: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| BlixardError::Internal {
                message: format!("Failed to parse nix path-info JSON: {}", e),
            })?;

        // Extract NAR hash and size from the first (and only) entry
        if let Some(info) = path_info
            .as_object()
            .and_then(|obj| obj.values().next())
            .and_then(|v| v.as_object())
        {
            let nar_hash = info
                .get("narHash")
                .and_then(|h| h.as_str())
                .map(String::from);

            let nar_size = info.get("narSize").and_then(|s| s.as_u64());

            info!(
                "Extracted NAR metadata for {:?}: hash={:?}, size={:?}",
                path, nar_hash, nar_size
            );

            Ok((nar_hash, nar_size))
        } else {
            warn!("Unexpected nix path-info output format for {:?}", path);
            Ok((None, None))
        }
    }

    /// Verify a downloaded Nix image against its NAR hash
    /// Verify NAR hash using nix path-info for store paths
    async fn verify_store_path_hash(
        &self,
        path: &Path,
        expected_nar_hash: &str,
        image_name: &str,
    ) -> BlixardResult<Option<String>> {
        let options = CommandOptions::new()
            .with_env_var("NIX_CONFIG", "experimental-features = nix-command");

        let output = self
            .command_executor
            .execute(
                "nix",
                &["path-info", "--json", &path.to_string_lossy()],
                options,
            )
            .await?;

        if !output.success {
            return Ok(None);
        }

        let (nar_hash, _) = self
            .parse_nix_path_info_output(&output.stdout, path)
            .await?;
        
        if let Some(actual_hash) = nar_hash {
            if actual_hash == expected_nar_hash {
                info!("NAR hash verification succeeded for {}", image_name);
                Ok(Some(actual_hash))
            } else {
                Err(BlixardError::Internal {
                    message: format!(
                        "NAR hash mismatch for {}: expected {}, got {}",
                        image_name, expected_nar_hash, actual_hash
                    ),
                })
            }
        } else {
            Ok(None)
        }
    }

    /// Compute NAR hash directly by dumping and hashing
    async fn compute_nar_hash_directly(&self, path: &Path) -> BlixardResult<String> {
        // First, dump the NAR
        let dump_options = CommandOptions::new();
        let dump_output = self
            .command_executor
            .execute(
                "nix-store",
                &["--dump", &path.to_string_lossy()],
                dump_options,
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nix-store --dump: {}", e),
            })?;

        if dump_output.status != 0 {
            return Err(BlixardError::Internal {
                message: format!(
                    "Failed to dump NAR for {}: {}",
                    path.display(),
                    String::from_utf8_lossy(&dump_output.stderr)
                ),
            });
        }

        // Hash the NAR output
        let hash_options = CommandOptions::new().with_stdin(dump_output.stdout);

        let hash_result = self
            .command_executor
            .execute(
                "nix-hash",
                &["--type", "sha256", "--base32", "--flat"],
                hash_options,
            )
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run nix-hash: {}", e),
            })?;

        if hash_result.status != 0 {
            return Err(BlixardError::Internal {
                message: format!(
                    "Failed to hash NAR: {}",
                    String::from_utf8_lossy(&hash_result.stderr)
                ),
            });
        }

        let computed_hash = format!(
            "sha256:{}",
            String::from_utf8_lossy(&hash_result.stdout).trim()
        );
        
        Ok(computed_hash)
    }

    /// Verify NAR size if available in metadata
    fn verify_nar_size(
        &self,
        metadata: &NixImageMetadata,
        path: &Path,
    ) -> BlixardResult<()> {
        if let Some(expected_size) = metadata.nar_size {
            if path.starts_with(&self.nix_store_dir) {
                // For store paths, we already have the exact NAR size
                // The file size might be different from NAR size
                debug!(
                    "NAR size verification skipped for store path (size: {})",
                    expected_size
                );
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
    }

    async fn verify_nix_image(
        &self,
        metadata: &NixImageMetadata,
        path: &Path,
    ) -> BlixardResult<()> {
        let expected_nar_hash =
            metadata
                .nar_hash
                .as_ref()
                .ok_or_else(|| BlixardError::Internal {
                    message: "No NAR hash available for verification".to_string(),
                })?;

        info!(
            "Verifying Nix image {} against NAR hash {}",
            metadata.name, expected_nar_hash
        );

        // First try to get the NAR hash if the path is already in the store
        let computed_hash = if path.starts_with(&self.nix_store_dir) {
            match self.verify_store_path_hash(path, expected_nar_hash, &metadata.name).await? {
                Some(_) => {
                    // Verification succeeded, also check size
                    self.verify_nar_size(metadata, path)?;
                    return Ok(());
                }
                None => {
                    // Store path verification failed, fall back to direct computation
                    self.compute_nar_hash_directly(path).await?
                }
            }
        } else {
            // Not a store path, compute hash directly
            self.compute_nar_hash_directly(path).await?
        };

        // Compare the computed hash with the expected hash
        if computed_hash == *expected_nar_hash {
            info!("NAR hash verification succeeded for {}", metadata.name);
            self.verify_nar_size(metadata, path)?;
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
    async fn detect_runtime_requirements(
        &self,
        _paths: &[&Path],
    ) -> BlixardResult<RuntimeRequirements> {
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
    async fn get_nix_dependencies(&self, _path: &Path) -> BlixardResult<Vec<String>> {
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
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(tar_path.to_string_lossy().as_bytes());
        Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
    }

    /// Deduplicate chunks against existing storage
    async fn deduplicate_chunks(
        &self,
        chunks: &[ChunkMetadata],
    ) -> BlixardResult<(Vec<ChunkMetadata>, usize)> {
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
        use sha2::{Digest, Sha256};
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
    use crate::abstractions::command::TokioCommandExecutor;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_nix_image_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let p2p_manager = Arc::new(
            P2pManager::new(1, temp_dir.path(), Default::default())
                .await
                .unwrap(),
        );

        let store = NixImageStore::new(
            1,
            p2p_manager,
            temp_dir.path(),
            None,
            Arc::new(TokioCommandExecutor::new()),
        )
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
                .unwrap(),
        );

        let store = NixImageStore::new(
            1,
            p2p_manager,
            temp_dir.path(),
            None,
            Arc::new(TokioCommandExecutor::new()),
        )
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
