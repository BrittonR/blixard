//! Status service implementation for Iroh RPC
//!
//! This provides cluster and Raft status queries over Iroh transport.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    iroh_types,
    transport::{
        iroh_protocol::{deserialize_payload, serialize_payload},
        iroh_service::IrohService,
        services::status::{StatusService, StatusServiceImpl},
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

// Wrapper types for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClusterStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClusterStatusResponse {
    pub leader_id: u64,
    pub member_ids: Vec<u64>,
    pub term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetRaftStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetRaftStatusResponse {
    pub is_leader: bool,
    pub node_id: u64,
    pub leader_id: u64,
    pub term: u64,
    pub state: String,
}

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
                let proto_response = self.service.get_cluster_status().await?;
                
                // Convert to wrapper type
                let response = ClusterStatusResponse {
                    leader_id: proto_response.leader_id,
                    member_ids: proto_response.nodes.iter().map(|n| n.id).collect(),
                    term: proto_response.term,
                };
                
                // Serialize response
                serialize_payload(&response)
            }
            
            "get_raft_status" => {
                // Deserialize request
                let _request: GetRaftStatusRequest = deserialize_payload(&payload)?;
                
                // Get Raft status
                let proto_response = self.service.node.get_raft_status().await?;
                
                // Convert to wrapper type
                let response = GetRaftStatusResponse {
                    is_leader: proto_response.is_leader,
                    node_id: proto_response.node_id,
                    leader_id: proto_response.leader_id.unwrap_or(0),
                    term: proto_response.term,
                    state: proto_response.state,
                };
                
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
        use crate::types::NodeConfig;
        
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test".to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
        };
        let node = Arc::new(SharedNodeState::new(config));
        let service = IrohStatusService::new(node);
        
        assert_eq!(service.name(), "status");
        assert_eq!(service.methods(), vec!["get_cluster_status", "get_raft_status"]);
    }
}