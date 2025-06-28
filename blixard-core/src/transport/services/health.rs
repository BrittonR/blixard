//! Health check service implementation
//!
//! This service provides health checking functionality that works
//! over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    proto::{HealthCheckRequest, HealthCheckResponse},
    transport::config::TransportConfig,
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Trait for health check operations
#[async_trait]
pub trait HealthService: Send + Sync {
    /// Check health status
    async fn check_health(&self) -> BlixardResult<HealthCheckResponse>;
}

/// Health service implementation
#[derive(Clone)]
pub struct HealthServiceImpl {
    node: Arc<SharedNodeState>,
}

impl HealthServiceImpl {
    /// Create a new health service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
    
    /// Check if the node is healthy
    async fn is_node_healthy(&self) -> bool {
        // Check various health indicators
        
        // 1. Check if we can get Raft status (indicates Raft is running)
        if self.node.get_raft_status().await.is_err() {
            return false;
        }
        
        // 2. Check if we can access database
        if let Some(database) = self.node.get_database().await {
            // Try a simple operation to verify database is responsive
            // Just checking if we can get the database is enough for health check
            let _ = database;
        } else {
            return false;
        }
        
        // 3. Check if peer connector is healthy (for networked nodes)
        if let Some(peer_connector) = self.node.get_peer_connector().await {
            // Could check connection health here
            let _ = peer_connector;
        }
        
        true
    }
}

#[async_trait]
impl HealthService for HealthServiceImpl {
    async fn check_health(&self) -> BlixardResult<HealthCheckResponse> {
        let healthy = self.is_node_healthy().await;
        let message = if healthy {
            format!("Node {} is healthy", self.node.get_id())
        } else {
            format!("Node {} is unhealthy", self.node.get_id())
        };
        
        Ok(HealthCheckResponse {
            healthy,
            message,
        })
    }
}

/// gRPC adapter for health service
#[async_trait]
impl crate::proto::cluster_service_server::ClusterService for HealthServiceImpl {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let metrics = metrics();
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            vec![
                attributes::method("health_check"),
                attributes::node_id(self.node.get_id()),
            ],
        );
        metrics.grpc_requests_total.add(1, &[attributes::method("health_check")]);
        
        match self.check_health().await {
            Ok(response) => Ok(Response::new(response)),
            Err(e) => Err(Status::internal(format!("Health check failed: {}", e))),
        }
    }
    
    // Stub implementations for other methods required by the trait
    async fn join_cluster(
        &self,
        _request: Request<crate::proto::JoinRequest>,
    ) -> Result<Response<crate::proto::JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn leave_cluster(
        &self,
        _request: Request<crate::proto::LeaveRequest>,
    ) -> Result<Response<crate::proto::LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_cluster_status(
        &self,
        _request: Request<crate::proto::ClusterStatusRequest>,
    ) -> Result<Response<crate::proto::ClusterStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn send_raft_message(
        &self,
        _request: Request<crate::proto::RaftMessageRequest>,
    ) -> Result<Response<crate::proto::RaftMessageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn submit_task(
        &self,
        _request: Request<crate::proto::TaskRequest>,
    ) -> Result<Response<crate::proto::TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_task_status(
        &self,
        _request: Request<crate::proto::TaskStatusRequest>,
    ) -> Result<Response<crate::proto::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn create_vm(
        &self,
        _request: Request<crate::proto::CreateVmRequest>,
    ) -> Result<Response<crate::proto::CreateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn create_vm_with_scheduling(
        &self,
        _request: Request<crate::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<Response<crate::proto::CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn start_vm(
        &self,
        _request: Request<crate::proto::StartVmRequest>,
    ) -> Result<Response<crate::proto::StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn stop_vm(
        &self,
        _request: Request<crate::proto::StopVmRequest>,
    ) -> Result<Response<crate::proto::StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn delete_vm(
        &self,
        _request: Request<crate::proto::DeleteVmRequest>,
    ) -> Result<Response<crate::proto::DeleteVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn list_vms(
        &self,
        _request: Request<crate::proto::ListVmsRequest>,
    ) -> Result<Response<crate::proto::ListVmsResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_vm_status(
        &self,
        _request: Request<crate::proto::GetVmStatusRequest>,
    ) -> Result<Response<crate::proto::GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn migrate_vm(
        &self,
        _request: Request<crate::proto::MigrateVmRequest>,
    ) -> Result<Response<crate::proto::MigrateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn schedule_vm_placement(
        &self,
        _request: Request<crate::proto::ScheduleVmPlacementRequest>,
    ) -> Result<Response<crate::proto::ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_cluster_resource_summary(
        &self,
        _request: Request<crate::proto::ClusterResourceSummaryRequest>,
    ) -> Result<Response<crate::proto::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_p2p_status(
        &self,
        _request: Request<crate::proto::GetP2pStatusRequest>,
    ) -> Result<Response<crate::proto::GetP2pStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn share_vm_image(
        &self,
        _request: Request<crate::proto::ShareVmImageRequest>,
    ) -> Result<Response<crate::proto::ShareVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn get_vm_image(
        &self,
        _request: Request<crate::proto::GetVmImageRequest>,
    ) -> Result<Response<crate::proto::GetVmImageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
    
    async fn list_p2p_images(
        &self,
        _request: Request<crate::proto::ListP2pImagesRequest>,
    ) -> Result<Response<crate::proto::ListP2pImagesResponse>, Status> {
        Err(Status::unimplemented("Not implemented in health service"))
    }
}

/// Iroh protocol handler for health service
pub struct HealthProtocolHandler {
    service: HealthServiceImpl,
}

impl HealthProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: HealthServiceImpl::new(node),
        }
    }
    
    /// Handle a health check request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
    ) -> BlixardResult<()> {
        // TODO: Implement proper protocol handling
        // For now, this is a placeholder
        
        // 1. Read request from connection
        // 2. Deserialize HealthCheckRequest
        // 3. Call service.check_health()
        // 4. Serialize response
        // 5. Send response back
        
        Err(BlixardError::NotImplemented {
            feature: "Iroh health protocol handler".to_string(),
        })
    }
}