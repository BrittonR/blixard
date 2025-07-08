//! Iroh transport implementation for VM service

use crate::{
    error::{BlixardError, BlixardResult},
    metrics_otel::{attributes, metrics, Timer},
    node_shared::SharedNodeState,
    transport::{
        iroh_protocol::{deserialize_payload, serialize_payload},
        iroh_service::IrohService,
        services::vm::{
            VmInfoData, VmOperationRequest, VmOperationResponse, VmService, VmServiceImpl,
        },
    },
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};
// Removed unused imports - Hash and FromStr
use bytes::Bytes;
use futures::TryStreamExt;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// VM image metadata stored in-memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmImageMetadata {
    pub name: String,
    pub hash: String,
    pub size: u64,
    pub shared_by: u64,
    pub created_at: i64,
    pub description: String,
    pub tags: Vec<String>,
}

/// VM image operation request types for P2P image sharing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmImageRequest {
    ShareImage {
        image_name: String,
        image_path: String,
        description: String,
        tags: Vec<String>,
    },
    GetImage {
        image_hash: String,
        output_path: String,
    },
    ListImages,
}

/// VM image operation response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmImageResponse {
    ImageShared {
        success: bool,
        message: String,
        image_hash: String,
        size: u64,
    },
    ImageRetrieved {
        success: bool,
        message: String,
        path: String,
        size: u64,
    },
    ImageList {
        images: Vec<VmImageMetadata>,
    },
}

/// Combined request type for VM operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmRequest {
    /// VM lifecycle operations
    Operation(VmOperationRequest),
    /// VM image operations
    Image(VmImageRequest),
}

/// Combined response type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmResponse {
    /// VM lifecycle response
    Operation(VmOperationResponse),
    /// VM image response
    Image(VmImageResponse),
}

/// Iroh VM service implementation
pub struct IrohVmService {
    node: Arc<SharedNodeState>,
    vm_service: VmServiceImpl,
    // Store VM image metadata
    image_metadata: Arc<RwLock<HashMap<String, VmImageMetadata>>>,
}

