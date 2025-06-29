//! Nix-aware VM image service for P2P distribution
//!
//! This service provides specialized support for distributing Nix-built
//! microVM images and containers using content-addressed storage and
//! efficient chunked transfers.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    nix_image_store::{NixImageStore, NixImageMetadata, NixArtifactType, TransferStats},
    proto,
};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tonic::{Request, Response, Status};
use async_trait::async_trait;
use tracing::{info, debug, warn};
use serde::{Serialize, Deserialize};

/// Request to import a Nix-built VM image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportNixImageRequest {
    pub name: String,
    pub artifact_type: NixImageType,
    pub system_path: String,
    pub kernel_path: Option<String>,
    pub derivation_hash: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Response from importing a Nix image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportNixImageResponse {
    pub image_id: String,
    pub content_hash: String,
    pub total_size: u64,
    pub chunk_count: u32,
    pub deduplication_ratio: f32,
}

/// Type of Nix image to import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NixImageType {
    MicroVM { has_kernel: bool },
    Container { manifest_digest: String },
    NixClosure { path_count: usize },
}

/// Service implementation for Nix VM images
pub struct NixVmImageServiceImpl {
    node: Arc<SharedNodeState>,
    image_store: Arc<NixImageStore>,
}

impl NixVmImageServiceImpl {
    /// Create a new Nix VM image service
    pub async fn new(node: Arc<SharedNodeState>) -> BlixardResult<Self> {
        // Get P2P manager
        let p2p_manager = node.get_p2p_manager().await
            .ok_or_else(|| BlixardError::NotInitialized {
                component: "P2P manager".to_string(),
            })?;

        // Get data directory
        let data_dir = PathBuf::from(&node.config.data_dir);
        
        // Create Nix image store
        let image_store = Arc::new(
            NixImageStore::new(
                node.get_id(),
                p2p_manager,
                &data_dir,
                None, // Use default /nix/store
            ).await?
        );

        Ok(Self {
            node,
            image_store,
        })
    }

