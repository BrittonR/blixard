//! VM service implementation for dual transport
//!
//! This service handles VM lifecycle operations over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    types::{VmCommand, VmConfig, VmStatus as InternalVmStatus},
    proto::{
        CreateVmRequest, CreateVmResponse, DeleteVmRequest, DeleteVmResponse,
        GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse,
        StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse,
        VmInfo, VmState, MigrateVmRequest, MigrateVmResponse,
        CreateVmWithSchedulingRequest, CreateVmWithSchedulingResponse,
        ScheduleVmPlacementRequest, ScheduleVmPlacementResponse,
    },
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use serde::{Serialize, Deserialize};

/// VM operation request types for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOperationRequest {
    Create {
        name: String,
        config_path: String,
        vcpus: u32,
        memory_mb: u32,
    },
    Start {
        name: String,
    },
    Stop {
        name: String,
    },
    Delete {
        name: String,
    },
    List,
    GetStatus {
        name: String,
    },
    Migrate {
        vm_name: String,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    },
}

/// VM operation response for Iroh transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOperationResponse {
    Create {
        success: bool,
        message: String,
        vm_id: String,
    },
    Start {
        success: bool,
        message: String,
    },
    Stop {
        success: bool,
        message: String,
    },
    Delete {
        success: bool,
        message: String,
    },
    List {
        vms: Vec<VmInfoData>,
    },
    GetStatus {
        found: bool,
        vm_info: Option<VmInfoData>,
    },
    Migrate {
        success: bool,
        message: String,
        source_node_id: u64,
        target_node_id: u64,
        status: i32,
        duration_ms: i64,
    },
}

/// Serializable VM info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfoData {
    pub name: String,
    pub state: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub node_id: u64,
    pub ip_address: String,
}

/// Trait for VM operations
#[async_trait]
pub trait VmService: Send + Sync {
    /// Create a new VM
    async fn create_vm(&self, name: String, vcpus: u32, memory_mb: u32) -> BlixardResult<String>;
    
    /// Start a VM
    async fn start_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Stop a VM
    async fn stop_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// Delete a VM
    async fn delete_vm(&self, name: &str) -> BlixardResult<()>;
    
    /// List all VMs
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>>;
    
    /// Get VM status
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>>;
    
    /// Migrate a VM to another node
    async fn migrate_vm(
        &self,
        vm_name: &str,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    ) -> BlixardResult<()>;
}

/// VM service implementation
#[derive(Clone)]
pub struct VmServiceImpl {
    node: Arc<SharedNodeState>,
}

impl VmServiceImpl {
    /// Create a new VM service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
    
    /// Convert internal VM status to proto
    fn vm_status_to_proto(status: &InternalVmStatus) -> VmState {
        match status {
            InternalVmStatus::Creating => VmState::Created,
            InternalVmStatus::Starting => VmState::Starting,
            InternalVmStatus::Running => VmState::Running,
            InternalVmStatus::Stopping => VmState::Stopping,
            InternalVmStatus::Stopped => VmState::Stopped,
            InternalVmStatus::Failed => VmState::Failed,
        }
    }
}

#[async_trait]
impl VmService for VmServiceImpl {
    async fn create_vm(&self, name: String, vcpus: u32, memory_mb: u32) -> BlixardResult<String> {
        let vm_config = VmConfig {
            name: name.clone(),
            config_path: format!("/etc/blixard/vms/{}.yaml", name),
            vcpus,
            memory: memory_mb,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
        };
        
        // Send command through Raft consensus
        self.node.send_vm_command(VmCommand::Create {
            config: vm_config,
            node_id: self.node.get_id(),
        }).await?;
        
        Ok(name)
    }
    
    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Start {
            name: name.to_string(),
        }).await
    }
    
    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Stop {
            name: name.to_string(),
        }).await
    }
    
    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        self.node.send_vm_command(VmCommand::Delete {
            name: name.to_string(),
        }).await
    }
    
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, InternalVmStatus)>> {
        self.node.list_vms().await
    }
    
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, InternalVmStatus)>> {
        self.node.get_vm_status(name).await
    }
    
    async fn migrate_vm(
        &self,
        vm_name: &str,
        target_node_id: u64,
        live_migration: bool,
        force: bool,
    ) -> BlixardResult<()> {
        use crate::types::VmMigrationTask;
        
        // Verify we're the leader
        if !self.node.is_leader().await {
            return Err(BlixardError::Internal {
                message: "Not the leader".to_string(),
            });
        }
        
        let migration_task = VmMigrationTask {
            vm_name: vm_name.to_string(),
            source_node_id: self.node.get_id(),
            target_node_id,
            live_migration,
            force,
        };
        
        self.node.send_vm_command(VmCommand::Migrate { task: migration_task }).await
    }
}

