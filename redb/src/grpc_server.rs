//! gRPC server implementation for Blixard cluster service
//!
//! This module provides the gRPC server that handles cluster management
//! and VM orchestration requests.

use crate::{
    error::{BlixardError, BlixardResult},
    node::Node,
    types::{VmCommand, VmStatus as InternalVmStatus},
    proto::{
        cluster_service_server::{ClusterService, ClusterServiceServer},
        CreateVmRequest, CreateVmResponse, GetVmStatusRequest, GetVmStatusResponse,
        HealthCheckRequest, HealthCheckResponse, JoinRequest, JoinResponse, LeaveRequest,
        LeaveResponse, ListVmsRequest, ListVmsResponse, NodeInfo, NodeState, StartVmRequest,
        StartVmResponse, StopVmRequest, StopVmResponse, VmInfo, VmState, ClusterStatusRequest,
        ClusterStatusResponse,
    },
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// gRPC service implementation for Blixard cluster operations
pub struct BlixardGrpcService {
    node: Arc<Node>,
}

impl BlixardGrpcService {
    /// Create a new gRPC service instance
    pub fn new(node: Arc<Node>) -> Self {
        Self { node }
    }

    /// Convert internal VM status to proto VM state
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

    /// Convert internal error to gRPC status
    fn error_to_status(err: BlixardError) -> Status {
        match err {
            BlixardError::NotImplemented { feature } => {
                Status::unimplemented(format!("Feature not implemented: {}", feature))
            }
            BlixardError::ServiceNotFound(name) => {
                Status::not_found(format!("Service not found: {}", name))
            }
            BlixardError::ServiceAlreadyExists(name) => {
                Status::already_exists(format!("Service already exists: {}", name))
            }
            BlixardError::ConfigError(msg) => {
                Status::invalid_argument(format!("Configuration error: {}", msg))
            }
            BlixardError::Storage { operation, .. } => {
                Status::internal(format!("Storage error during {}", operation))
            }
            BlixardError::Serialization { operation, .. } => {
                Status::internal(format!("Serialization error during {}", operation))
            }
            BlixardError::Raft { operation, .. } => Status::internal(format!("Raft error during {}", operation)),
            BlixardError::NodeError(msg) => Status::internal(format!("Node error: {}", msg)),
            BlixardError::ClusterError(msg) => Status::internal(format!("Cluster error: {}", msg)),
            _ => Status::internal("Internal error"),
        }
    }
}

#[tonic::async_trait]
impl ClusterService for BlixardGrpcService {
    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let _req = request.into_inner();
        
        // TODO: Implement cluster join logic once Raft is ready
        Err(Status::unimplemented("Cluster join not yet implemented"))
    }

    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let _req = request.into_inner();
        
        // TODO: Implement cluster leave logic once Raft is ready
        Err(Status::unimplemented("Cluster leave not yet implemented"))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        // TODO: Get actual cluster status from Raft
        let response = ClusterStatusResponse {
            leader_id: 0,
            nodes: vec![NodeInfo {
                id: self.node.get_id(),
                address: self.node.get_bind_addr().to_string(),
                state: NodeState::Follower.into(),
            }],
            term: 0,
        };

        Ok(Response::new(response))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();

        // Validate request
        if req.name.is_empty() {
            return Ok(Response::new(CreateVmResponse {
                success: false,
                message: "VM name cannot be empty".to_string(),
                vm_id: String::new(),
            }));
        }

        // Send create command to node
        let vm_config = crate::types::VmConfig {
            name: req.name.clone(),
            config_path: req.config_path,
            vcpus: req.vcpus,
            memory: req.memory_mb,
        };
        
        let command = VmCommand::Create {
            config: vm_config,
            node_id: self.node.get_id(),
        };

        match self.node.send_vm_command(command).await {
            Ok(_) => Ok(Response::new(CreateVmResponse {
                success: true,
                message: format!("VM '{}' created successfully", req.name),
                vm_id: req.name, // Using name as ID for now
            })),
            Err(e) => Ok(Response::new(CreateVmResponse {
                success: false,
                message: format!("Failed to create VM: {}", e),
                vm_id: String::new(),
            })),
        }
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();

        let command = VmCommand::Start {
            name: req.name.clone(),
        };

        match self.node.send_vm_command(command).await {
            Ok(_) => Ok(Response::new(StartVmResponse {
                success: true,
                message: format!("VM '{}' start command sent", req.name),
            })),
            Err(e) => Ok(Response::new(StartVmResponse {
                success: false,
                message: format!("Failed to start VM: {}", e),
            })),
        }
    }

    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let req = request.into_inner();

        let command = VmCommand::Stop {
            name: req.name.clone(),
        };

        match self.node.send_vm_command(command).await {
            Ok(_) => Ok(Response::new(StopVmResponse {
                success: true,
                message: format!("VM '{}' stop command sent", req.name),
            })),
            Err(e) => Ok(Response::new(StopVmResponse {
                success: false,
                message: format!("Failed to stop VM: {}", e),
            })),
        }
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        match self.node.list_vms().await {
            Ok(vms) => {
                let vm_infos: Vec<VmInfo> = vms
                    .into_iter()
                    .map(|(vm_config, vm_status)| VmInfo {
                        name: vm_config.name,
                        state: Self::vm_status_to_proto(&vm_status).into(),
                        node_id: self.node.get_id(),
                        vcpus: vm_config.vcpus,
                        memory_mb: vm_config.memory,
                    })
                    .collect();

                Ok(Response::new(ListVmsResponse { vms: vm_infos }))
            }
            Err(e) => Err(Self::error_to_status(e)),
        }
    }

    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let req = request.into_inner();

        match self.node.get_vm_status(&req.name).await {
            Ok(Some((vm_config, vm_status))) => {
                let vm_info = VmInfo {
                    name: vm_config.name,
                    state: Self::vm_status_to_proto(&vm_status).into(),
                    node_id: self.node.get_id(),
                    vcpus: vm_config.vcpus,
                    memory_mb: vm_config.memory,
                };

                Ok(Response::new(GetVmStatusResponse {
                    found: true,
                    vm_info: Some(vm_info),
                }))
            }
            Ok(None) => Ok(Response::new(GetVmStatusResponse {
                found: false,
                vm_info: None,
            })),
            Err(e) => Err(Self::error_to_status(e)),
        }
    }

    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        // Simple health check - just verify node is running
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: format!("Node {} is healthy", self.node.get_id()),
        }))
    }
}

/// Start the gRPC server
pub async fn start_grpc_server(
    node: Arc<Node>,
    bind_address: std::net::SocketAddr,
) -> BlixardResult<()> {
    let service = BlixardGrpcService::new(node);
    let server = ClusterServiceServer::new(service);

    tracing::info!("Starting gRPC server on {}", bind_address);

    tonic::transport::Server::builder()
        .add_service(server)
        .serve(bind_address)
        .await
        .map_err(|e| BlixardError::Internal { 
            message: format!("gRPC server error: {}", e) 
        })?;

    Ok(())
}