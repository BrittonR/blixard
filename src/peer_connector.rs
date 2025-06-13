//! Peer connection management for cluster nodes
//!
//! This module handles establishing and maintaining gRPC connections to peer nodes.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Channel;

use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_client::ClusterServiceClient;
use crate::node_shared::{SharedNodeState, PeerInfo};

/// Manages connections to peer nodes
pub struct PeerConnector {
    node: Arc<SharedNodeState>,
    connections: RwLock<HashMap<u64, ClusterServiceClient<Channel>>>,
}

impl PeerConnector {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            node,
            connections: RwLock::new(HashMap::new()),
        }
    }
    
    /// Establish connection to a peer
    pub async fn connect_to_peer(&self, peer: &PeerInfo) -> BlixardResult<()> {
        let endpoint = format!("http://{}", peer.address);
        
        match Channel::from_shared(endpoint.clone()) {
            Ok(endpoint) => {
                match endpoint.connect().await {
                    Ok(channel) => {
                        let client = ClusterServiceClient::new(channel);
                        
                        // Store the connection
                        let mut connections = self.connections.write().await;
                        connections.insert(peer.id, client);
                        
                        // Update peer connection status
                        self.node.update_peer_connection(peer.id, true).await?;
                        
                        tracing::info!("Connected to peer {} at {}", peer.id, peer.address);
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!("Failed to connect to peer {} at {}: {}", peer.id, peer.address, e);
                        self.node.update_peer_connection(peer.id, false).await?;
                        Err(BlixardError::ClusterError(format!(
                            "Failed to connect to peer {}: {}", peer.id, e
                        )))
                    }
                }
            }
            Err(e) => {
                Err(BlixardError::ClusterError(format!(
                    "Invalid peer address {}: {}", peer.address, e
                )))
            }
        }
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_from_peer(&self, peer_id: u64) -> BlixardResult<()> {
        let mut connections = self.connections.write().await;
        connections.remove(&peer_id);
        
        self.node.update_peer_connection(peer_id, false).await?;
        
        tracing::info!("Disconnected from peer {}", peer_id);
        Ok(())
    }
    
    /// Get connection to a peer
    pub async fn get_connection(&self, peer_id: u64) -> Option<ClusterServiceClient<Channel>> {
        let connections = self.connections.read().await;
        connections.get(&peer_id).cloned()
    }
    
    /// Send Raft message to a peer
    pub async fn send_raft_message(&self, to: u64, message: raft::prelude::Message) -> BlixardResult<()> {
        use crate::proto::RaftMessageRequest;
        
        // Serialize the message
        let raft_data = crate::raft_codec::serialize_message(&message)?;
        
        // Get connection to peer
        if let Some(mut client) = self.get_connection(to).await {
            let request = RaftMessageRequest { raft_data };
            
            match client.send_raft_message(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        Ok(())
                    } else {
                        Err(BlixardError::ClusterError(format!(
                            "Failed to send Raft message to {}: {}", to, resp.error
                        )))
                    }
                }
                Err(e) => {
                    // Connection failed, mark as disconnected
                    self.node.update_peer_connection(to, false).await?;
                    Err(BlixardError::ClusterError(format!(
                        "Failed to send Raft message to {}: {}", to, e
                    )))
                }
            }
        } else {
            // Try to reconnect
            if let Some(peer) = self.node.get_peer(to).await {
                self.connect_to_peer(&peer).await?;
                
                // Retry sending
                if let Some(mut client) = self.get_connection(to).await {
                    let request = RaftMessageRequest { raft_data };
                    
                    match client.send_raft_message(request).await {
                        Ok(response) => {
                            let resp = response.into_inner();
                            if resp.success {
                                Ok(())
                            } else {
                                Err(BlixardError::ClusterError(format!(
                                    "Failed to send Raft message to {}: {}", to, resp.error
                                )))
                            }
                        }
                        Err(e) => {
                            Err(BlixardError::ClusterError(format!(
                                "Failed to send Raft message to {}: {}", to, e
                            )))
                        }
                    }
                } else {
                    Err(BlixardError::ClusterError(format!(
                        "No connection to peer {}", to
                    )))
                }
            } else {
                Err(BlixardError::ClusterError(format!(
                    "Unknown peer {}", to
                )))
            }
        }
    }
    
    /// Start background task to maintain connections
    pub fn start_connection_maintenance(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                // Get all peers
                let peers = self.node.get_peers().await;
                
                // Check each peer connection
                for peer in peers {
                    if !peer.is_connected {
                        // Try to connect
                        let _ = self.connect_to_peer(&peer).await;
                    }
                }
            }
        });
    }
}