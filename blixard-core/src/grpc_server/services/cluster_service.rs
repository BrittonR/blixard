//! Cluster management gRPC service implementation
//!
//! This module handles cluster operations including node joining/leaving,
//! cluster status queries, and Raft message forwarding.

use crate::{
    node_shared::SharedNodeState,
    grpc_server::common::GrpcMiddleware,
    proto::{
        cluster_service_server::ClusterService,
        JoinRequest, JoinResponse, LeaveRequest, LeaveResponse,
        ClusterStatusRequest, ClusterStatusResponse,
        RaftMessageRequest, RaftMessageResponse,
    },
    security::Permission,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Cluster service implementation
#[derive(Clone)]
pub struct ClusterServiceImpl {
    node: Arc<SharedNodeState>,
    middleware: GrpcMiddleware,
}

impl ClusterServiceImpl {
    /// Create a new cluster service instance
    pub fn new(
        node: Arc<SharedNodeState>,
        security_middleware: Option<crate::grpc_security::GrpcSecurityMiddleware>,
    ) -> Self {
        let middleware = GrpcMiddleware::new(security_middleware, None, None);
        
        Self {
            node,
            middleware,
        }
    }
}

#[tonic::async_trait]
impl ClusterService for ClusterServiceImpl {
    // Cluster management methods
    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        // Use Cedar for authorization if available
        let cluster_id = "default";
        let _ctx = if self.middleware.has_cedar() {
            let (_ctx, _tenant_id) = self.middleware
                .authenticate_and_authorize_cedar(
                    &request,
                    "joinCluster",
                    "Cluster",
                    cluster_id,
                )
                .await?;
            _ctx
        } else {
            // Fall back to traditional RBAC
            self.middleware
                .authenticate(&request, Permission::ClusterWrite)
                .await?
        };
        
        Err(Status::unimplemented("join_cluster not implemented"))
    }
    
    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        // Use Cedar for authorization if available
        let cluster_id = "default";
        let _ctx = if self.middleware.has_cedar() {
            let (_ctx, _tenant_id) = self.middleware
                .authenticate_and_authorize_cedar(
                    &request,
                    "leaveCluster",
                    "Cluster",
                    cluster_id,
                )
                .await?;
            _ctx
        } else {
            // Fall back to traditional RBAC
            self.middleware
                .authenticate(&request, Permission::ClusterWrite)
                .await?
        };
        
        Err(Status::unimplemented("leave_cluster not implemented"))
    }
    
    async fn get_cluster_status(
        &self,
        request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        use crate::proto::{NodeInfo, NodeState};
        
        // Use Cedar for authorization if available
        let cluster_id = "default";
        let _ctx = if self.middleware.has_cedar() {
            let (_ctx, _tenant_id) = self.middleware
                .authenticate_and_authorize_cedar(
                    &request,
                    "readCluster",
                    "Cluster",
                    cluster_id,
                )
                .await?;
            _ctx
        } else {
            // Fall back to traditional RBAC
            self.middleware
                .authenticate(&request, Permission::ClusterRead)
                .await?
        };
        
        // Get cluster status from Raft configuration (authoritative source)
        let (leader_id, node_ids, term) = self.node.get_cluster_status().await
            .map_err(|e| Status::internal(format!("Failed to get cluster status: {}", e)))?;
        
        // Get Raft status for state information
        let raft_status = self.node.get_raft_status().await
            .map_err(|e| Status::internal(format!("Failed to get Raft status: {}", e)))?;
        
        // Get peers for address information
        let peers = self.node.get_peers().await;
        
        // Get our P2P node address if available
        let p2p_node_addr = self.node.get_p2p_node_addr().await;
        
        // Build node list from the authoritative Raft configuration
        let mut nodes = Vec::new();
        
        for node_id in node_ids {
            let (address, state, p2p_node_id, p2p_addresses, p2p_relay_url) = if node_id == self.node.get_id() {
                // Self
                let state = if raft_status.is_leader {
                    NodeState::Leader
                } else if raft_status.state == "candidate" {
                    NodeState::Candidate
                } else {
                    NodeState::Follower
                };
                
                // Extract P2P info for self
                let (p2p_id, p2p_addrs, relay_url) = if let Some(ref addr) = p2p_node_addr {
                    (
                        // NodeAddr doesn't have a direct node_id() method, use format
                        format!("{:?}", addr),
                        // TODO: Get direct addresses from NodeAddr when API is available
                        Vec::new(),
                        // TODO: Get relay URL from NodeAddr when API is available
                        String::new(),
                    )
                } else {
                    (String::new(), Vec::new(), String::new())
                };
                
                (self.node.get_bind_addr().to_string(), state, p2p_id, p2p_addrs, relay_url)
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
                
                // TODO: Get P2P info for peers from P2P manager or stored state
                // For now, return empty P2P info for peers
                (peer_addr, state, String::new(), Vec::new(), String::new())
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
        
        let response = ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        };

        Ok(Response::new(response))
    }
    
    // Raft communication
    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        // Raft messages are internal cluster operations - require cluster admin
        let cluster_id = "default";
        let _ctx = if self.middleware.has_cedar() {
            let (_ctx, _tenant_id) = self.middleware
                .authenticate_and_authorize_cedar(
                    &request,
                    "manageCluster",
                    "Cluster",
                    cluster_id,
                )
                .await?;
            _ctx
        } else {
            // Fall back to traditional RBAC
            self.middleware
                .authenticate(&request, Permission::Admin)
                .await?
        };
        
        Err(Status::unimplemented("send_raft_message not implemented"))
    }
    
    // Task management
    async fn submit_task(
        &self,
        request: Request<crate::proto::TaskRequest>,
    ) -> Result<Response<crate::proto::TaskResponse>, Status> {
        Err(Status::unimplemented("submit_task not implemented"))
    }
    
    async fn get_task_status(
        &self,
        request: Request<crate::proto::TaskStatusRequest>,
    ) -> Result<Response<crate::proto::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("get_task_status not implemented"))
    }
    
    // VM operations
    async fn create_vm(
        &self,
        request: Request<crate::proto::CreateVmRequest>,
    ) -> Result<Response<crate::proto::CreateVmResponse>, Status> {
        Err(Status::unimplemented("create_vm not implemented"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        request: Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<Response<crate::proto::CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("create_vm_with_scheduling not implemented"))
    }
    
    async fn start_vm(
        &self,
        request: Request<crate::proto::StartVmRequest>,
    ) -> Result<Response<crate::proto::StartVmResponse>, Status> {
        Err(Status::unimplemented("start_vm not implemented"))
    }
    
    async fn stop_vm(
        &self,
        request: Request<crate::proto::StopVmRequest>,
    ) -> Result<Response<crate::proto::StopVmResponse>, Status> {
        Err(Status::unimplemented("stop_vm not implemented"))
    }
    
    async fn delete_vm(
        &self,
        request: Request<crate::proto::DeleteVmRequest>,
    ) -> Result<Response<crate::proto::DeleteVmResponse>, Status> {
        Err(Status::unimplemented("delete_vm not implemented"))
    }
    
    async fn list_vms(
        &self,
        request: Request<crate::proto::ListVmsRequest>,
    ) -> Result<Response<crate::proto::ListVmsResponse>, Status> {
        Err(Status::unimplemented("list_vms not implemented"))
    }
    
    async fn get_vm_status(
        &self,
        request: Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<Response<crate::proto::GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("get_vm_status not implemented"))
    }
    
    async fn migrate_vm(
        &self,
        request: Request<crate::proto::MigrateVmRequest>,
    ) -> Result<Response<crate::proto::MigrateVmResponse>, Status> {
        Err(Status::unimplemented("migrate_vm not implemented"))
    }
    
    // VM scheduling operations
    async fn schedule_vm_placement(
        &self,
        request: Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<Response<crate::proto::ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("schedule_vm_placement not implemented"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        request: Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<Response<crate::proto::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("get_cluster_resource_summary not implemented"))
    }
    
    // Health check
    async fn health_check(
        &self,
        _request: Request<crate::proto::HealthCheckRequest>,
    ) -> Result<Response<crate::proto::HealthCheckResponse>, Status> {
        Err(Status::unimplemented("health_check not implemented"))
    }
    
    // P2P operations
    async fn get_p2p_status(
        &self,
        _request: Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<Response<crate::proto::GetP2pStatusResponse>, Status> {
        Err(Status::unimplemented("get_p2p_status not implemented"))
    }
    
    async fn share_vm_image(
        &self,
        _request: Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<Response<crate::proto::ShareVmImageResponse>, Status> {
        Err(Status::unimplemented("share_vm_image not implemented"))
    }
    
    async fn get_vm_image(
        &self,
        _request: Request<crate::proto::GetVmImageRequest>,
    ) -> Result<Response<crate::proto::GetVmImageResponse>, Status> {
        Err(Status::unimplemented("get_vm_image not implemented"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<Response<crate::proto::ListP2pImagesResponse>, Status> {
        Err(Status::unimplemented("list_p2p_images not implemented"))
    }
}