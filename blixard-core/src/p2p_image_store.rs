//! P2P VM Image Store using Iroh
//!
//! This module provides a distributed VM image store that uses Iroh for
//! efficient peer-to-peer sharing of VM images across the cluster.

use crate::error::{BlixardError, BlixardResult};
use crate::iroh_transport_v2::{DocumentType, IrohTransportV2};
use chrono::{DateTime, Utc};
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Metadata for a VM image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmImageMetadata {
    /// Unique name of the image
    pub name: String,
    /// Version of the image
    pub version: String,
    /// Description of the image
    pub description: String,
    /// Size in bytes
    pub size: u64,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,
    /// Iroh hash of the image file
    pub content_hash: String,
    /// OS type (linux, windows, etc.)
    pub os_type: String,
    /// Architecture (amd64, arm64, etc.)
    pub architecture: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Node that originally uploaded this image
    pub uploaded_by_node: u64,
}

/// P2P VM image store
pub struct P2pImageStore {
    /// Iroh transport layer
    transport: IrohTransportV2,
    /// Local cache directory
    cache_dir: PathBuf,
    /// Node ID
    node_id: u64,
}

impl P2pImageStore {
    /// Create a new P2P image store
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        let cache_dir = data_dir.join("image-cache");
        std::fs::create_dir_all(&cache_dir)?;

        let transport = IrohTransportV2::new(node_id, data_dir).await?;

        // Create or join the VM images document
        // Now we can enable this since IrohTransportV2 has working document operations!
        transport
            .create_or_join_doc(DocumentType::VmImages, true)
            .await?;

        Ok(Self {
            transport,
            cache_dir,
            node_id,
        })
    }

    /// Upload a VM image to the P2P network
    pub async fn upload_image(
        &self,
        name: &str,
        version: &str,
        description: &str,
        image_path: &Path,
        os_type: &str,
        architecture: &str,
        tags: Vec<String>,
    ) -> BlixardResult<VmImageMetadata> {
        info!(
            "Uploading VM image {} v{} from {:?}",
            name, version, image_path
        );

        // Get file size
        let file_metadata = std::fs::metadata(image_path)?;
        let size = file_metadata.len();

        // Share the file via Iroh
        let content_hash = self.transport.share_file(image_path).await?;

        // Create metadata
        let metadata = VmImageMetadata {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            size,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            content_hash: content_hash.to_string(),
            os_type: os_type.to_string(),
            architecture: architecture.to_string(),
            tags,
            uploaded_by_node: self.node_id,
        };

        // Store metadata in the document
        let key = format!("image:{}:{}", name, version);
        let metadata_json = serde_json::to_vec(&metadata)?;
        self.transport
            .write_to_doc(DocumentType::VmImages, &key, metadata_json.as_slice())
            .await?;

        info!(
            "VM image {} v{} uploaded successfully with hash {}",
            name, version, content_hash
        );
        Ok(metadata)
    }

    /// List all available VM images
    pub async fn list_images(&self) -> BlixardResult<Vec<VmImageMetadata>> {
        let images = Vec::new();

        // In a real implementation, we would iterate through all entries in the document
        // For now, we'll return an empty list
        // TODO: Implement proper document iteration once Iroh provides the API

        Ok(images)
    }

    /// Get metadata for a specific image
    pub async fn get_image_metadata(
        &self,
        name: &str,
        version: &str,
    ) -> BlixardResult<Option<VmImageMetadata>> {
        let key = format!("image:{}:{}", name, version);

        match self
            .transport
            .read_from_doc(DocumentType::VmImages, &key)
            .await
        {
            Ok(data) => {
                let metadata: VmImageMetadata = serde_json::from_slice(&data)?;
                Ok(Some(metadata))
            }
            Err(_) => Ok(None),
        }
    }

    /// Download a VM image from the P2P network
    pub async fn download_image(&self, name: &str, version: &str) -> BlixardResult<PathBuf> {
        info!("Downloading VM image {} v{}", name, version);

        // Get metadata
        let metadata = self
            .get_image_metadata(name, version)
            .await?
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("VM image {}:{}", name, version),
            })?;

        // Parse hash using FromStr trait
        use std::str::FromStr;
        let hash = Hash::from_str(&metadata.content_hash).map_err(|e| BlixardError::Internal {
            message: format!("Invalid hash format: {}", e),
        })?;

        // Determine output path
        let output_path = self.cache_dir.join(format!("{}-{}.img", name, version));

        // Check if already cached
        if output_path.exists() {
            info!("Image {} v{} already cached", name, version);
            return Ok(output_path);
        }

        // Download the image
        self.transport.download_file(hash, &output_path).await?;

        info!(
            "VM image {} v{} downloaded to {:?}",
            name, version, output_path
        );
        Ok(output_path)
    }

    /// Check if an image is cached locally
    pub fn is_cached(&self, name: &str, version: &str) -> bool {
        let cache_path = self.cache_dir.join(format!("{}-{}.img", name, version));
        cache_path.exists()
    }

    /// Clear the local cache
    pub async fn clear_cache(&self) -> BlixardResult<()> {
        info!("Clearing image cache");

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |ext| ext == "img") {
                std::fs::remove_file(entry.path())?;
            }
        }

        Ok(())
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> BlixardResult<CacheStats> {
        let mut total_size = 0u64;
        let mut image_count = 0u32;

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |ext| ext == "img") {
                let metadata = entry.metadata()?;
                total_size += metadata.len();
                image_count += 1;
            }
        }

        Ok(CacheStats {
            total_size,
            image_count,
            cache_dir: self.cache_dir.clone(),
        })
    }

    /// Subscribe to new image announcements
    pub async fn subscribe_to_images(
        &self,
    ) -> BlixardResult<impl futures::Stream<Item = VmImageMetadata> + '_> {
        use futures::stream;

        // Return empty stream for now (stub implementation)
        Ok(stream::empty())
    }

    /// Get the Iroh node address for sharing
    pub async fn get_node_addr(&self) -> BlixardResult<iroh::NodeAddr> {
        self.transport.node_addr().await
    }

    /// Send image metadata to a peer
    pub async fn send_image_metadata(
        &self,
        peer_addr: &iroh::NodeAddr,
        metadata: &VmImageMetadata,
    ) -> BlixardResult<()> {
        let data = serde_json::to_vec(metadata).map_err(|e| BlixardError::JsonError(e))?;
        self.transport
            .send_to_peer(peer_addr, DocumentType::VmImages, &data)
            .await
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total size of cached images in bytes
    pub total_size: u64,
    /// Number of cached images
    pub image_count: u32,
    /// Cache directory path
    pub cache_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_image_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = P2pImageStore::new(1, temp_dir.path()).await.unwrap();

        // Check cache directory was created
        assert!(temp_dir.path().join("image-cache").exists());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let store = P2pImageStore::new(1, temp_dir.path()).await.unwrap();

        let stats = store.get_cache_stats().unwrap();
        assert_eq!(stats.image_count, 0);
        assert_eq!(stats.total_size, 0);
    }
}
