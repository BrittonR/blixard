//! gRPC server implementation for Blixard cluster service
//!
//! This module provides the gRPC server that handles cluster management
//! and VM orchestration requests.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    types::{VmCommand, VmStatus as InternalVmStatus},
    raft_manager::{TaskSpec, ResourceRequirements},
    proto::{
        cluster_service_server::{ClusterService, ClusterServiceServer},
        blixard_service_server::{BlixardService, BlixardServiceServer},
        CreateVmRequest, CreateVmResponse, GetVmStatusRequest, GetVmStatusResponse,
        HealthCheckRequest, HealthCheckResponse, JoinRequest, JoinResponse, LeaveRequest,
        LeaveResponse, ListVmsRequest, ListVmsResponse, NodeInfo, NodeState, StartVmRequest,
        StartVmResponse, StopVmRequest, StopVmResponse, VmInfo, VmState, ClusterStatusRequest,
        ClusterStatusResponse, RaftMessageRequest, RaftMessageResponse, TaskRequest, TaskResponse,
        TaskStatusRequest, TaskStatusResponse, TaskStatus,
        GetRaftStatusRequest, GetRaftStatusResponse, ProposeTaskRequest, ProposeTaskResponse,
    },
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// gRPC service implementation for Blixard cluster operations
#[derive(Clone)]
pub struct BlixardGrpcService {
    node: Arc<SharedNodeState>,
}

impl BlixardGrpcService {
    /// Create a new gRPC service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
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

    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        let req = request.into_inner();
        
        // Parse Raft message from bytes
        let raft_msg = crate::raft_codec::deserialize_message(&req.raft_data)
            .map_err(|e| Status::invalid_argument(format!("Invalid Raft message: {}", e)))?;
        
        // Send to Raft manager through node
        match self.node.send_raft_message(raft_msg.from, raft_msg).await {
            Ok(_) => Ok(Response::new(RaftMessageResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(RaftMessageResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }

    async fn submit_task(
        &self,
        request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        let req = request.into_inner();
        
        // Create task specification
        let task_spec = TaskSpec {
            command: req.command,
            args: req.args,
            resources: ResourceRequirements {
                cpu_cores: req.cpu_cores,
                memory_mb: req.memory_mb,
                disk_gb: req.disk_gb,
                required_features: req.required_features,
            },
            timeout_secs: req.timeout_secs,
        };
        
        // Submit task through Raft
        match self.node.submit_task(&req.task_id, task_spec).await {
            Ok(assigned_node) => Ok(Response::new(TaskResponse {
                accepted: true,
                message: format!("Task {} assigned to node {}", req.task_id, assigned_node),
                assigned_node,
            })),
            Err(e) => Ok(Response::new(TaskResponse {
                accepted: false,
                message: format!("Failed to submit task: {}", e),
                assigned_node: 0,
            })),
        }
    }

    async fn get_task_status(
        &self,
        request: Request<TaskStatusRequest>,
    ) -> Result<Response<TaskStatusResponse>, Status> {
        let req = request.into_inner();
        
        match self.node.get_task_status(&req.task_id).await {
            Ok(Some((status, result))) => {
                let task_status = match status.as_str() {
                    "pending" => TaskStatus::Pending,
                    "running" => TaskStatus::Running,
                    "completed" => TaskStatus::Completed,
                    "failed" => TaskStatus::Failed,
                    _ => TaskStatus::Unknown,
                };
                
                Ok(Response::new(TaskStatusResponse {
                    found: true,
                    status: task_status.into(),
                    output: result.as_ref().map(|r| r.output.clone()).unwrap_or_default(),
                    error: result.and_then(|r| r.error).unwrap_or_default(),
                    execution_time_ms: 0, // TODO: Track execution time
                }))
            }
            Ok(None) => Ok(Response::new(TaskStatusResponse {
                found: false,
                status: TaskStatus::Unknown.into(),
                output: String::new(),
                error: String::new(),
                execution_time_ms: 0,
            })),
            Err(e) => Err(Self::error_to_status(e)),
        }
    }
}

#[tonic::async_trait]
impl BlixardService for BlixardGrpcService {
    async fn get_raft_status(
        &self,
        _request: Request<GetRaftStatusRequest>,
    ) -> Result<Response<GetRaftStatusResponse>, Status> {
        // Get Raft status from the node
        match self.node.get_raft_status().await {
            Ok(status) => {
                Ok(Response::new(GetRaftStatusResponse {
                    is_leader: status.is_leader,
                    node_id: status.node_id,
                    leader_id: status.leader_id.unwrap_or(0),
                    term: status.term,
                    state: status.state,
                }))
            }
            Err(e) => Err(Self::error_to_status(e)),
        }
    }

    async fn propose_task(
        &self,
        request: Request<ProposeTaskRequest>,
    ) -> Result<Response<ProposeTaskResponse>, Status> {
        let req = request.into_inner();
        
        // Validate task
        let task = req.task.ok_or_else(|| Status::invalid_argument("Task is required"))?;
        
        if task.id.is_empty() {
            return Ok(Response::new(ProposeTaskResponse {
                success: false,
                message: "Task ID cannot be empty".to_string(),
            }));
        }
        
        // Create task specification from proto task
        let task_spec = TaskSpec {
            command: task.command,
            args: task.args,
            resources: ResourceRequirements {
                cpu_cores: task.cpu_cores,
                memory_mb: task.memory_mb,
                disk_gb: 0, // Not in proto Task, default to 0
                required_features: vec![], // Not in proto Task, default to empty
            },
            timeout_secs: 300, // Default 5 minute timeout
        };
        
        // Submit task through Raft consensus
        match self.node.submit_task(&task.id, task_spec).await {
            Ok(assigned_node) => Ok(Response::new(ProposeTaskResponse {
                success: true,
                message: format!("Task {} proposed successfully, assigned to node {}", task.id, assigned_node),
            })),
            Err(e) => Ok(Response::new(ProposeTaskResponse {
                success: false,
                message: format!("Failed to propose task: {}", e),
            })),
        }
    }
}

/// Start the gRPC server
pub async fn start_grpc_server(
    node: Arc<SharedNodeState>,
    bind_address: std::net::SocketAddr,
) -> BlixardResult<()> {
    let service = BlixardGrpcService::new(node);
    let cluster_server = ClusterServiceServer::new(service.clone());
    let blixard_server = BlixardServiceServer::new(service);

    tracing::info!("Starting gRPC server on {}", bind_address);

    tonic::transport::Server::builder()
        .add_service(cluster_server)
        .add_service(blixard_server)
        .serve(bind_address)
        .await
        .map_err(|e| BlixardError::Internal { 
            message: format!("gRPC server error: {}", e) 
        })?;

    Ok(())
}