//! VM image service implementation for P2P image sharing
//!
//! This service handles VM image sharing using Iroh's blob transfer capabilities.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    p2p_manager::{P2pManager, ResourceType},
    iroh_types::{
        ShareVmImageRequest, ShareVmImageResponse,
        GetVmImageRequest, GetVmImageResponse,
        ListP2pImagesRequest, ListP2pImagesResponse,
        P2pImageInfo,
    },
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
// Removed tonic imports - using Iroh transport
use serde::{Serialize, Deserialize};
use tokio::fs;
use sha2::{Sha256, Digest};

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
        let mut file = fs::File::open(path).await
            .map_err(|e| BlixardError::IoError(e))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8192];
        
        loop {
            use tokio::io::AsyncReadExt;
            let bytes_read = file.read(&mut buffer).await
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
        let metadata = fs::metadata(path).await
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
        let p2p_manager = self.node.get_p2p_manager().await
            .ok_or_else(|| BlixardError::Internal {
                message: "P2P manager not available".to_string(),
            })?;
        
        // Calculate image hash
        let path = Path::new(image_path);
        let hash = Self::calculate_file_hash(path).await?;
        let size = Self::get_file_size(path).await?;
        
        // Upload the image to P2P network
        p2p_manager.upload_resource(
            ResourceType::VmImage,
            image_name,
            version,
            path,
        ).await?;
        
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
        let p2p_manager = self.node.get_p2p_manager().await
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
        let request_id = p2p_manager.request_download(
            ResourceType::VmImage,
            image_name,
            version,
            crate::p2p_manager::TransferPriority::Normal,
        ).await?;
        
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
        let p2p_manager = self.node.get_p2p_manager().await
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

/// gRPC adapter for VM image service
#[async_trait]
impl crate::iroh_types::cluster_service_server::ClusterService for VmImageServiceImpl {
    async fn share_vm_image(
        &self,
        request: Request<ShareVmImageRequest>,
    ) -> Result<Response<ShareVmImageResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("share_vm_image"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("share_vm_image")]);
        
        let req = request.into_inner();
        
        match self.share_image(&req.image_name, &req.image_path, &req.version, req.metadata).await {
            Ok(hash) => Ok(Response::new(ShareVmImageResponse {
                success: true,
                message: format!("Image '{}' shared successfully", req.image_name),
                image_hash: hash,
            })),
            Err(e) => Ok(Response::new(ShareVmImageResponse {
                success: false,
                message: e.to_string(),
                image_hash: String::new(),
            })),
        }
    }
    
    async fn get_vm_image(
        &self,
        request: Request<GetVmImageRequest>,
    ) -> Result<Response<GetVmImageResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("get_vm_image"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("get_vm_image")]);
        
        let req = request.into_inner();
        
        match self.get_image(&req.image_name, &req.version, &req.image_hash).await {
            Ok((local_path, metadata)) => Ok(Response::new(GetVmImageResponse {
                success: true,
                message: format!("Image '{}' retrieved successfully", req.image_name),
                local_path,
                metadata,
            })),
            Err(e) => Ok(Response::new(GetVmImageResponse {
                success: false,
                message: e.to_string(),
                local_path: String::new(),
                metadata: HashMap::new(),
            })),
        }
    }
    
    async fn list_p2p_images(
        &self,
        _request: Request<ListP2pImagesRequest>,
    ) -> Result<Response<ListP2pImagesResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("list_p2p_images"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("list_p2p_images")]);
        
        match self.list_images().await {
            Ok(images) => {
                let p2p_images = images.into_iter().map(|img| {
                    P2pImageInfo {
                        name: img.name,
                        version: img.version,
                        hash: img.hash,
                        size_bytes: img.size_bytes,
                        created_at: img.created_at,
                        metadata: img.metadata,
                        is_cached: img.is_cached,
                    }
                }).collect();
                
                Ok(Response::new(ListP2pImagesResponse {
                    images: p2p_images,
                }))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
    
    // Stub implementations for other ClusterService methods
    async fn join_cluster(
        &self,
        _request: Request<crate::iroh_types::JoinRequest>,
    ) -> Result<Response<crate::iroh_types::JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn leave_cluster(
        &self,
        _request: Request<crate::iroh_types::LeaveRequest>,
    ) -> Result<Response<crate::iroh_types::LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn get_cluster_status(
        &self,
        _request: Request<crate::iroh_types::ClusterStatusRequest>,
    ) -> Result<Response<crate::iroh_types::ClusterStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn health_check(
        &self,
        _request: Request<crate::iroh_types::HealthCheckRequest>,
    ) -> Result<Response<crate::iroh_types::HealthCheckResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn send_raft_message(
        &self,
        _request: Request<crate::iroh_types::RaftMessageRequest>,
    ) -> Result<Response<crate::iroh_types::RaftMessageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn submit_task(
        &self,
        _request: Request<crate::iroh_types::TaskRequest>,
    ) -> Result<Response<crate::iroh_types::TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn get_task_status(
        &self,
        _request: Request<crate::iroh_types::TaskStatusRequest>,
    ) -> Result<Response<crate::iroh_types::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn create_vm(
        &self,
        _request: Request<crate::iroh_types::CreateVmRequest>,
    ) -> Result<Response<crate::iroh_types::CreateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: Request<crate::iroh_types::CreateVmWithSchedulingRequest>,
    ) -> Result<Response<crate::iroh_types::CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn start_vm(
        &self,
        _request: Request<crate::iroh_types::StartVmRequest>,
    ) -> Result<Response<crate::iroh_types::StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn stop_vm(
        &self,
        _request: Request<crate::iroh_types::StopVmRequest>,
    ) -> Result<Response<crate::iroh_types::StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn delete_vm(
        &self,
        _request: Request<crate::iroh_types::DeleteVmRequest>,
    ) -> Result<Response<crate::iroh_types::DeleteVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn list_vms(
        &self,
        _request: Request<crate::iroh_types::ListVmsRequest>,
    ) -> Result<Response<crate::iroh_types::ListVmsResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn get_vm_status(
        &self,
        _request: Request<crate::iroh_types::GetVmStatusRequest>,
    ) -> Result<Response<crate::iroh_types::GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn migrate_vm(
        &self,
        _request: Request<crate::iroh_types::MigrateVmRequest>,
    ) -> Result<Response<crate::iroh_types::MigrateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: Request<crate::iroh_types::ScheduleVmPlacementRequest>,
    ) -> Result<Response<crate::iroh_types::ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: Request<crate::iroh_types::ClusterResourceSummaryRequest>,
    ) -> Result<Response<crate::iroh_types::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: Request<crate::iroh_types::GetP2pStatusRequest>,
    ) -> Result<Response<crate::iroh_types::GetP2pStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM image service"))
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
            VmImageOperation::Share { image_name, image_path, version, metadata } => {
                match self.service.share_image(&image_name, &image_path, &version, metadata).await {
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
            VmImageOperation::Get { image_name, version, image_hash } => {
                match self.service.get_image(&image_name, &version, &image_hash).await {
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
            VmImageOperation::List => {
                match self.service.list_images().await {
                    Ok(images) => Ok(VmImageResponse::List { images }),
                    Err(e) => Err(e),
                }
            }
        }
    }
}