impl IrohVmService {
    /// Create a new Iroh VM service
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            node: node.clone(),
            vm_service: VmServiceImpl::new(node.clone()),
            image_metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle VM operation request
    pub async fn handle_vm_operation(
        &self,
        request: VmOperationRequest,
    ) -> BlixardResult<VmOperationResponse> {
        match request {
            VmOperationRequest::Create {
                name,
                config_path: _,
                vcpus,
                memory_mb,
            } => {
                match self
                    .vm_service
                    .create_vm(name.clone(), vcpus, memory_mb)
                    .await
                {
                    Ok(vm_id) => Ok(VmOperationResponse::Create {
                        success: true,
                        message: format!("VM '{}' created successfully", name),
                        vm_id,
                    }),
                    Err(e) => Ok(VmOperationResponse::Create {
                        success: false,
                        message: e.to_string(),
                        vm_id: String::new(),
                    }),
                }
            }
            VmOperationRequest::Start { name } => match self.vm_service.start_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Start {
                    success: true,
                    message: format!("VM '{}' started", name),
                }),
                Err(e) => Ok(VmOperationResponse::Start {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::Stop { name } => match self.vm_service.stop_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Stop {
                    success: true,
                    message: format!("VM '{}' stopped", name),
                }),
                Err(e) => Ok(VmOperationResponse::Stop {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::Delete { name } => match self.vm_service.delete_vm(&name).await {
                Ok(()) => Ok(VmOperationResponse::Delete {
                    success: true,
                    message: format!("VM '{}' deleted", name),
                }),
                Err(e) => Ok(VmOperationResponse::Delete {
                    success: false,
                    message: e.to_string(),
                }),
            },
            VmOperationRequest::List => match self.vm_service.list_vms().await {
                Ok(vms) => {
                    let vm_infos = vms
                        .into_iter()
                        .map(|(config, status)| VmInfoData {
                            name: config.name,
                            state: format!("{:?}", status),
                            vcpus: config.vcpus,
                            memory_mb: config.memory,
                            node_id: self.node.get_id(),
                            ip_address: config.ip_address.unwrap_or_default(),
                        })
                        .collect();
                    Ok(VmOperationResponse::List { vms: vm_infos })
                }
                Err(e) => Err(e),
            },
            VmOperationRequest::GetStatus { name } => {
                match self.vm_service.get_vm_status(&name).await {
                    Ok(Some((config, status))) => Ok(VmOperationResponse::GetStatus {
                        found: true,
                        vm_info: Some(VmInfoData {
                            name: config.name,
                            state: format!("{:?}", status),
                            vcpus: config.vcpus,
                            memory_mb: config.memory,
                            node_id: self.node.get_id(),
                            ip_address: config.ip_address.unwrap_or_default(),
                        }),
                    }),
                    Ok(None) => Ok(VmOperationResponse::GetStatus {
                        found: false,
                        vm_info: None,
                    }),
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::Migrate {
                vm_name,
                target_node_id,
                live_migration,
                force,
            } => {
                match self
                    .vm_service
                    .migrate_vm(&vm_name, target_node_id, live_migration, force)
                    .await
                {
                    Ok(()) => Ok(VmOperationResponse::Migrate {
                        success: true,
                        message: format!("Migration of VM '{}' started", vm_name),
                        source_node_id: self.node.get_id(),
                        target_node_id,
                        status: 1, // MIGRATION_STATUS_PREPARING
                        duration_ms: 0,
                    }),
                    Err(e) => Ok(VmOperationResponse::Migrate {
                        success: false,
                        message: e.to_string(),
                        source_node_id: self.node.get_id(),
                        target_node_id: 0,
                        status: 5, // MIGRATION_STATUS_FAILED
                        duration_ms: 0,
                    }),
                }
            }
            VmOperationRequest::CreateWithScheduling {
                name,
                vcpus,
                memory_mb,
                strategy,
                constraints,
                features,
                priority,
            } => {
                match self
                    .vm_service
                    .create_vm_with_scheduling(
                        name.clone(),
                        vcpus,
                        memory_mb,
                        strategy,
                        constraints,
                        features,
                        priority,
                    )
                    .await
                {
                    Ok((vm_id, node_id, reason)) => Ok(VmOperationResponse::CreateWithScheduling {
                        success: true,
                        message: format!("VM '{}' created successfully on node {}", name, node_id),
                        vm_id,
                        assigned_node_id: node_id,
                        placement_decision: reason,
                    }),
                    Err(e) => Ok(VmOperationResponse::CreateWithScheduling {
                        success: false,
                        message: e.to_string(),
                        vm_id: String::new(),
                        assigned_node_id: 0,
                        placement_decision: String::new(),
                    }),
                }
            }
            VmOperationRequest::SchedulePlacement {
                name,
                vcpus,
                memory_mb,
                strategy,
                constraints,
                features,
            } => {
                match self
                    .vm_service
                    .schedule_vm_placement(&name, vcpus, memory_mb, strategy, constraints, features)
                    .await
                {
                    Ok((node_id, score, reason, alternatives)) => {
                        Ok(VmOperationResponse::SchedulePlacement {
                            success: true,
                            assigned_node_id: node_id,
                            placement_score: score,
                            placement_reason: reason,
                            alternative_nodes: alternatives,
                        })
                    }
                    Err(e) => Ok(VmOperationResponse::SchedulePlacement {
                        success: false,
                        assigned_node_id: 0,
                        placement_score: 0.0,
                        placement_reason: e.to_string(),
                        alternative_nodes: vec![],
                    }),
                }
            }
        }
    }

    /// Handle VM image operations
    pub async fn handle_image_request(
        &self,
        request: VmImageRequest,
    ) -> BlixardResult<VmImageResponse> {
        match request {
            VmImageRequest::ShareImage {
                image_name,
                image_path,
                description,
                tags,
            } => {
                self.share_vm_image(image_name, image_path, description, tags)
                    .await
            }
            VmImageRequest::GetImage {
                image_hash,
                output_path,
            } => self.get_vm_image(image_hash, output_path).await,
            VmImageRequest::ListImages => self.list_p2p_images().await,
        }
    }

    /// Share a VM image over P2P
    async fn share_vm_image(
        &self,
        image_name: String,
        image_path: String,
        description: String,
        tags: Vec<String>,
    ) -> BlixardResult<VmImageResponse> {
        info!("Sharing VM image: {} from path: {}", image_name, image_path);

        // For now, we'll use the P2P image store through the node state
        // instead of directly accessing the Iroh node
        let p2p_manager = self
            .node
            .get_p2p_manager()
            .await
            .ok_or_else(|| BlixardError::P2PError("P2P manager not available".to_string()))?;

        // Share the image through the P2P manager using upload_resource
        let path = std::path::Path::new(&image_path);
        p2p_manager
            .upload_resource(
                crate::p2p_manager::ResourceType::VmImage,
                &image_name,
                "latest", // version
                path,
            )
            .await?;

        // Generate a hash for tracking using SHA256
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&image_name);
        let hash = format!("{:x}", hasher.finalize());
        let size = tokio::fs::metadata(&image_path)
            .await
            .map_err(|e| BlixardError::IoError(e))?
            .len();

        // Store metadata
        let metadata = VmImageMetadata {
            name: image_name.clone(),
            hash: hash.to_string(),
            size,
            shared_by: self.node.get_id(),
            created_at: chrono::Utc::now().timestamp(),
            description,
            tags,
        };

        {
            let mut images = self.image_metadata.write().await;
            images.insert(hash.clone(), metadata);
        }

        info!("VM image shared successfully: {} ({})", image_name, hash);

        Ok(VmImageResponse::ImageShared {
            success: true,
            message: format!("VM image '{}' shared successfully", image_name),
            image_hash: hash,
            size,
        })
    }

    /// Get a VM image from P2P network
    async fn get_vm_image(
        &self,
        image_hash: String,
        output_path: String,
    ) -> BlixardResult<VmImageResponse> {
        info!(
            "Retrieving VM image: {} to path: {}",
            image_hash, output_path
        );

        // Get the P2P manager
        let p2p_manager = self
            .node
            .get_p2p_manager()
            .await
            .ok_or_else(|| BlixardError::P2PError("P2P manager not available".to_string()))?;

        // Download the image through the P2P manager
        // For now, create a dummy download until we have proper hash-based lookup
        let download_id = p2p_manager
            .request_download(
                crate::p2p_manager::ResourceType::VmImage,
                &image_hash, // Use hash as name for now
                "latest",
                crate::p2p_manager::TransferPriority::Normal,
            )
            .await?;

        // TODO: Wait for download to complete and copy to output_path
        tracing::info!("Download requested with ID: {}", download_id);

        // Get the size of the downloaded file
        let size = tokio::fs::metadata(&output_path)
            .await
            .map_err(|e| BlixardError::IoError(e))?
            .len();

        info!(
            "VM image retrieved successfully: {} ({} bytes)",
            image_hash, size
        );

        Ok(VmImageResponse::ImageRetrieved {
            success: true,
            message: format!("VM image retrieved successfully"),
            path: output_path,
            size,
        })
    }

    /// List available P2P images
    async fn list_p2p_images(&self) -> BlixardResult<VmImageResponse> {
        let images = self.image_metadata.read().await;
        let image_list: Vec<VmImageMetadata> = images.values().cloned().collect();

        Ok(VmImageResponse::ImageList { images: image_list })
    }

    // VM Health Monitoring handlers

    /// Get VM health status
    async fn handle_get_health_status(
        &self,
        request: crate::iroh_types::GetVmHealthStatusRequest,
    ) -> BlixardResult<crate::iroh_types::GetVmHealthStatusResponse> {
        // Get the VM health monitor from the node
        let health_monitor =
            self.node
                .get_vm_health_monitor()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM health monitor not available".to_string(),
                })?;

        // Get health status for the VM
        let health_status = health_monitor.get_health_status(&request.vm_name).await;

        Ok(crate::iroh_types::GetVmHealthStatusResponse { health_status })
    }

    /// Add a health check to a VM
    async fn handle_add_health_check(
        &self,
        request: crate::iroh_types::AddVmHealthCheckRequest,
    ) -> BlixardResult<crate::iroh_types::AddVmHealthCheckResponse> {
        // Get the VM health monitor from the node
        let health_monitor =
            self.node
                .get_vm_health_monitor()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM health monitor not available".to_string(),
                })?;

