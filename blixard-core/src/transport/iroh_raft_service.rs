//! Iroh service for handling Raft consensus messages
//!
//! This service receives Raft messages over Iroh P2P and forwards them to the Raft manager.

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_service::IrohService;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Service for handling Raft consensus messages over Iroh
pub struct IrohRaftService {
    shared_state: Arc<SharedNodeState>,
}

impl IrohRaftService {
    /// Create a new Raft service
    pub fn new(shared_state: Arc<SharedNodeState>) -> Self {
        Self { shared_state }
    }

    /// Extract the sender node ID from an incoming Raft message
    /// This is a fallback method when we can't determine the sender from connection info
    fn extract_sender_from_message(&self, message: &raft::prelude::Message) -> Option<u64> {
        // The 'from' field in the Raft message should contain the sender's cluster node ID
        if message.from != 0 {
            Some(message.from)
        } else {
            warn!("Raft message has from: 0, cannot determine sender");
            None
        }
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
                debug!("RAFT_SERVICE: Received raft.message call with {} bytes", payload.len());

                // Deserialize the Raft message
                let message = match crate::raft_codec::deserialize_message(&payload) {
                    Ok(msg) => {
                        info!(
                            "RAFT_SERVICE: Successfully deserialized message type {:?} from {} to {}",
                            msg.msg_type(), msg.from, msg.to
                        );
                        msg
                    }
                    Err(e) => {
                        error!("RAFT_SERVICE: Failed to deserialize Raft message: {}", e);
                        return Err(BlixardError::Internal {
                            message: format!("Failed to deserialize Raft message: {}", e),
                        });
                    }
                };

                // Determine the sender node ID
                let sender_id = match self.extract_sender_from_message(&message) {
                    Some(id) => {
                        debug!("RAFT_SERVICE: Extracted sender ID {} from message", id);
                        id
                    }
                    None => {
                        error!("RAFT_SERVICE: Cannot determine sender ID from message");
                        return Err(BlixardError::Internal {
                            message: "Cannot determine sender ID from Raft message".to_string(),
                        });
                    }
                };

                // Get the Raft message channel from SharedNodeState
                let message_tx = match self.shared_state.get_raft_message_tx().await {
                    Some(tx) => {
                        debug!("RAFT_SERVICE: Got Raft message channel");
                        tx
                    }
                    None => {
                        error!("RAFT_SERVICE: Raft message channel not available - RaftManager not initialized");
                        return Err(BlixardError::NotInitialized {
                            component: "RaftManager message channel".to_string(),
                        });
                    }
                };

                // Forward the message to the RaftManager
                info!(
                    "RAFT_SERVICE: Forwarding {:?} message from node {} to RaftManager",
                    message.msg_type(), sender_id
                );

                if let Err(_) = message_tx.send((sender_id, message)) {
                    error!("RAFT_SERVICE: Failed to send message to RaftManager - channel closed");
                    return Err(BlixardError::Internal {
                        message: "Failed to forward message to RaftManager - channel closed".to_string(),
                    });
                }

                info!("RAFT_SERVICE: Successfully forwarded message to RaftManager");

                // Return success response
                Ok(bytes::Bytes::from_static(b"OK"))
            }
            _ => {
                warn!("RAFT_SERVICE: Unknown Raft method: {}", method);
                Err(BlixardError::Internal {
                    message: format!("Unknown Raft method: {}", method),
                })
            }
        }
    }

    fn methods(&self) -> Vec<&'static str> {
        vec!["raft.message"]
    }
}