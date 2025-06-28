//! Health service implementation for Iroh RPC
//!
//! This demonstrates how to implement a service using our custom Iroh transport.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    proto::{HealthCheckRequest, HealthCheckResponse},
    transport::{
        iroh_protocol::{deserialize_payload, serialize_payload},
        iroh_service::IrohService,
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

/// Health service implementation
pub struct IrohHealthService {
    node: Arc<SharedNodeState>,
}

impl IrohHealthService {
    /// Create a new health service
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
}

#[async_trait]
impl IrohService for IrohHealthService {
    fn name(&self) -> &'static str {
        "health"
    }
    
    fn methods(&self) -> Vec<&'static str> {
        vec!["check", "ping"]
    }
    
    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        match method {
            "check" => {
                // Deserialize request
                let _request: HealthCheckRequest = deserialize_payload(&payload)?;
                
                // Create response
                let response = HealthCheckResponse {
                    healthy: true,
                    message: format!("Node {} is healthy", self.node.get_id()),
                };
                
                // Serialize response
                serialize_payload(&response)
            }
            
            "ping" => {
                // Simple ping/pong
                Ok(Bytes::from_static(b"pong"))
            }
            
            _ => Err(BlixardError::Internal {
                message: format!("Unknown method: {}", method),
            }),
        }
    }
}

/// Client wrapper for health service
pub struct IrohHealthClient<'a> {
    client: &'a crate::transport::iroh_service::IrohRpcClient,
    node_addr: iroh::NodeAddr,
}

impl<'a> IrohHealthClient<'a> {
    /// Create a new health client
    pub fn new(
        client: &'a crate::transport::iroh_service::IrohRpcClient,
        node_addr: iroh::NodeAddr,
    ) -> Self {
        Self { client, node_addr }
    }
    
    /// Perform health check
    pub async fn check(&self) -> BlixardResult<HealthCheckResponse> {
        let request = HealthCheckRequest {};
        self.client
            .call(self.node_addr.clone(), "health", "check", request)
            .await
    }
    
    /// Simple ping
    pub async fn ping(&self) -> BlixardResult<String> {
        let response: Bytes = self.client
            .call(self.node_addr.clone(), "health", "ping", ())
            .await?;
        Ok(String::from_utf8_lossy(&response).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_health_service_creation() {
        let node = Arc::new(SharedNodeState::new(1));
        let service = IrohHealthService::new(node);
        
        assert_eq!(service.name(), "health");
        assert_eq!(service.methods(), vec!["check", "ping"]);
    }
    
    #[tokio::test]
    async fn test_ping_method() {
        let node = Arc::new(SharedNodeState::new(1));
        let service = IrohHealthService::new(node);
        
        let result = service.handle_call("ping", Bytes::new()).await.unwrap();
        assert_eq!(&result[..], b"pong");
    }
}