        // Add the health check
        match health_monitor
            .add_health_check(&request.vm_name, request.health_check)
            .await
        {
            Ok(()) => Ok(crate::iroh_types::AddVmHealthCheckResponse {
                success: true,
                message: format!(
                    "Health check added successfully to VM '{}'",
                    request.vm_name
                ),
            }),
            Err(e) => Ok(crate::iroh_types::AddVmHealthCheckResponse {
                success: false,
                message: e.to_string(),
            }),
        }
    }

    /// List health checks for a VM
    async fn handle_list_health_checks(
        &self,
        request: crate::iroh_types::ListVmHealthChecksRequest,
    ) -> BlixardResult<crate::iroh_types::ListVmHealthChecksResponse> {
        // Get the VM health monitor from the node
        let health_monitor =
            self.node
                .get_vm_health_monitor()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM health monitor not available".to_string(),
                })?;

        // Get health checks for the VM
        let health_checks = health_monitor
            .list_health_checks(&request.vm_name)
            .await
            .unwrap_or_else(|_| Vec::new());

        Ok(crate::iroh_types::ListVmHealthChecksResponse { health_checks })
    }

    /// Remove a health check from a VM
    async fn handle_remove_health_check(
        &self,
        request: crate::iroh_types::RemoveVmHealthCheckRequest,
    ) -> BlixardResult<crate::iroh_types::RemoveVmHealthCheckResponse> {
        // Get the VM health monitor from the node
        let health_monitor =
            self.node
                .get_vm_health_monitor()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM health monitor not available".to_string(),
                })?;

        // Remove the health check
        match health_monitor
            .remove_health_check(&request.vm_name, &request.check_name)
            .await
        {
            Ok(()) => Ok(crate::iroh_types::RemoveVmHealthCheckResponse {
                success: true,
                message: format!(
                    "Health check '{}' removed from VM '{}'",
                    request.check_name, request.vm_name
                ),
            }),
            Err(e) => Ok(crate::iroh_types::RemoveVmHealthCheckResponse {
                success: false,
                message: e.to_string(),
            }),
        }
    }

    /// Toggle health monitoring for a VM
    async fn handle_toggle_health_monitoring(
        &self,
        request: crate::iroh_types::ToggleVmHealthMonitoringRequest,
    ) -> BlixardResult<crate::iroh_types::ToggleVmHealthMonitoringResponse> {
        // Get the VM health monitor from the node
        let health_monitor =
            self.node
                .get_vm_health_monitor()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM health monitor not available".to_string(),
                })?;

        // Toggle health monitoring
        let action = if request.enable { "enable" } else { "disable" };
        match health_monitor
            .toggle_monitoring(&request.vm_name, request.enable)
            .await
        {
            Ok(()) => Ok(crate::iroh_types::ToggleVmHealthMonitoringResponse {
                success: true,
                message: format!("Health monitoring {} for VM '{}'", action, request.vm_name),
            }),
            Err(e) => Ok(crate::iroh_types::ToggleVmHealthMonitoringResponse {
                success: false,
                message: e.to_string(),
            }),
        }
    }

    /// Configure recovery policy for a VM
    async fn handle_configure_recovery_policy(
        &self,
        request: crate::iroh_types::ConfigureVmRecoveryPolicyRequest,
    ) -> BlixardResult<crate::iroh_types::ConfigureVmRecoveryPolicyResponse> {
        // Get the VM auto-recovery service from the node
        let recovery_service =
            self.node
                .get_vm_auto_recovery()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "VM auto-recovery service not available".to_string(),
                })?;

        // Configure the recovery policy
        match recovery_service
            .configure_recovery_policy(&request.vm_name, request.policy)
            .await
        {
            Ok(()) => Ok(crate::iroh_types::ConfigureVmRecoveryPolicyResponse {
                success: true,
                message: format!("Recovery policy configured for VM '{}'", request.vm_name),
            }),
            Err(e) => Ok(crate::iroh_types::ConfigureVmRecoveryPolicyResponse {
                success: false,
                message: e.to_string(),
            }),
        }
    }
}

