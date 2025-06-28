//! Unified Raft transport adapter that can use either gRPC or Iroh
//!
//! This module provides a common interface for Raft message transport,
//! allowing seamless switching between gRPC and Iroh P2P transport.

use std::sync::Arc;
use tokio::sync::mpsc;
use raft::prelude::Message;

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::peer_connector::PeerConnector;
use crate::transport::iroh_raft_transport::IrohRaftTransport;
use crate::transport::config::{TransportConfig, RaftTransportPreference};
use crate::config_global;

/// Unified Raft transport that can use either gRPC or Iroh
pub enum RaftTransport {
    /// Traditional gRPC-based transport using PeerConnector
    Grpc(Arc<PeerConnector>),
    /// Iroh P2P transport
    Iroh(Arc<IrohRaftTransport>),
    /// Dual mode - uses both transports for gradual migration
    Dual {
        grpc: Arc<PeerConnector>,
        iroh: Arc<IrohRaftTransport>,
        prefer_iroh: bool,
    },
}

impl RaftTransport {
    /// Create a new Raft transport based on configuration
    pub async fn new(
        node: Arc<SharedNodeState>,
        raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
        transport_config: &TransportConfig,
    ) -> BlixardResult<Self> {
        match transport_config {
            TransportConfig::Grpc(_) => {
                let peer_connector = Arc::new(PeerConnector::new(node));
                Ok(RaftTransport::Grpc(peer_connector))
            }
            TransportConfig::Iroh(_) => {
                // Get Iroh endpoint from node
                let (endpoint, local_node_id) = node.get_iroh_endpoint().await?;
                
                let iroh_transport = Arc::new(IrohRaftTransport::new(
                    node,
                    endpoint,
                    local_node_id,
                    raft_rx_tx,
                ));
                
                // Start the Iroh transport
                iroh_transport.start().await?;
                
                Ok(RaftTransport::Iroh(iroh_transport))
            }
            TransportConfig::Dual { strategy, .. } => {
                // Create both transports
                let peer_connector = Arc::new(PeerConnector::new(node.clone()));
                
                let (endpoint, local_node_id) = node.get_iroh_endpoint().await?;
                let iroh_transport = Arc::new(IrohRaftTransport::new(
                    node,
                    endpoint,
                    local_node_id,
                    raft_rx_tx,
                ));
                
                // Start the Iroh transport
                iroh_transport.start().await?;
                
                // Check Raft transport preference
                let prefer_iroh = match strategy.raft_transport {
                    RaftTransportPreference::AlwaysGrpc => false,
                    RaftTransportPreference::AlwaysIroh => true,
                    RaftTransportPreference::Adaptive { .. } => {
                        // Default to gRPC for adaptive mode initially
                        false
                    }
                };
                
                Ok(RaftTransport::Dual {
                    grpc: peer_connector,
                    iroh: iroh_transport,
                    prefer_iroh,
                })
            }
        }
    }
    
    /// Send a Raft message to a peer
    pub async fn send_message(&self, to: u64, message: Message) -> BlixardResult<()> {
        match self {
            RaftTransport::Grpc(peer_connector) => {
                peer_connector.send_raft_message(to, message).await
            }
            RaftTransport::Iroh(iroh_transport) => {
                iroh_transport.send_message(to, message).await
            }
            RaftTransport::Dual { grpc, iroh, prefer_iroh } => {
                if *prefer_iroh {
                    // Try Iroh first, fall back to gRPC
                    match iroh.send_message(to, message.clone()).await {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            tracing::debug!("Iroh transport failed, falling back to gRPC: {}", e);
                            grpc.send_raft_message(to, message).await
                        }
                    }
                } else {
                    // Try gRPC first, fall back to Iroh
                    match grpc.send_raft_message(to, message.clone()).await {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            tracing::debug!("gRPC transport failed, falling back to Iroh: {}", e);
                            iroh.send_message(to, message).await
                        }
                    }
                }
            }
        }
    }
    
    /// Start connection maintenance (for gRPC transport)
    pub async fn start_maintenance(&self) {
        match self {
            RaftTransport::Grpc(peer_connector) => {
                peer_connector.clone().start_connection_maintenance().await;
            }
            RaftTransport::Iroh(_) => {
                // Iroh transport handles its own maintenance
            }
            RaftTransport::Dual { grpc, .. } => {
                grpc.clone().start_connection_maintenance().await;
            }
        }
    }
    
    /// Get transport metrics
    pub async fn get_metrics(&self) -> RaftTransportMetrics {
        match self {
            RaftTransport::Grpc(peer_connector) => {
                RaftTransportMetrics {
                    transport_type: "grpc".to_string(),
                    connections: 0, // TODO: Add connection count tracking
                    messages_sent: 0, // TODO: Implement in PeerConnector
                    messages_received: 0,
                }
            }
            RaftTransport::Iroh(iroh_transport) => {
                // TODO: Get metrics from IrohRaftTransport
                RaftTransportMetrics {
                    transport_type: "iroh".to_string(),
                    connections: 0,
                    messages_sent: 0,
                    messages_received: 0,
                }
            }
            RaftTransport::Dual { grpc, iroh, prefer_iroh } => {
                let grpc_connections = 0; // TODO: Add connection count tracking
                RaftTransportMetrics {
                    transport_type: format!("dual(prefer_{})", if *prefer_iroh { "iroh" } else { "grpc" }),
                    connections: grpc_connections,
                    messages_sent: 0,
                    messages_received: 0,
                }
            }
        }
    }
    
    /// Shutdown the transport
    pub async fn shutdown(&self) {
        match self {
            RaftTransport::Grpc(peer_connector) => {
                peer_connector.shutdown().await;
            }
            RaftTransport::Iroh(iroh_transport) => {
                iroh_transport.shutdown().await;
            }
            RaftTransport::Dual { grpc, iroh, .. } => {
                // Shutdown both transports
                let grpc_shutdown = grpc.shutdown();
                let iroh_shutdown = iroh.shutdown();
                tokio::join!(grpc_shutdown, iroh_shutdown);
            }
        }
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