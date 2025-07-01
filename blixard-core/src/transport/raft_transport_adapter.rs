//! Unified Raft transport adapter that can use either gRPC or Iroh
//!
//! This module provides a common interface for Raft message transport,
//! allowing seamless switching between gRPC and Iroh P2P transport.

use std::sync::Arc;
use tokio::sync::mpsc;
use raft::prelude::Message;

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::transport::config::TransportConfig;
use crate::transport::iroh_peer_connector::IrohPeerConnector;
use crate::transport::iroh_raft_transport::IrohRaftTransport;

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
        // Get Iroh endpoint from node
        let (endpoint, local_node_id) = node.get_iroh_endpoint().await?;
        
        let iroh_transport = Arc::new(IrohRaftTransport::new(
            node,
            endpoint.clone(),
            local_node_id,
            raft_rx_tx,
        ));
        
        // Start the Iroh transport
        iroh_transport.start().await?;
        
        Ok(RaftTransport {
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
    pub async fn get_metrics(&self) -> RaftTransportMetrics {
        // TODO: Get metrics from IrohRaftTransport
        RaftTransportMetrics {
            transport_type: "iroh".to_string(),
            connections: 0,
            messages_sent: 0,
            messages_received: 0,
        }
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

/// Metrics for Raft transport
#[derive(Debug, Clone)]
pub struct RaftTransportMetrics {
    pub transport_type: String,
    pub connections: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
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