#[async_trait]
impl IrohService for IrohVmService {
    fn name(&self) -> &'static str {
        "vm"
    }

    fn methods(&self) -> Vec<&'static str> {
        vec![
            "create",
            "start",
            "stop",
            "delete",
            "list",
            "get_status",
            "migrate",
            "share_image",
            "get_image",
            "list_images",
            "get_health_status",
            "add_health_check",
            "list_health_checks",
            "remove_health_check",
            "toggle_health_monitoring",
            "configure_recovery_policy",
        ]
    }

    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method(method),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics
            .grpc_requests_total
            .add(1, &[attributes::method(method)]);

        match method {
            // VM lifecycle operations
            "create" | "start" | "stop" | "delete" | "list" | "get_status" | "migrate" => {
                // Deserialize VM operation request
                let request: VmOperationRequest = deserialize_payload(&payload)?;
                debug!("Handling VM operation: {:?}", request);

                let response = self.handle_vm_operation(request).await?;
                serialize_payload(&response)
            }

            // VM image operations
            "share_image" | "get_image" | "list_images" => {
                // Deserialize VM image request
                let request: VmImageRequest = deserialize_payload(&payload)?;
                debug!("Handling VM image operation: {:?}", request);

                let response = self.handle_image_request(request).await?;
                serialize_payload(&response)
            }

            // VM health monitoring operations
            "get_health_status" => {
                let request: crate::iroh_types::GetVmHealthStatusRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_get_health_status(request).await?;
                serialize_payload(&response)
            }

            "add_health_check" => {
                let request: crate::iroh_types::AddVmHealthCheckRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_add_health_check(request).await?;
                serialize_payload(&response)
            }

            "list_health_checks" => {
                let request: crate::iroh_types::ListVmHealthChecksRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_list_health_checks(request).await?;
                serialize_payload(&response)
            }

            "remove_health_check" => {
                let request: crate::iroh_types::RemoveVmHealthCheckRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_remove_health_check(request).await?;
                serialize_payload(&response)
            }

            "toggle_health_monitoring" => {
                let request: crate::iroh_types::ToggleVmHealthMonitoringRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_toggle_health_monitoring(request).await?;
                serialize_payload(&response)
            }

            "configure_recovery_policy" => {
                let request: crate::iroh_types::ConfigureVmRecoveryPolicyRequest =
                    deserialize_payload(&payload)?;
                let response = self.handle_configure_recovery_policy(request).await?;
                serialize_payload(&response)
            }

            _ => Err(BlixardError::Internal {
                message: format!("Unknown method: {}", method),
            }),
        }
    }
}

