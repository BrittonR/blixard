//! Iroh service for handling Raft consensus messages
//!
//! This is a stub implementation to maintain compilation compatibility.

use crate::error::BlixardResult;
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_service::IrohService;
use std::sync::Arc;

/// Service for handling Raft consensus messages over Iroh
pub struct IrohRaftService {
    shared_state: Arc<SharedNodeState>,
}

impl IrohRaftService {
    /// Create a new Raft service
    pub fn new(shared_state: Arc<SharedNodeState>) -> Self {
        Self { shared_state }
    }
}

#[async_trait::async_trait]
impl IrohService for IrohRaftService {
    fn name(&self) -> &'static str {
        "raft"
    }

    async fn handle_call(&self, method: &str, payload: bytes::Bytes) -> BlixardResult<bytes::Bytes> {
        match method {
            "raft.message" => {
                // Handle Raft message
                // In a real implementation, this would deserialize the message
                // and forward it to the Raft manager
                Ok(bytes::Bytes::new())
            }
            _ => Err(crate::error::BlixardError::Internal {
                message: format!("Unknown Raft method: {}", method),
            }),
        }
    }

    fn methods(&self) -> Vec<&'static str> {
        vec!["raft.message"]
    }
}