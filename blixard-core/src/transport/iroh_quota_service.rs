//! Iroh service wrapper for quota operations
//!
//! This module provides the Iroh service interface for quota management.

use crate::{
    error::BlixardResult,
    node_shared::SharedNodeState,
    transport::{
        iroh_protocol::IrohService,
        services::quota::{QuotaProtocolHandler, QuotaOperationRequest, QuotaOperationResponse},
    },
};
use async_trait::async_trait;
use std::sync::Arc;

/// Iroh service for quota operations
pub struct IrohQuotaService {
    handler: QuotaProtocolHandler,
}

impl IrohQuotaService {
    /// Create a new quota service
    pub fn new(shared_state: Arc<SharedNodeState>) -> Self {
        Self {
            handler: QuotaProtocolHandler::new(shared_state),
        }
    }
}

#[async_trait]
impl IrohService for IrohQuotaService {
    fn name(&self) -> &'static str {
        "quota"
    }

    async fn handle_call(&self, method: &str, payload: Vec<u8>) -> BlixardResult<Vec<u8>> {
        // Deserialize the request based on method
        let request: QuotaOperationRequest = match method {
            "set_quota" | "get_quota" | "list_quotas" | "get_usage" | "remove_quota" => {
                crate::transport::iroh_protocol::deserialize_payload(&payload)?
            }
            _ => {
                return Err(crate::error::BlixardError::NotFound {
                    resource: format!("Method not found: {}", method),
                })
            }
        };

        // Create a dummy connection for now (not used by quota handler)
        let endpoint = iroh::endpoint::Endpoint::builder()
            .discovery_n0()
            .bind()
            .await
            .map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to create endpoint: {}", e),
            })?;

        let node_id = iroh::key::PublicKey::from_bytes(&[0u8; 32]).expect("valid key");
        let addr = iroh::NodeAddr::new(node_id);
        
        let connection = endpoint
            .connect(addr, crate::transport::BLIXARD_ALPN)
            .await
            .map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to create dummy connection: {}", e),
            })?;

        // Handle the request
        let response = self.handler.handle_request(connection, request).await?;

        // Serialize the response
        crate::transport::iroh_protocol::serialize_payload(&response)
    }
}