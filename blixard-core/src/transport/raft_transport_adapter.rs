//! Unified Raft transport adapter that can use either gRPC or Iroh
//!
//! This module provides a common interface for Raft message transport,
//! allowing seamless switching between gRPC and Iroh P2P transport.

use raft::prelude::Message;
use std::sync::Arc;
use tokio::sync::mpsc;
use base64::prelude::*;

use crate::error::BlixardResult;
use crate::node_shared::SharedNodeState;
use crate::transport::config::TransportConfig;
use crate::transport::iroh_raft_transport::{IrohRaftTransport, IrohRaftMetrics};

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
        // Use the IrohEndpointManager for proper endpoint initialization
        use crate::transport::iroh_endpoint_manager::IrohEndpointManager;
        
        // Define ALPNs for both Raft and regular services
        let alpns = vec![
            b"blixard-raft".to_vec(),
            crate::transport::BLIXARD_ALPN.to_vec(),
        ];
        
        // Create endpoint manager with node ID-based port for predictable addressing
        // Use port 50000 + node_id to ensure each node gets a unique, predictable port
        let port = 50000 + (node.config.id as u16);
        let endpoint_manager = IrohEndpointManager::new_with_port(alpns, port).await?;
        let endpoint = endpoint_manager.endpoint().clone();
        let local_node_id = endpoint.node_id();
        
        tracing::info!(
            "Created Raft transport with Iroh node ID: {}",
            local_node_id
        );
        tracing::info!(
            "Node ticket for discovery: {}",
            endpoint_manager.node_ticket()
        );
        
        // Store the endpoint in SharedNodeState for other components to access
        node.set_iroh_endpoint(Some(endpoint.clone())).await;
        
        // Save node information to registry file in proper NodeRegistryEntry format
        let registry_path = format!("{}/node-{}-registry.json", node.config.data_dir, node.config.id);
        Self::save_node_registry(&endpoint_manager, &node, &registry_path).await?;
        
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

    /// Save node registry in the proper NodeRegistryEntry format
    async fn save_node_registry(
        endpoint_manager: &crate::transport::iroh_endpoint_manager::IrohEndpointManager,
        node: &Arc<SharedNodeState>,
        registry_path: &str,
    ) -> BlixardResult<()> {
        use tokio::fs;
        
        let node_addr = endpoint_manager.node_addr();
        let cluster_node_id = node.config.id;
        let bind_addr = node.get_bind_addr();
        
        // Create proper NodeRegistryEntry format
        let registry_entry = serde_json::json!({
            "cluster_node_id": cluster_node_id,
            "iroh_node_id": BASE64_STANDARD.encode(node_addr.node_id.as_bytes()),
            "direct_addresses": node_addr.direct_addresses()
                .map(|addr| addr.to_string())
                .collect::<Vec<_>>(),
            "relay_url": node_addr.relay_url().map(|url| url.to_string()),
            "address": bind_addr,
        });
        
        let content = serde_json::to_string_pretty(&registry_entry)
            .map_err(|e| crate::error::BlixardError::Serialization {
                operation: "serialize registry entry".to_string(),
                source: Box::new(e),
            })?;
        
        fs::write(registry_path, content).await
            .map_err(|e| crate::error::BlixardError::Storage {
                operation: format!("write node registry to {}", registry_path),
                source: Box::new(e),
            })?;
        
        tracing::info!("Node registry (NodeRegistryEntry format) saved to: {}", registry_path);
        Ok(())
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
