//! Unified Raft transport adapter that can use either gRPC or Iroh
//!
//! This module provides a common interface for Raft message transport,
//! allowing seamless switching between gRPC and Iroh P2P transport.

use raft::prelude::Message;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::BlixardResult;
use crate::node_shared::SharedNodeState;
use crate::transport::config::TransportConfig;
use crate::transport::iroh_raft_transport::{IrohRaftTransport, IrohRaftMetrics};
use iroh::{Endpoint, SecretKey};
use rand;

/// Unified Raft transport using Iroh P2P
#[derive(Clone)]
pub struct RaftTransport {
    /// Iroh P2P transport
    iroh: Arc<IrohRaftTransport>,
    /// Endpoint for creating connections
    endpoint: Arc<iroh::endpoint::Endpoint>,
}

impl RaftTransport {
    /// Create a new Raft transport using Iroh
    pub async fn new(
        node: Arc<SharedNodeState>,
        raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
        _transport_config: &TransportConfig,
    ) -> BlixardResult<Self> {
        // Create Iroh endpoint for Raft communication
        let secret_key = SecretKey::generate(rand::thread_rng());
        
        // Use Raft-specific ALPN
        let raft_alpn = b"blixard-raft".to_vec();
        
        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![raft_alpn])
            .bind()
            .await
            .map_err(|e| crate::error::BlixardError::Internal {
                message: format!("Failed to create Raft endpoint: {}", e),
            })?;

        let local_node_id = endpoint.node_id();
        tracing::info!("Created Raft transport with Iroh node ID: {}", local_node_id);
        
        // Store the endpoint in SharedNodeState for other components to access
        node.set_iroh_endpoint(Some(endpoint.clone())).await;
        
        // Create the IrohRaftTransport
        let iroh_transport = Arc::new(IrohRaftTransport::new(
            node,
            endpoint.clone(),
            local_node_id,
            raft_rx_tx,
        ));
        
        // Start the transport to handle connections
        iroh_transport.start().await?;
        
        Ok(Self {
            iroh: iroh_transport,
            endpoint: Arc::new(endpoint),
        })
    }

    /// Send a Raft message to a peer
    pub async fn send_message(&self, to: u64, message: Message) -> BlixardResult<()> {
        self.iroh.send_message(to, message).await
    }

    /// Start connection maintenance
    pub async fn start_maintenance(&self) {
        // Iroh transport handles its own maintenance
    }

    /// Get transport metrics
    pub async fn get_metrics(&self) -> IrohRaftMetrics {
        // Get actual metrics from IrohRaftTransport
        self.iroh.get_metrics().await
    }

    /// Shutdown the transport
    pub async fn shutdown(&self) {
        self.iroh.shutdown().await;
    }

    /// Get the Iroh endpoint
    pub fn endpoint(&self) -> &Arc<iroh::endpoint::Endpoint> {
        &self.endpoint
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_creation() {
        // Test that we can create each transport type
        // This would require a test configuration and mock node state
    }
}
