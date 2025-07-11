//! VM image service implementation for P2P image sharing
//!
//! This service handles VM image sharing using Iroh's blob transfer capabilities.

use crate::{
    error::{BlixardError, BlixardResult},
    iroh_types::{
    },
    node_shared::SharedNodeState,
    p2p_manager::ResourceType,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
// Removed tonic imports - using Iroh transport
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;

/// VM image operation types for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmImageOperation {
    Share {
        image_name: String,
        image_path: String,
        version: String,
        metadata: HashMap<String, String>,
    },
    Get {
        image_name: String,
        version: String,
        image_hash: String,
    },
    List,
}

/// VM image operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmImageResponse {
    Share {
        success: bool,
        message: String,
        image_hash: String,
    },
    Get {
        success: bool,
        message: String,
        local_path: String,
        metadata: HashMap<String, String>,
    },
    List {
        images: Vec<ImageInfo>,
    },
}

/// Image information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub name: String,
    pub version: String,
    pub hash: String,
    pub size_bytes: u64,
    pub created_at: String,
    pub metadata: HashMap<String, String>,
    pub is_cached: bool,
}

/// Trait for VM image operations
#[async_trait]
pub trait VmImageService: Send + Sync {
    /// Share a VM image to the P2P network
    async fn share_image(
        &self,
        image_name: &str,
        image_path: &str,
        version: &str,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<String>;

    /// Get a VM image from the P2P network
    async fn get_image(
        &self,
        image_name: &str,
        version: &str,
        image_hash: &str,
    ) -> BlixardResult<(String, HashMap<String, String>)>;

    /// List available P2P images
    async fn list_images(&self) -> BlixardResult<Vec<ImageInfo>>;
}

/// VM image service implementation
#[derive(Clone)]
pub struct VmImageServiceImpl {
    node: Arc<SharedNodeState>,
}

impl VmImageServiceImpl {
    /// Create a new VM image service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }

    /// Calculate SHA256 hash of a file
    async fn calculate_file_hash(path: &Path) -> BlixardResult<String> {
        let mut file = fs::File::open(path)
            .await
            .map_err(|e| BlixardError::IoError(e))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8192];

        loop {
            use tokio::io::AsyncReadExt;
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| BlixardError::IoError(e))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Get file size
    async fn get_file_size(path: &Path) -> BlixardResult<u64> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| BlixardError::IoError(e))?;
        Ok(metadata.len())
    }
}

#[async_trait]
impl VmImageService for VmImageServiceImpl {
    async fn share_image(
        &self,
        image_name: &str,
        image_path: &str,
        version: &str,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<String> {
        // Get P2P manager
        let p2p_manager =
            self.node
                .get_p2p_manager()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "P2P manager not available".to_string(),
                })?;

        // Calculate image hash
        let path = Path::new(image_path);
        let hash = Self::calculate_file_hash(path).await?;
        let size = Self::get_file_size(path).await?;

        // Upload the image to P2P network
        p2p_manager
            .upload_resource(ResourceType::VmImage, image_name, version, path)
            .await?;

        // TODO: Store metadata in image store when implemented
        // if let Some(image_store) = p2p_manager.get_image_store() {
        //     let mut image_metadata = metadata.clone();
        //     image_metadata.insert("original_path".to_string(), image_path.to_string());
        //     image_metadata.insert("size_bytes".to_string(), size.to_string());
        //
        //     image_store.add_image(
        //         image_name,
        //         version,
        //         &hash,
        //         path,
        //         image_metadata,
        //     ).await?;
        // }

        Ok(hash)
    }

    async fn get_image(
        &self,
        image_name: &str,
        version: &str,
        image_hash: &str,
    ) -> BlixardResult<(String, HashMap<String, String>)> {
        // Get P2P manager
        let p2p_manager =
            self.node
                .get_p2p_manager()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "P2P manager not available".to_string(),
                })?;

        // TODO: Check if image is already cached when image store is available
        // if let Some(image_store) = p2p_manager.get_image_store() {
        //     if let Ok(Some(image)) = image_store.get_image_by_hash(image_hash).await {
        //         return Ok((image.local_path.to_string_lossy().to_string(), image.metadata));
        //     }
        // }

        // Request download from P2P network
        let request_id = p2p_manager
            .request_download(
                ResourceType::VmImage,
                image_name,
                version,
                crate::p2p_manager::TransferPriority::Normal,
            )
            .await?;

        // Wait for download to complete (simplified - in production would use events)
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // TODO: Check if download completed when image store is available
        // if let Some(image_store) = p2p_manager.get_image_store() {
        //     if let Ok(Some(image)) = image_store.get_image(image_name, version).await {
        //         return Ok((image.local_path.to_string_lossy().to_string(), image.metadata));
        //     }
        // }

        Err(BlixardError::NotFound {
            resource: format!("VM image {}:{}", image_name, version),
        })
    }

    async fn list_images(&self) -> BlixardResult<Vec<ImageInfo>> {
        // Get P2P manager
        let p2p_manager =
            self.node
                .get_p2p_manager()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "P2P manager not available".to_string(),
                })?;

        // Get image store
        // TODO: Get image store when available
        // let image_store = p2p_manager.get_image_store()
        //     .ok_or_else(|| BlixardError::Internal {
        //         message: "Image store not available".to_string(),
        //     })?;

        // TODO: List all images when image store is available
        // let images = image_store.list_images().await?;
        //
        // // Convert to ImageInfo
        // let image_infos = images.into_iter().map(|img| {
        //     ImageInfo {
        //         name: img.name,
        //         version: img.version,
        //         hash: img.hash,
        //         size_bytes: img.size,
        //         created_at: img.created_at.to_rfc3339(),
        //         metadata: img.metadata,
        //         is_cached: true, // All listed images are cached locally
        //     }
        // }).collect();
        //
        // Ok(image_infos)

        // Return empty list for now
        Ok(Vec::new())
    }
}

/// Iroh protocol handler for VM image service
pub struct VmImageProtocolHandler {
    service: VmImageServiceImpl,
}

impl VmImageProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: VmImageServiceImpl::new(node),
        }
    }

    /// Handle a VM image operation over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        operation: VmImageOperation,
    ) -> BlixardResult<VmImageResponse> {
        match operation {
            VmImageOperation::Share {
                image_name,
                image_path,
                version,
                metadata,
            } => {
                match self
                    .service
                    .share_image(&image_name, &image_path, &version, metadata)
                    .await
                {
                    Ok(hash) => Ok(VmImageResponse::Share {
                        success: true,
                        message: format!("Image '{}' shared successfully", image_name),
                        image_hash: hash,
                    }),
                    Err(e) => Ok(VmImageResponse::Share {
                        success: false,
                        message: e.to_string(),
                        image_hash: String::new(),
                    }),
                }
            }
            VmImageOperation::Get {
                image_name,
                version,
                image_hash,
            } => {
                match self
                    .service
                    .get_image(&image_name, &version, &image_hash)
                    .await
                {
                    Ok((local_path, metadata)) => Ok(VmImageResponse::Get {
                        success: true,
                        message: format!("Image '{}' retrieved successfully", image_name),
                        local_path,
                        metadata,
                    }),
                    Err(e) => Ok(VmImageResponse::Get {
                        success: false,
                        message: e.to_string(),
                        local_path: String::new(),
                        metadata: HashMap::new(),
                    }),
                }
            }
            VmImageOperation::List => match self.service.list_images().await {
                Ok(images) => Ok(VmImageResponse::List { images }),
                Err(e) => Err(e),
            },
        }
    }
}
