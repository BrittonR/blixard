//! Status query service implementation
//!
//! This service provides cluster and Raft status queries that work
//! over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    proto::{
        ClusterStatusRequest, ClusterStatusResponse, NodeInfo, NodeState,
        GetRaftStatusRequest, GetRaftStatusResponse,
    },
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Trait for status query operations
#[async_trait]
pub trait StatusService: Send + Sync {
    /// Get cluster status
    async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse>;
    
    /// Get Raft status
    async fn get_raft_status(&self) -> BlixardResult<GetRaftStatusResponse>;
}

/// Status service implementation
#[derive(Clone)]
pub struct StatusServiceImpl {
    node: Arc<SharedNodeState>,
}

impl StatusServiceImpl {
    /// Create a new status service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
}

#[async_trait]
impl StatusService for StatusServiceImpl {
    async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse> {
        // Get Raft status to determine leadership and term
        let raft_status = self.node.get_raft_status().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get Raft status: {}", e),
            })?;
        
        let leader_id = raft_status.leader_id.unwrap_or(0);
        let term = raft_status.term;
        
        // Get all configured node IDs from Raft
        let node_ids = self.node.get_raft_node_ids().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get node IDs: {}", e),
            })?;
        
        // Get peers for address information
        let peers = self.node.get_peers().await;
        
        // Build node list from the authoritative Raft configuration
        let mut nodes = Vec::new();
        
        for node_id in node_ids {
            let (address, state) = if node_id == self.node.get_id() {
                // Self
                let state = if raft_status.is_leader {
                    NodeState::Leader
                } else if raft_status.state == "candidate" {
                    NodeState::Candidate
                } else {
                    NodeState::Follower
                };
                (self.node.get_bind_addr().to_string(), state)
            } else {
                // Peer - get address from peers list
                let peer_addr = peers.iter()
                    .find(|p| p.id == node_id)
                    .map(|p| p.address.clone())
                    .unwrap_or_else(|| format!("unknown-{}", node_id));
                    
                // For peers, we can determine if they're the leader
                let state = if Some(node_id) == raft_status.leader_id {
                    NodeState::Leader
                } else {
                    // We don't have enough info to know if they're candidates or followers
                    NodeState::Follower
                };
                (peer_addr, state)
            };
            
            // Get P2P info if available
            let (p2p_node_id, p2p_addresses, p2p_relay_url) = 
                if let Some(p2p_manager) = self.node.get_p2p_manager().await {
                    if node_id == self.node.get_id() {
                        // Self - get our own P2P info
                        let node_addr = p2p_manager.get_node_address().await;
                        (
                            node_addr.node_id.to_string(),
                            node_addr.direct_addresses.iter()
                                .map(|addr| addr.to_string())
                                .collect(),
                            node_addr.relay_url.map(|url| url.to_string())
                                .unwrap_or_default(),
                        )
                    } else {
                        // Peer - would need to query them for their P2P info
                        (String::new(), Vec::new(), String::new())
                    }
                } else {
                    (String::new(), Vec::new(), String::new())
                };
            
            nodes.push(NodeInfo {
                id: node_id,
                address,
                state: state.into(),
                p2p_node_id,
                p2p_addresses,
                p2p_relay_url,
            });
        }
        
        Ok(ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        })
    }
    
    async fn get_raft_status(&self) -> BlixardResult<GetRaftStatusResponse> {
        let raft_status = self.node.get_raft_status().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get Raft status: {}", e),
            })?;
        
        Ok(GetRaftStatusResponse {
            is_leader: raft_status.is_leader,
            node_id: self.node.get_id(),
            leader_id: raft_status.leader_id.unwrap_or(0),
            term: raft_status.term,
            state: raft_status.state,
        })
    }
}

