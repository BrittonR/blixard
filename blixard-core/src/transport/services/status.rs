//! Status query service implementation
//!
//! This service provides cluster and Raft status queries that work
//! over both gRPC and Iroh transports.

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    iroh_types::{
        ClusterStatusRequest, ClusterStatusResponse, NodeInfo, NodeState,
        // GetRaftStatusRequest and GetRaftStatusResponse are defined in iroh_status_service
    },
    metrics_otel::{metrics, Timer, attributes},
};
use async_trait::async_trait;
use std::sync::Arc;
// Removed tonic imports - using Iroh transport

/// Trait for status query operations
#[async_trait]
pub trait StatusService: Send + Sync {
    /// Get cluster status
    async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse>;
    
    // Raft status is now part of cluster status
}

/// Status service implementation
#[derive(Clone)]
pub struct StatusServiceImpl {
    pub node: Arc<SharedNodeState>,
}

impl StatusServiceImpl {
    /// Create a new status service instance
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self { node }
    }
}

#[async_trait]
impl StatusService for StatusServiceImpl {
    async fn get_cluster_status(&self) -> BlixardResult<ClusterStatusResponse> {
        // Get Raft status to determine leadership and term
        let raft_status = self.node.get_raft_status().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get Raft status: {}", e),
            })?;
        
        let leader_id = raft_status.leader_id.unwrap_or(0);
        let term = raft_status.term;
        
        // Get all configured node IDs from peers
        let peers = self.node.get_peers().await;
        let mut node_ids = vec![self.node.get_id()];
        node_ids.extend(peers.iter().map(|p| p.id));
        
        // Get peers for address information
        let peers = self.node.get_peers().await;
        
        // Build node list from the authoritative Raft configuration
        let mut nodes = Vec::new();
        
        for node_id in node_ids {
            let (address, state) = if node_id == self.node.get_id() {
                // Self
                let state = if raft_status.is_leader {
                    NodeState::NodeStateLeader
                } else if raft_status.state == "candidate" {
                    NodeState::NodeStateCandidate
                } else {
                    NodeState::NodeStateFollower
                };
                (self.node.get_bind_addr().to_string(), state)
            } else {
                // Peer - get address from peers list
                let peer_addr = peers.iter()
                    .find(|p| p.id == node_id)
                    .map(|p| p.address.clone())
                    .unwrap_or_else(|| format!("unknown-{}", node_id));
                    
                // For peers, we can determine if they're the leader
                let state = if Some(node_id) == raft_status.leader_id {
                    NodeState::NodeStateLeader
                } else {
                    // We don't have enough info to know if they're candidates or followers
                    NodeState::NodeStateFollower
                };
                (peer_addr, state)
            };
            
            // Get P2P info if available
            let (p2p_node_id, p2p_addresses, p2p_relay_url) = 
                if let Some(p2p_manager) = self.node.get_p2p_manager().await {
                    if node_id == self.node.get_id() {
                        // Self - get our own P2P info
                        if let Ok(node_addr) = p2p_manager.get_node_addr().await {
                            (
                                node_addr.node_id.to_string(),
                                node_addr.direct_addresses.iter()
                                    .map(|addr| addr.to_string())
                                    .collect(),
                                node_addr.relay_url.map(|url| url.to_string())
                                    .unwrap_or_default(),
                            )
                        } else {
                            (String::new(), vec![], String::new())
                        }
                    } else {
                        // Peer - would need to query them for their P2P info
                        (String::new(), Vec::new(), String::new())
                    }
                } else {
                    (String::new(), Vec::new(), String::new())
                };
            
            nodes.push(NodeInfo {
                id: node_id,
                address,
                state: state.into(),
                p2p_node_id,
                p2p_addresses,
                p2p_relay_url,
            });
        }
        
        Ok(ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        })
    }
    
}

/// Iroh protocol handler for status service
pub struct StatusProtocolHandler {
    service: StatusServiceImpl,
}

impl StatusProtocolHandler {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        Self {
            service: StatusServiceImpl::new(node),
        }
    }
    
    /// Handle a status request over Iroh
    pub async fn handle_request(
        &self,
        _connection: iroh::endpoint::Connection,
        request_type: StatusRequestType,
    ) -> BlixardResult<()> {
        // TODO: Implement proper protocol handling
        match request_type {
            StatusRequestType::ClusterStatus => {
                // Handle cluster status request
            }
            StatusRequestType::RaftStatus => {
                // Handle Raft status request
            }
        }
        
        Err(BlixardError::NotImplemented {
            feature: "Iroh status protocol handler".to_string(),
        })
    }
}

/// Types of status requests
pub enum StatusRequestType {
    ClusterStatus,
    RaftStatus,
}