/// gRPC adapter for VM service - implements ClusterService partially
#[async_trait]
impl crate::proto::cluster_service_server::ClusterService for VmServiceImpl {
    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("create_vm"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("create_vm")]);
        
        let req = request.into_inner();
        
        match VmService::create_vm(self, req.name.clone(), req.vcpus, req.memory_mb).await {
            Ok(vm_id) => Ok(Response::new(CreateVmResponse {
                success: true,
                message: format!("VM '{}' created successfully", req.name),
                vm_id,
            })),
            Err(e) => Ok(Response::new(CreateVmResponse {
                success: false,
                message: e.to_string(),
                vm_id: String::new(),
            })),
        }
    }
    
    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("start_vm"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("start_vm")]);
        
        let req = request.into_inner();
        
        match VmService::start_vm(self, &req.name).await {
            Ok(()) => Ok(Response::new(StartVmResponse {
                success: true,
                message: format!("VM '{}' start command issued", req.name),
            })),
            Err(e) => Ok(Response::new(StartVmResponse {
                success: false,
                message: e.to_string(),
            })),
        }
    }
    
    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("stop_vm"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("stop_vm")]);
        
        let req = request.into_inner();
        
        match VmService::stop_vm(self, &req.name).await {
            Ok(()) => Ok(Response::new(StopVmResponse {
                success: true,
                message: format!("VM '{}' stop command issued", req.name),
            })),
            Err(e) => Ok(Response::new(StopVmResponse {
                success: false,
                message: e.to_string(),
            })),
        }
    }
    
    async fn delete_vm(
        &self,
        request: Request<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("delete_vm"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("delete_vm")]);
        
        let req = request.into_inner();
        
        match VmService::delete_vm(self, &req.name).await {
            Ok(()) => Ok(Response::new(DeleteVmResponse {
                success: true,
                message: format!("VM '{}' deleted successfully", req.name),
            })),
            Err(e) => Ok(Response::new(DeleteVmResponse {
                success: false,
                message: e.to_string(),
            })),
        }
    }
    
    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("list_vms"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("list_vms")]);
        
        match VmService::list_vms(self).await {
            Ok(vms) => {
                let vm_infos = vms.into_iter().map(|(config, status)| {
                    VmInfo {
                        name: config.name,
                        state: Self::vm_status_to_proto(&status) as i32,
                        vcpus: config.vcpus,
                        memory_mb: config.memory,
                        node_id: self.node.get_id(),
                        ip_address: config.ip_address.unwrap_or_default(),
                    }
                }).collect();
                
                Ok(Response::new(ListVmsResponse { vms: vm_infos }))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
    
    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("get_vm_status"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("get_vm_status")]);
        
        let req = request.into_inner();
        
        match VmService::get_vm_status(self, &req.name).await {
            Ok(Some((config, status))) => {
                Ok(Response::new(GetVmStatusResponse {
                    found: true,
                    vm_info: Some(VmInfo {
                        name: config.name,
                        state: Self::vm_status_to_proto(&status) as i32,
                        vcpus: config.vcpus,
                        memory_mb: config.memory,
                        node_id: self.node.get_id(),
                        ip_address: config.ip_address.unwrap_or_default(),
                    }),
                }))
            }
            Ok(None) => {
                Ok(Response::new(GetVmStatusResponse {
                    found: false,
                    vm_info: None,
                }))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
    
    async fn migrate_vm(
        &self,
        request: Request<MigrateVmRequest>,
    ) -> Result<Response<MigrateVmResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("migrate_vm"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("migrate_vm")]);
        
        let req = request.into_inner();
        
        match VmService::migrate_vm(self, &req.vm_name, req.target_node_id, req.live_migration, req.force).await {
            Ok(()) => Ok(Response::new(MigrateVmResponse {
                success: true,
                message: format!("Migration of VM '{}' started", req.vm_name),
                source_node_id: self.node.get_id(),
                target_node_id: req.target_node_id,
                status: 1, // MIGRATION_STATUS_PREPARING
                duration_ms: 0,
            })),
            Err(e) => Ok(Response::new(MigrateVmResponse {
                success: false,
                message: e.to_string(),
                source_node_id: self.node.get_id(),
                target_node_id: 0,
                status: 5, // MIGRATION_STATUS_FAILED
                duration_ms: 0,
            })),
        }
    }
    
    // Stub implementations for other methods
    async fn join_cluster(
        &self,
        _request: Request<crate::proto::JoinRequest>,
    ) -> Result<Response<crate::proto::JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn leave_cluster(
        &self,
        _request: Request<crate::proto::LeaveRequest>,
    ) -> Result<Response<crate::proto::LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn get_cluster_status(
        &self,
        _request: Request<crate::proto::ClusterStatusRequest>,
    ) -> Result<Response<crate::proto::ClusterStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn health_check(
        &self,
        _request: Request<crate::proto::HealthCheckRequest>,
    ) -> Result<Response<crate::proto::HealthCheckResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn send_raft_message(
        &self,
        _request: Request<crate::proto::RaftMessageRequest>,
    ) -> Result<Response<crate::proto::RaftMessageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn submit_task(
        &self,
        _request: Request<crate::proto::TaskRequest>,
    ) -> Result<Response<crate::proto::TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn get_task_status(
        &self,
        _request: Request<crate::proto::TaskStatusRequest>,
    ) -> Result<Response<crate::proto::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: Request<CreateVmWithSchedulingRequest>,
    ) -> Result<Response<CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("VM scheduling not implemented yet"))
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: Request<ScheduleVmPlacementRequest>,
    ) -> Result<Response<ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("VM placement scheduling not implemented yet"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<Response<crate::proto::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<Response<crate::proto::GetP2pStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn share_vm_image(
        &self,
        _request: Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<Response<crate::proto::ShareVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn get_vm_image(
        &self,
        _request: Request<crate::proto::GetVmImageRequest>,
    ) -> Result<Response<crate::proto::GetVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<Response<crate::proto::ListP2pImagesResponse>, Status> {
        Err(Status::unimplemented("Not implemented in VM service"))
    }
}

/// Iroh protocol handler for VM service
pub struct VmProtocolHandler {
    service: VmServiceImpl,
}

impl VmProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: VmServiceImpl::new(node),
        }
    }
    
    /// Handle a VM operation request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request: VmOperationRequest,
    ) -> BlixardResult<VmOperationResponse> {
        match request {
            VmOperationRequest::Create { name, config_path: _, vcpus, memory_mb } => {
                match self.service.create_vm(name.clone(), vcpus, memory_mb).await {
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
            VmOperationRequest::Start { name } => {
                match self.service.start_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Start {
                        success: true,
                        message: format!("VM '{}' started", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Start {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::Stop { name } => {
                match self.service.stop_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Stop {
                        success: true,
                        message: format!("VM '{}' stopped", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Stop {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::Delete { name } => {
                match self.service.delete_vm(&name).await {
                    Ok(()) => Ok(VmOperationResponse::Delete {
                        success: true,
                        message: format!("VM '{}' deleted", name),
                    }),
                    Err(e) => Ok(VmOperationResponse::Delete {
                        success: false,
                        message: e.to_string(),
                    }),
                }
            }
            VmOperationRequest::List => {
                match self.service.list_vms().await {
                    Ok(vms) => {
                        let vm_infos = vms.into_iter().map(|(config, status)| {
                            VmInfoData {
                                name: config.name,
                                state: format!("{:?}", status),
                                vcpus: config.vcpus,
                                memory_mb: config.memory,
                                node_id: self.service.node.get_id(),
                                ip_address: config.ip_address.unwrap_or_default(),
                            }
                        }).collect();
                        Ok(VmOperationResponse::List { vms: vm_infos })
                    }
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::GetStatus { name } => {
                match self.service.get_vm_status(&name).await {
                    Ok(Some((config, status))) => {
                        Ok(VmOperationResponse::GetStatus {
                            found: true,
                            vm_info: Some(VmInfoData {
                                name: config.name,
                                state: format!("{:?}", status),
                                vcpus: config.vcpus,
                                memory_mb: config.memory,
                                node_id: self.service.node.get_id(),
                                ip_address: config.ip_address.unwrap_or_default(),
                            }),
                        })
                    }
                    Ok(None) => Ok(VmOperationResponse::GetStatus {
                        found: false,
                        vm_info: None,
                    }),
                    Err(e) => Err(e),
                }
            }
            VmOperationRequest::Migrate { vm_name, target_node_id, live_migration, force } => {
                match self.service.migrate_vm(&vm_name, target_node_id, live_migration, force).await {
                    Ok(()) => Ok(VmOperationResponse::Migrate {
                        success: true,
                        message: format!("Migration of VM '{}' started", vm_name),
                        source_node_id: self.service.node.get_id(),
                        target_node_id,
                        status: 1, // MIGRATION_STATUS_PREPARING
                        duration_ms: 0,
                    }),
                    Err(e) => Ok(VmOperationResponse::Migrate {
                        success: false,
                        message: e.to_string(),
                        source_node_id: self.service.node.get_id(),
                        target_node_id: 0,
                        status: 5, // MIGRATION_STATUS_FAILED
                        duration_ms: 0,
                    }),
                }
            }
        }
    }
}