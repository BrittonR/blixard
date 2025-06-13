//! Peer connection management for cluster nodes
//!
//! This module handles establishing and maintaining gRPC connections to peer nodes.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tonic::transport::Channel;

use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_client::ClusterServiceClient;
use crate::node_shared::{SharedNodeState, PeerInfo};

/// Buffered message for delayed sending
struct BufferedMessage {
    to: u64,
    message: raft::prelude::Message,
    attempts: u32,
}

/// Manages connections to peer nodes
pub struct PeerConnector {
    node: Arc<SharedNodeState>,
    connections: RwLock<HashMap<u64, ClusterServiceClient<Channel>>>,
    /// Messages buffered while waiting for connection
    message_buffer: Mutex<HashMap<u64, VecDeque<BufferedMessage>>>,
    /// Track connection attempts in progress
    connecting: Mutex<HashMap<u64, bool>>,
}

impl PeerConnector {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            node,
            connections: RwLock::new(HashMap::new()),
            message_buffer: Mutex::new(HashMap::new()),
            connecting: Mutex::new(HashMap::new()),
        }
    }
    
    /// Establish connection to a peer
    pub async fn connect_to_peer(&self, peer: &PeerInfo) -> BlixardResult<()> {
        // Mark that we're connecting to prevent duplicate attempts
        {
            let mut connecting = self.connecting.lock().await;
            if connecting.get(&peer.id).copied().unwrap_or(false) {
                tracing::debug!("Already connecting to peer {}", peer.id);
                return Ok(());
            }
            connecting.insert(peer.id, true);
        }
        
        let endpoint = format!("http://{}", peer.address);
        
        let result = match Channel::from_shared(endpoint.clone()) {
            Ok(endpoint) => {
                match endpoint.connect().await {
                    Ok(channel) => {
                        let client = ClusterServiceClient::new(channel);
                        
                        // Store the connection
                        let mut connections = self.connections.write().await;
                        connections.insert(peer.id, client.clone());
                        
                        // Update peer connection status
                        self.node.update_peer_connection(peer.id, true).await?;
                        
                        tracing::info!("Connected to peer {} at {}", peer.id, peer.address);
                        
                        // Send any buffered messages
                        self.send_buffered_messages(peer.id, client).await;
                        
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
        };
        
        // Clear connecting flag
        {
            let mut connecting = self.connecting.lock().await;
            connecting.remove(&peer.id);
        }
        
        result
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
    
    /// Send buffered messages after connection is established
    async fn send_buffered_messages(&self, peer_id: u64, mut client: ClusterServiceClient<Channel>) {
        use crate::proto::RaftMessageRequest;
        
        let mut buffer = self.message_buffer.lock().await;
        if let Some(messages) = buffer.remove(&peer_id) {
            tracing::info!("[PEER-CONN] Sending {} buffered messages to node {}", messages.len(), peer_id);
            
            for buffered in messages {
                match crate::raft_codec::serialize_message(&buffered.message) {
                    Ok(raft_data) => {
                        let request = RaftMessageRequest { raft_data };
                        
                        if let Err(e) = client.send_raft_message(request).await {
                            tracing::warn!("[PEER-CONN] Failed to send buffered message to {}: {}", peer_id, e);
                        } else {
                            tracing::debug!("[PEER-CONN] Successfully sent buffered message to {}, type: {:?}", 
                                peer_id, buffered.message.msg_type());
                        }
                    }
                    Err(e) => {
                        tracing::error!("[PEER-CONN] Failed to serialize buffered message: {}", e);
                    }
                }
            }
        }
    }
    
    /// Send Raft message to a peer
    pub async fn send_raft_message(self: &Arc<Self>, to: u64, message: raft::prelude::Message) -> BlixardResult<()> {
        use crate::proto::RaftMessageRequest;
        
        tracing::info!("[PEER-CONN] Sending Raft message to node {}, type: {:?}", to, message.msg_type());
        
        // Check if we have a connection
        if let Some(mut client) = self.get_connection(to).await {
            // Serialize the message
            let raft_data = crate::raft_codec::serialize_message(&message)?;
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
                    // Connection failed, mark as disconnected and buffer the message
                    self.node.update_peer_connection(to, false).await?;
                    self.buffer_message(to, message).await;
                    
                    // Try to reconnect asynchronously
                    if let Some(peer) = self.node.get_peer(to).await {
                        let self_clone = self.clone();
                        let peer_clone = peer.clone();
                        tokio::spawn(async move {
                            let _ = self_clone.connect_to_peer(&peer_clone).await;
                        });
                    }
                    
                    Err(BlixardError::ClusterError(format!(
                        "Failed to send Raft message to {}: {}", to, e
                    )))
                }
            }
        } else {
            // No connection, buffer the message
            tracing::info!("[PEER-CONN] No connection to node {}, buffering message", to);
            self.buffer_message(to, message).await;
            
            // Try to connect asynchronously
            if let Some(peer) = self.node.get_peer(to).await {
                tracing::info!("[PEER-CONN] Found peer info for node {}: {}, attempting connection", to, peer.address);
                
                // Clone self for async task
                let peer_connector = self.clone();
                let peer_info = peer.clone();
                
                // Spawn connection attempt
                tokio::spawn(async move {
                    if let Err(e) = peer_connector.connect_to_peer(&peer_info).await {
                        tracing::warn!("[PEER-CONN] Failed to connect to peer {}: {}", peer_info.id, e);
                    }
                });
                
                Ok(())  // Message is buffered, will be sent when connection is established
            } else {
                Err(BlixardError::ClusterError(format!(
                    "Unknown peer {}", to
                )))
            }
        }
    }
    
    /// Buffer a message for later delivery
    async fn buffer_message(&self, to: u64, message: raft::prelude::Message) {
        let mut buffer = self.message_buffer.lock().await;
        let queue = buffer.entry(to).or_insert_with(VecDeque::new);
        
        // Limit buffer size to prevent memory issues
        const MAX_BUFFERED_MESSAGES: usize = 100;
        if queue.len() >= MAX_BUFFERED_MESSAGES {
            tracing::warn!("[PEER-CONN] Message buffer full for node {}, dropping oldest message", to);
            queue.pop_front();
        }
        
        queue.push_back(BufferedMessage {
            to,
            message,
            attempts: 0,
        });
        
        tracing::debug!("[PEER-CONN] Buffered message for node {}, buffer size: {}", to, queue.len());
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