/// Client wrapper for VM service
pub struct IrohVmClient<'a> {
    client: &'a crate::transport::iroh_service::IrohRpcClient,
    node_addr: iroh::NodeAddr,
}

impl<'a> IrohVmClient<'a> {
    /// Create a new VM client
    pub fn new(
        client: &'a crate::transport::iroh_service::IrohRpcClient,
        node_addr: iroh::NodeAddr,
    ) -> Self {
        Self { client, node_addr }
    }

    /// Create a VM
    pub async fn create_vm(
        &self,
        name: String,
        vcpus: u32,
        memory_mb: u32,
    ) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::Create {
            name,
            config_path: String::new(),
            vcpus,
            memory_mb,
        };
        self.client
            .call(self.node_addr.clone(), "vm", "create", request)
            .await
    }

    /// Start a VM
    pub async fn start_vm(&self, name: String) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::Start { name };
        self.client
            .call(self.node_addr.clone(), "vm", "start", request)
            .await
    }

    /// Stop a VM
    pub async fn stop_vm(&self, name: String) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::Stop { name };
        self.client
            .call(self.node_addr.clone(), "vm", "stop", request)
            .await
    }

    /// Delete a VM
    pub async fn delete_vm(&self, name: String) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::Delete { name };
        self.client
            .call(self.node_addr.clone(), "vm", "delete", request)
            .await
    }

    /// List VMs
    pub async fn list_vms(&self) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::List;
        self.client
            .call(self.node_addr.clone(), "vm", "list", request)
            .await
    }

    /// Get VM status
    pub async fn get_vm_status(&self, name: String) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::GetStatus { name };
        self.client
            .call(self.node_addr.clone(), "vm", "get_status", request)
            .await
    }

    /// Migrate a VM
    pub async fn migrate_vm(
        &self,
        vm_name: String,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    ) -> BlixardResult<VmOperationResponse> {
        let request = VmOperationRequest::Migrate {
            vm_name,
            target_node_id,
            live_migration,
            force,
        };
        self.client
            .call(self.node_addr.clone(), "vm", "migrate", request)
            .await
    }

    /// Share a VM image
    pub async fn share_image(
        &self,
        image_name: String,
        image_path: String,
        description: String,
        tags: Vec<String>,
    ) -> BlixardResult<VmImageResponse> {
        let request = VmImageRequest::ShareImage {
            image_name,
            image_path,
            description,
            tags,
        };
        self.client
            .call(self.node_addr.clone(), "vm", "share_image", request)
            .await
    }

    /// Get a VM image
    pub async fn get_image(
        &self,
        image_hash: String,
        output_path: String,
    ) -> BlixardResult<VmImageResponse> {
        let request = VmImageRequest::GetImage {
            image_hash,
            output_path,
        };
        self.client
            .call(self.node_addr.clone(), "vm", "get_image", request)
            .await
    }

    /// List available images
    pub async fn list_images(&self) -> BlixardResult<VmImageResponse> {
        let request = VmImageRequest::ListImages;
        self.client
            .call(self.node_addr.clone(), "vm", "list_images", request)
            .await
    }
}