/// gRPC adapter for status service - implements ClusterService partially
#[async_trait]
impl crate::proto::cluster_service_server::ClusterService for StatusServiceImpl {
    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("get_cluster_status"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("get_cluster_status")]);
        
        match self.get_cluster_status().await {
            Ok(response) => Ok(Response::new(response)),
            Err(e) => Err(Status::internal(format!("Failed to get cluster status: {}", e))),
        }
    }
    
    // Stub implementations for other methods required by the trait
    async fn join_cluster(
        &self,
        _request: Request<crate::proto::JoinRequest>,
    ) -> Result<Response<crate::proto::JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn leave_cluster(
        &self,
        _request: Request<crate::proto::LeaveRequest>,
    ) -> Result<Response<crate::proto::LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn health_check(
        &self,
        _request: Request<crate::proto::HealthCheckRequest>,
    ) -> Result<Response<crate::proto::HealthCheckResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn send_raft_message(
        &self,
        _request: Request<crate::proto::RaftMessageRequest>,
    ) -> Result<Response<crate::proto::RaftMessageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn submit_task(
        &self,
        _request: Request<crate::proto::TaskRequest>,
    ) -> Result<Response<crate::proto::TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn get_task_status(
        &self,
        _request: Request<crate::proto::TaskStatusRequest>,
    ) -> Result<Response<crate::proto::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn create_vm(
        &self,
        _request: Request<crate::proto::CreateVmRequest>,
    ) -> Result<Response<crate::proto::CreateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<Response<crate::proto::CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn start_vm(
        &self,
        _request: Request<crate::proto::StartVmRequest>,
    ) -> Result<Response<crate::proto::StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn stop_vm(
        &self,
        _request: Request<crate::proto::StopVmRequest>,
    ) -> Result<Response<crate::proto::StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn delete_vm(
        &self,
        _request: Request<crate::proto::DeleteVmRequest>,
    ) -> Result<Response<crate::proto::DeleteVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn list_vms(
        &self,
        _request: Request<crate::proto::ListVmsRequest>,
    ) -> Result<Response<crate::proto::ListVmsResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn get_vm_status(
        &self,
        _request: Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<Response<crate::proto::GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn migrate_vm(
        &self,
        _request: Request<crate::proto::MigrateVmRequest>,
    ) -> Result<Response<crate::proto::MigrateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<Response<crate::proto::ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<Response<crate::proto::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<Response<crate::proto::GetP2pStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn share_vm_image(
        &self,
        _request: Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<Response<crate::proto::ShareVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn get_vm_image(
        &self,
        _request: Request<crate::proto::GetVmImageRequest>,
    ) -> Result<Response<crate::proto::GetVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<Response<crate::proto::ListP2pImagesResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
}

/// gRPC adapter for status service - implements BlixardService
#[async_trait]
impl crate::proto::blixard_service_server::BlixardService for StatusServiceImpl {
    async fn get_raft_status(
        &self,
        _request: Request<GetRaftStatusRequest>,
    ) -> Result<Response<GetRaftStatusResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("get_raft_status"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("get_raft_status")]);
        
        match self.get_raft_status().await {
            Ok(response) => Ok(Response::new(response)),
            Err(e) => Err(Status::internal(format!("Failed to get Raft status: {}", e))),
        }
    }
    
    async fn propose_task(
        &self,
        _request: Request<crate::proto::ProposeTaskRequest>,
    ) -> Result<Response<crate::proto::ProposeTaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in status service"))
    }
}

/// Iroh protocol handler for status service
pub struct StatusProtocolHandler {
    service: StatusServiceImpl,
}

impl StatusProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: StatusServiceImpl::new(node),
        }
    }
    
    /// Handle a status request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request_type: StatusRequestType,
    ) -> BlixardResult<()> {
        // TODO: Implement proper protocol handling
        match request_type {
            StatusRequestType::ClusterStatus => {
                // Handle cluster status request
            }
            StatusRequestType::RaftStatus => {
                // Handle Raft status request
            }
        }
        
        Err(BlixardError::NotImplemented {
            feature: "Iroh status protocol handler".to_string(),
        })
    }
}

/// Types of status requests
pub enum StatusRequestType {
    ClusterStatus,
    RaftStatus,
}