    /// Import a Nix-built microVM
    pub async fn import_microvm(
        &self,
        name: &str,
        system_path: &Path,
        kernel_path: Option<&Path>,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<ImportNixImageResponse> {
        info!("Importing Nix microVM: {}", name);

        let start = std::time::Instant::now();
        
        // Import the microVM
        let image_metadata = self.image_store.import_microvm(
            name,
            system_path,
            kernel_path,
        ).await?;

        // Calculate deduplication ratio
        let total_chunks = image_metadata.chunk_hashes.len();
        let unique_chunks = image_metadata.chunk_hashes.iter()
            .filter(|c| !c.is_common)
            .count();
        let dedup_ratio = if total_chunks > 0 {
            1.0 - (unique_chunks as f32 / total_chunks as f32)
        } else {
            0.0
        };

        info!(
            "Imported microVM {} in {:?} - {} chunks, {:.1}% deduplicated",
            name,
            start.elapsed(),
            total_chunks,
            dedup_ratio * 100.0
        );

        Ok(ImportNixImageResponse {
            image_id: image_metadata.id,
            content_hash: image_metadata.content_hash,
            total_size: image_metadata.total_size,
            chunk_count: total_chunks as u32,
            deduplication_ratio: dedup_ratio,
        })
    }

    /// Import a container image
    pub async fn import_container(
        &self,
        image_ref: &str,
        tar_path: &Path,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<ImportNixImageResponse> {
        info!("Importing container image: {}", image_ref);

        let start = std::time::Instant::now();
        
        // Import the container
        let image_metadata = self.image_store.import_container(
            image_ref,
            tar_path,
        ).await?;

        // Calculate deduplication ratio
        let total_chunks = image_metadata.chunk_hashes.len();
        let unique_chunks = image_metadata.chunk_hashes.iter()
            .filter(|c| !c.is_common)
            .count();
        let dedup_ratio = if total_chunks > 0 {
            1.0 - (unique_chunks as f32 / total_chunks as f32)
        } else {
            0.0
        };

        info!(
            "Imported container {} in {:?} - {} chunks, {:.1}% deduplicated",
            image_ref,
            start.elapsed(),
            total_chunks,
            dedup_ratio * 100.0
        );

        Ok(ImportNixImageResponse {
            image_id: image_metadata.id,
            content_hash: image_metadata.content_hash,
            total_size: image_metadata.total_size,
            chunk_count: total_chunks as u32,
            deduplication_ratio: dedup_ratio,
        })
    }

    /// Import a Nix closure
    pub async fn import_closure(
        &self,
        name: &str,
        root_path: &Path,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<ImportNixImageResponse> {
        info!("Importing Nix closure: {} from {:?}", name, root_path);

        let start = std::time::Instant::now();
        
        // Import the closure
        let image_metadata = self.image_store.import_closure(
            name,
            root_path,
        ).await?;

        // Calculate deduplication ratio
        let total_chunks = image_metadata.chunk_hashes.len();
        let unique_chunks = image_metadata.chunk_hashes.iter()
            .filter(|c| !c.is_common)
            .count();
        let dedup_ratio = if total_chunks > 0 {
            1.0 - (unique_chunks as f32 / total_chunks as f32)
        } else {
            0.0
        };

        info!(
            "Imported closure {} in {:?} - {} paths, {} chunks, {:.1}% deduplicated",
            name,
            start.elapsed(),
            match &image_metadata.artifact_type {
                NixArtifactType::Closure { path_count, .. } => *path_count,
                _ => 0,
            },
            total_chunks,
            dedup_ratio * 100.0
        );

        Ok(ImportNixImageResponse {
            image_id: image_metadata.id,
            content_hash: image_metadata.content_hash,
            total_size: image_metadata.total_size,
            chunk_count: total_chunks as u32,
            deduplication_ratio: dedup_ratio,
        })
    }

    /// Download an image with progress tracking
    pub async fn download_image(
        &self,
        image_id: &str,
        target_dir: Option<&Path>,
    ) -> BlixardResult<(PathBuf, TransferStats)> {
        info!("Downloading Nix image: {}", image_id);

        let (path, stats) = self.image_store.download_image(
            image_id,
            target_dir,
        ).await?;

        info!(
            "Downloaded {} - transferred {} MB, deduplicated {} MB ({:.1}% savings)",
            image_id,
            stats.bytes_transferred as f64 / 1_048_576.0,
            stats.bytes_deduplicated as f64 / 1_048_576.0,
            (stats.bytes_deduplicated as f64 / (stats.bytes_transferred + stats.bytes_deduplicated) as f64) * 100.0
        );

        Ok((path, stats))
    }

    /// Pre-fetch images for a VM migration
    pub async fn prefetch_for_migration(
        &self,
        vm_name: &str,
        target_node: u64,
    ) -> BlixardResult<()> {
        info!("Pre-fetching images for VM {} migration to node {}", vm_name, target_node);
        
        self.image_store.prefetch_for_migration(vm_name, target_node).await?;
        
        Ok(())
    }

    /// Run garbage collection
    pub async fn garbage_collect(
        &self,
        keep_recent_days: u32,
        keep_min_copies: usize,
    ) -> BlixardResult<u64> {
        let keep_recent = chrono::Duration::days(keep_recent_days as i64);
        self.image_store.garbage_collect(keep_recent, keep_min_copies).await
    }

    /// Get image metadata
    pub async fn get_image_metadata(
        &self,
        image_id: &str,
    ) -> BlixardResult<Option<NixImageMetadata>> {
        // This would need to be added to NixImageStore
        // For now, return None
        Ok(None)
    }

    /// List available images
    pub async fn list_images(
        &self,
        filter: Option<&str>,
    ) -> BlixardResult<Vec<NixImageMetadata>> {
        // This would need to be added to NixImageStore
        // For now, return empty list
        Ok(Vec::new())
    }
}

/// Iroh service implementation for Nix VM images
#[async_trait]
impl crate::transport::iroh_service::IrohService for NixVmImageServiceImpl {
    fn name(&self) -> &'static str {
        "nix-vm-image"
    }
    
    fn methods(&self) -> Vec<&'static str> {
        vec![
            "import_microvm",
            "import_container",
            "import_closure",
            "download_image",
            "prefetch_migration",
            "garbage_collect",
            "get_metadata",
            "list_images",
        ]
    }
    
    async fn handle_call(&self, method: &str, payload: bytes::Bytes) -> BlixardResult<bytes::Bytes> {
        use crate::transport::iroh_protocol::{serialize_payload, deserialize_payload};
        
        match method {
            "import_microvm" => {
                #[derive(Deserialize)]
                struct Request {
                    name: String,
                    system_path: String,
                    kernel_path: Option<String>,
                    metadata: HashMap<String, String>,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let response = self.import_microvm(
                    &req.name,
                    Path::new(&req.system_path),
                    req.kernel_path.as_ref().map(|p| Path::new(p)),
                    req.metadata,
                ).await?;
                
                serialize_payload(&response)
            }
            
            "import_container" => {
                #[derive(Deserialize)]
                struct Request {
                    image_ref: String,
                    tar_path: String,
                    metadata: HashMap<String, String>,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let response = self.import_container(
                    &req.image_ref,
                    Path::new(&req.tar_path),
                    req.metadata,
                ).await?;
                
                serialize_payload(&response)
            }
            
            "import_closure" => {
                #[derive(Deserialize)]
                struct Request {
                    name: String,
                    root_path: String,
                    metadata: HashMap<String, String>,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let response = self.import_closure(
                    &req.name,
                    Path::new(&req.root_path),
                    req.metadata,
                ).await?;
                
                serialize_payload(&response)
            }
            
            "download_image" => {
                #[derive(Deserialize)]
                struct Request {
                    image_id: String,
                    target_dir: Option<String>,
                }
                
                #[derive(Serialize)]
                struct Response {
                    local_path: String,
                    bytes_transferred: u64,
                    bytes_deduplicated: u64,
                    chunks_transferred: usize,
                    chunks_deduplicated: usize,
                    duration_ms: u64,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let (path, stats) = self.download_image(
                    &req.image_id,
                    req.target_dir.as_ref().map(|d| Path::new(d)),
                ).await?;
                
                let response = Response {
                    local_path: path.to_string_lossy().to_string(),
                    bytes_transferred: stats.bytes_transferred,
                    bytes_deduplicated: stats.bytes_deduplicated,
                    chunks_transferred: stats.chunks_transferred,
                    chunks_deduplicated: stats.chunks_deduplicated,
                    duration_ms: stats.duration.as_millis() as u64,
                };
                
                serialize_payload(&response)
            }
            
            "prefetch_migration" => {
                #[derive(Deserialize)]
                struct Request {
                    vm_name: String,
                    target_node: u64,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                self.prefetch_for_migration(&req.vm_name, req.target_node).await?;
                
                serialize_payload(&true)
            }
            
            "garbage_collect" => {
                #[derive(Deserialize)]
                struct Request {
                    keep_recent_days: u32,
                    keep_min_copies: usize,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let bytes_freed = self.garbage_collect(req.keep_recent_days, req.keep_min_copies).await?;
                
                serialize_payload(&bytes_freed)
            }
            
            "get_metadata" => {
                #[derive(Deserialize)]
                struct Request {
                    image_id: String,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let metadata = self.get_image_metadata(&req.image_id).await?;
                
                serialize_payload(&metadata)
            }
            
            "list_images" => {
                #[derive(Deserialize)]
                struct Request {
                    filter: Option<String>,
                }
                
                let req: Request = deserialize_payload(&payload)?;
                let images = self.list_images(req.filter.as_deref()).await?;
                
                serialize_payload(&images)
            }
            
            _ => Err(BlixardError::Internal {
                message: format!("Unknown method: {}", method),
            }),
        }
    }
}

/// Client for Nix VM image service over Iroh
pub struct NixVmImageClient<'a> {
    client: &'a crate::transport::iroh_service::IrohRpcClient,
    node_addr: iroh::NodeAddr,
}

impl<'a> NixVmImageClient<'a> {
    /// Create a new client
    pub fn new(
        client: &'a crate::transport::iroh_service::IrohRpcClient,
        node_addr: iroh::NodeAddr,
    ) -> Self {
        Self { client, node_addr }
    }
    
    /// Import a microVM
    pub async fn import_microvm(
        &self,
        name: String,
        system_path: String,
        kernel_path: Option<String>,
        metadata: HashMap<String, String>,
    ) -> BlixardResult<ImportNixImageResponse> {
        #[derive(Serialize)]
        struct Request {
            name: String,
            system_path: String,
            kernel_path: Option<String>,
            metadata: HashMap<String, String>,
        }
        
        let request = Request {
            name,
            system_path,
            kernel_path,
            metadata,
        };
        
        self.client
            .call(self.node_addr.clone(), "nix-vm-image", "import_microvm", request)
            .await
    }
    
    /// Download an image
    pub async fn download_image(
        &self,
        image_id: String,
        target_dir: Option<String>,
    ) -> BlixardResult<(String, u64, u64, u64)> {
        #[derive(Serialize)]
        struct Request {
            image_id: String,
            target_dir: Option<String>,
        }
        
        #[derive(Deserialize)]
        struct Response {
            local_path: String,
            bytes_transferred: u64,
            bytes_deduplicated: u64,
            duration_ms: u64,
        }
        
        let request = Request {
            image_id,
            target_dir,
        };
        
        let resp: Response = self.client
            .call(self.node_addr.clone(), "nix-vm-image", "download_image", request)
            .await?;
            
        Ok((resp.local_path, resp.bytes_transferred, resp.bytes_deduplicated, resp.duration_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::types::NodeConfig;

    #[tokio::test]
    async fn test_nix_vm_image_service() {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeConfig {
            id: 1,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
        };
        
        let node = Arc::new(SharedNodeState::new(config));
        
        // Service creation will fail without P2P manager
        let result = NixVmImageServiceImpl::new(node).await;
        assert!(result.is_err());
    }
}