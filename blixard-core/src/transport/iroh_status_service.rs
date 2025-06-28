//! Status service implementation for Iroh RPC
//!
//! This provides cluster and Raft status queries over Iroh transport.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    proto::{
        ClusterStatusRequest, ClusterStatusResponse,
        GetRaftStatusRequest, GetRaftStatusResponse,
    },
    transport::{
        iroh_protocol::{deserialize_payload, serialize_payload},
        iroh_service::IrohService,
        services::status::{StatusService, StatusServiceImpl},
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

/// Status service implementation for Iroh
pub struct IrohStatusService {
    service: StatusServiceImpl,
}

impl IrohStatusService {
    /// Create a new status service
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: StatusServiceImpl::new(node),
        }
    }
}

#[async_trait]
impl IrohService for IrohStatusService {
    fn name(&self) -> &'static str {
        "status"
    }
    
    fn methods(&self) -> Vec<&'static str> {
        vec!["get_cluster_status", "get_raft_status"]
    }
    
    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        match method {
            "get_cluster_status" => {
                // Deserialize request
                let _request: ClusterStatusRequest = deserialize_payload(&payload)?;
                
                // Get cluster status
                let response = self.service.get_cluster_status().await?;
                
                // Serialize response
                serialize_payload(&response)
            }
            
            "get_raft_status" => {
                // Deserialize request
                let _request: GetRaftStatusRequest = deserialize_payload(&payload)?;
                
                // Get Raft status
                let response = self.service.get_raft_status().await?;
                
                // Serialize response
                serialize_payload(&response)
            }
            
            _ => Err(BlixardError::Internal {
                message: format!("Unknown method: {}", method),
            }),
        }
    }
}

/// Client wrapper for status service
pub struct IrohStatusClient<'a> {
    client: &'a crate::transport::iroh_service::IrohRpcClient,
    node_addr: iroh::NodeAddr,
}

impl<'a> IrohStatusClient<'a> {
    /// Create a new status client
    pub fn new(
        client: &'a crate::transport::iroh_service::IrohRpcClient,
        node_addr: iroh::NodeAddr,
    ) -> Self {
        Self { client, node_addr }
    }
    
    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse> {
        let request = ClusterStatusRequest {};
        self.client
            .call(self.node_addr.clone(), "status", "get_cluster_status", request)
            .await
    }
    
    /// Get Raft status
    pub async fn get_raft_status(&self) -> BlixardResult<GetRaftStatusResponse> {
        let request = GetRaftStatusRequest {};
        self.client
            .call(self.node_addr.clone(), "status", "get_raft_status", request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_status_service_creation() {
        let node = Arc::new(SharedNodeState::new(1));
        let service = IrohStatusService::new(node);
        
        assert_eq!(service.name(), "status");
        assert_eq!(service.methods(), vec!["get_cluster_status", "get_raft_status"]);
    }
}