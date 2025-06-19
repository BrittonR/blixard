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
        let req = request.into_inner();
        tracing::info!("Received join request from node {} at {}", req.node_id, req.bind_address);
        
        // Validate the request
        if req.node_id == 0 {
            return Ok(Response::new(JoinResponse {
                success: false,
                message: "Invalid node ID: must be non-zero".to_string(),
                peers: vec![],
                voters: vec![],
            }));
        }
        
        if req.bind_address.is_empty() {
            return Ok(Response::new(JoinResponse {
                success: false,
                message: "Bind address cannot be empty".to_string(),
                peers: vec![],
                voters: vec![],
            }));
        }
        
        // Check if node already exists
        if let Some(_) = self.node.get_peer(req.node_id).await {
            return Ok(Response::new(JoinResponse {
                success: false,
                message: format!("Node {} already exists in cluster", req.node_id),
                peers: vec![],
                voters: vec![],
            }));
        }
        
        // Add peer to local tracking BEFORE proposing to Raft
        // This ensures the peer info is available for message routing
        match self.node.add_peer(req.node_id, req.bind_address.clone()).await {
            Ok(_) => {
                tracing::info!("[JOIN] Added peer {} at {} to local tracking", req.node_id, req.bind_address);
            },
            Err(e) => {
                return Ok(Response::new(JoinResponse {
                    success: false,
                    message: format!("Failed to add peer: {}", e),
                    peers: vec![],
                    voters: vec![],
                }));
            }
        }
        
        // Establish bidirectional connection before Raft operations
        // This ensures responses can be routed back
        if let Some(peer_connector) = self.node.get_peer_connector().await {
            if let Some(peer_info) = self.node.get_peer(req.node_id).await {
                tracing::info!("[JOIN] Pre-connecting to new node {} at {}", req.node_id, req.bind_address);
                if let Err(e) = peer_connector.connect_to_peer(&peer_info).await {
                    tracing::warn!("[JOIN] Failed to pre-connect to new node: {}", e);
                    // Don't fail the join - connection will be retried
                }
            }
        }
        
        // Check if we're the leader
        let is_leader = self.node.is_leader().await;
        tracing::info!("[JOIN] Node {} is_leader: {}", self.node.get_id(), is_leader);
        
        // Propose configuration change through Raft
        tracing::info!("[JOIN] Proposing configuration change to add node {} at address {}", req.node_id, req.bind_address);
        match self.node.propose_conf_change(
            crate::raft_manager::ConfChangeType::AddNode,
            req.node_id,
            req.bind_address.clone()
        ).await {
            Ok(_) => {
                // Note: The configuration change has been proposed, but not necessarily committed yet
                tracing::info!("[JOIN] Configuration change proposed to Raft for node {}", req.node_id);
                
                // Also propose worker registration for the joining node
                let capabilities = crate::raft_manager::WorkerCapabilities {
                    cpu_cores: 4, // Default capabilities for joining nodes
                    memory_mb: 8192,
                    disk_gb: 100,
                    features: vec!["microvm".to_string()],
                };
                
                let proposal_data = crate::raft_manager::ProposalData::RegisterWorker {
                    node_id: req.node_id,
                    address: req.bind_address.clone(),
                    capabilities,
                };
                
                let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
                let proposal = crate::raft_manager::RaftProposal {
                    id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                    data: proposal_data,
                    response_tx: Some(response_tx),
                };
                
                // Send worker registration proposal
                if let Err(e) = self.node.send_raft_proposal(proposal).await {
                    tracing::warn!("[JOIN] Failed to propose worker registration for node {}: {}", req.node_id, e);
                    // Don't fail the join - worker registration can be retried
                }
                // Get current peers to return
                let peers = self.node.get_peers().await;
                let mut peer_infos: Vec<NodeInfo> = peers.into_iter()
                    .map(|p| NodeInfo {
                        id: p.id,
                        address: p.address,
                        state: NodeState::Follower.into(), // TODO: Get actual state
                    })
                    .collect();
                
                // Include the current node (leader) in the response
                tracing::info!("[JOIN] Including self (node {}) at {} in join response", 
                    self.node.get_id(), self.node.get_bind_addr());
                peer_infos.push(NodeInfo {
                    id: self.node.get_id(),
                    address: self.node.get_bind_addr().to_string(),
                    state: if is_leader {
                        NodeState::Leader.into()
                    } else {
                        NodeState::Follower.into()
                    },
                });
                
                tracing::info!("[JOIN] Returning {} peers in join response to node {}", 
                    peer_infos.len(), req.node_id);
                
                // Get current configuration state to include in response
                let voters = match self.node.get_current_voters().await {
                    Ok(v) => {
                        tracing::info!("[JOIN] Retrieved current voters: {:?}", v);
                        v
                    }
                    Err(e) => {
                        tracing::warn!("[JOIN] Failed to get current voters: {}, using leader_id as fallback", e);
                        // If we can't get voters from storage, at least include the leader
                        vec![self.node.get_id()]
                    }
                };
                tracing::info!("[JOIN] Current cluster voters in response: {:?}", voters);
                
                Ok(Response::new(JoinResponse {
                    success: true,
                    message: format!("Node {} successfully joined cluster", req.node_id),
                    peers: peer_infos,
                    voters,
                }))
            }
            Err(e) => {
                // Remove peer on failure
                let _ = self.node.remove_peer(req.node_id).await;
                
                Ok(Response::new(JoinResponse {
                    success: false,
                    message: format!("Failed to join cluster: {}", e),
                    peers: vec![],
                    voters: vec![],
                }))
            }
        }
    }

    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("[LEAVE] Received leave request for node {}", req.node_id);
        
        // Validate the request
        if req.node_id == 0 {
            tracing::warn!("[LEAVE] Invalid node ID: 0");
            return Ok(Response::new(LeaveResponse {
                success: false,
                message: "Invalid node ID: must be non-zero".to_string(),
            }));
        }
        
        // Check if trying to remove self
        let self_id = self.node.get_id();
        if req.node_id == self_id {
            tracing::warn!("[LEAVE] Attempted to remove self (node {})", self_id);
            return Ok(Response::new(LeaveResponse {
                success: false,
                message: "Cannot remove self from cluster. Use shutdown instead.".to_string(),
            }));
        }
        
        // Check current cluster state before attempting configuration change
        let is_leader = self.node.is_leader().await;
        tracing::info!("[LEAVE] Current node {} is_leader: {}", self_id, is_leader);
        
        // Get and log current cluster status
        match self.node.get_cluster_status().await {
            Ok((leader_id, nodes, term)) => {
                tracing::info!("[LEAVE] Current cluster state: leader_id={:?}, term={}, nodes={:?}", 
                    leader_id, term, nodes);
                
                if !nodes.contains(&req.node_id) {
                    tracing::warn!("[LEAVE] Node {} not found in cluster configuration", req.node_id);
                    return Ok(Response::new(LeaveResponse {
                        success: false,
                        message: format!("Node {} not found in cluster configuration", req.node_id),
                    }));
                }
                
                // Validate that removing this node won't leave us with too few voters
                let remaining_voters = nodes.len() - 1;
                if remaining_voters == 0 {
                    tracing::error!("[LEAVE] Cannot remove node {} - would leave cluster with no voters", req.node_id);
                    return Ok(Response::new(LeaveResponse {
                        success: false,
                        message: format!("Cannot remove node {} - would leave cluster with no voters", req.node_id),
                    }));
                }
                
                // Warn if we're going down to a single node
                if remaining_voters == 1 {
                    tracing::warn!("[LEAVE] Removing node {} will leave cluster with only 1 voter - consensus may be affected", req.node_id);
                }
            }
            Err(e) => {
                tracing::error!("[LEAVE] Failed to get cluster status: {}", e);
                return Err(Self::error_to_status(e));
            }
        }
        
        // Get peer address if available (it's okay if we don't have it)
        let peer_address = match self.node.get_peer(req.node_id).await {
            Some(peer) => {
                tracing::info!("[LEAVE] Found peer {} at address {}", req.node_id, peer.address);
                peer.address
            }
            None => {
                let addr = format!("unknown-{}", req.node_id);
                tracing::warn!("[LEAVE] Peer {} not found in local tracking, using address: {}", req.node_id, addr);
                addr
            }
        };
        
        // Log current Raft status before configuration change
        match self.node.get_raft_status().await {
            Ok(status) => {
                tracing::info!("[LEAVE] Raft status before conf change: is_leader={}, node_id={}, leader_id={:?}, term={}, state={}", 
                    status.is_leader, status.node_id, status.leader_id, status.term, status.state);
            }
            Err(e) => {
                tracing::warn!("[LEAVE] Failed to get Raft status: {}", e);
            }
        }
        
        // Propose configuration change through Raft
        tracing::info!("[LEAVE] Proposing configuration change to remove node {} with address {}", 
            req.node_id, peer_address);
        match self.node.propose_conf_change(
            crate::raft_manager::ConfChangeType::RemoveNode,
            req.node_id,
            peer_address.clone()
        ).await {
            Ok(_) => {
                tracing::info!("[LEAVE] Configuration change successfully proposed for node {}", req.node_id);
                
                // Log cluster state after configuration change
                match self.node.get_cluster_status().await {
                    Ok((leader_id, nodes, term)) => {
                        tracing::info!("[LEAVE] Cluster state after conf change: leader_id={:?}, term={}, nodes={:?}", 
                            leader_id, term, nodes);
                    }
                    Err(e) => {
                        tracing::warn!("[LEAVE] Failed to get cluster status after conf change: {}", e);
                    }
                }
                
                // The Raft manager will remove the peer when applying the configuration change,
                // so we don't need to do it here. Just return success.
                tracing::info!("[LEAVE] Configuration change applied, node {} will be removed by Raft manager", req.node_id);
                Ok(Response::new(LeaveResponse {
                    success: true,
                    message: format!("Node {} successfully removed from cluster", req.node_id),
                }))
            }
            Err(e) => {
                tracing::error!("[LEAVE] Failed to propose configuration change for node {}: {}", req.node_id, e);
                // Try to get more context about the error
                match self.node.get_raft_status().await {
                    Ok(status) => {
                        tracing::error!("[LEAVE] Raft status after error: is_leader={}, node_id={}, leader_id={:?}, term={}, state={}", 
                            status.is_leader, status.node_id, status.leader_id, status.term, status.state);
                    }
                    Err(raft_err) => {
                        tracing::error!("[LEAVE] Also failed to get Raft status: {}", raft_err);
                    }
                }
                
                Ok(Response::new(LeaveResponse {
                    success: false,
                    message: format!("Failed to remove node from cluster: {}", e),
                }))
            }
        }
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        // Get Raft status
        let raft_status = self.node.get_raft_status().await
            .map_err(|e| Self::error_to_status(e))?;
        
        // Get all peers
        let peers = self.node.get_peers().await;
        
        // Build node list - include self and all peers
        let mut nodes = vec![NodeInfo {
            id: self.node.get_id(),
            address: self.node.get_bind_addr().to_string(),
            state: if raft_status.is_leader {
                NodeState::Leader.into()
            } else if raft_status.state == "candidate" {
                NodeState::Candidate.into()
            } else {
                NodeState::Follower.into()
            },
        }];
        
        // Add peers
        for peer in peers {
            nodes.push(NodeInfo {
                id: peer.id,
                address: peer.address,
                state: NodeState::Follower.into(), // TODO: Track actual peer states
            });
        }
        
        let response = ClusterStatusResponse {
            leader_id: raft_status.leader_id.unwrap_or(0),
            nodes,
            term: raft_status.term,
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

        // Create VM through Raft consensus
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

        match self.node.create_vm_through_raft(command).await {
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

        match self.node.send_vm_operation_through_raft(command).await {
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

        match self.node.send_vm_operation_through_raft(command).await {
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

    match tonic::transport::Server::builder()
        .add_service(cluster_server)
        .add_service(blixard_server)
        .serve(bind_address)
        .await
    {
        Ok(()) => (),
        Err(e) => {
            let error_msg = if e.to_string().contains("Address already in use") {
                format!("gRPC server error: Address {} already in use", bind_address)
            } else {
                format!("gRPC server error: transport error")
            };
            tracing::error!("Failed to start gRPC server on {}: {:?}", bind_address, e);
            return Err(BlixardError::Internal { 
                message: error_msg 
            });
        }
    }

    Ok(())
}