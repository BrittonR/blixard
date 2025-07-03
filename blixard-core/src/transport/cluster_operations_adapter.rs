//! Adapter for cluster operations to integrate with SharedNodeState
//!
//! This module provides an implementation of ClusterOperations that delegates
//! to the existing SharedNodeState functionality.

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_cluster_service::{ClusterOperations, Task, TaskRequest, TaskStatus};
use crate::iroh_types::{NodeInfo, NodeState};
use crate::raft_manager::ConfChangeType;
use async_trait::async_trait;
use std::sync::Arc;
use std::net::SocketAddr;
use tracing::{debug, error, info, warn};

/// Adapter that implements ClusterOperations using SharedNodeState functionality
pub struct ClusterOperationsAdapter {
    shared_state: Arc<SharedNodeState>,
}

impl ClusterOperationsAdapter {
    pub fn new(shared_state: Arc<SharedNodeState>) -> Self {
        Self { shared_state }
    }
}

#[async_trait]
impl ClusterOperations for ClusterOperationsAdapter {
    async fn join_cluster(&self, node_id: u64, bind_address: String, p2p_node_id: Option<String>, p2p_addresses: Vec<String>, p2p_relay_url: Option<String>) -> BlixardResult<(bool, String, Vec<NodeInfo>, Vec<u64>)> {
        debug!("Join cluster request for node {} at {} with P2P info: {:?}", node_id, bind_address, p2p_node_id);
        
        // Parse the bind address
        let addr: SocketAddr = bind_address.parse().map_err(|e| BlixardError::Internal {
            message: format!("Invalid bind address: {}", e),
        })?;
        
        // Add peer to our peer list with P2P info (only if it doesn't already exist)
        if self.shared_state.get_peer(node_id).await.is_none() {
            if p2p_node_id.is_some() || !p2p_addresses.is_empty() {
                self.shared_state.add_peer_with_p2p(node_id, addr.to_string(), p2p_node_id.clone(), p2p_addresses.clone(), p2p_relay_url.clone()).await?;
            } else {
                self.shared_state.add_peer(node_id, addr.to_string()).await?;
            }
        } else {
            debug!("Peer {} already exists, updating P2P info", node_id);
            // Update P2P info if provided
            if p2p_node_id.is_some() || !p2p_addresses.is_empty() {
                // Remove and re-add to update P2P info
                self.shared_state.remove_peer(node_id).await;
                self.shared_state.add_peer_with_p2p(node_id, addr.to_string(), p2p_node_id.clone(), p2p_addresses.clone(), p2p_relay_url.clone()).await?;
            }
        }
        
        // If the joining node has P2P info, try to connect
        if let Some(ref p2p_id) = p2p_node_id {
            if let Ok(node_id_parsed) = p2p_id.parse::<iroh::NodeId>() {
                if let Some(p2p_manager) = self.shared_state.get_p2p_manager().await {
                    // Create NodeAddr from the P2P info
                    let mut node_addr = iroh::NodeAddr::new(node_id_parsed);
                    
                    // Add relay URL if available
                    if let Some(ref relay_url) = p2p_relay_url {
                        if let Ok(relay) = relay_url.parse() {
                            node_addr = node_addr.with_relay_url(relay);
                        }
                    }
                    
                    // Add direct addresses
                    for addr_str in &p2p_addresses {
                        if let Ok(addr) = addr_str.parse() {
                            node_addr = node_addr.with_direct_addresses([addr]);
                        }
                    }
                    
                    // Try to connect
                    info!("Attempting to connect to P2P peer {}", node_id);
                    if let Err(e) = p2p_manager.connect_p2p_peer(node_id, &node_addr).await {
                        warn!("Failed to establish P2P connection to node {}: {}", node_id, e);
                    }
                }
            }
        }
        
        // Propose configuration change through Raft
        match self.shared_state.propose_conf_change(ConfChangeType::AddNode, node_id, addr.to_string()).await {
            Ok(_) => {
                info!("Node {} successfully joined cluster", node_id);
                
                // Get current cluster information
                let mut peers = self.shared_state.get_peers().await;
                let status = self.shared_state.get_raft_status().await?;
                
                // Add our own info to the peers list
                let our_id = self.shared_state.get_id();
                let our_addr = self.shared_state.get_bind_addr().to_string();
                
                // Get our P2P info if available
                let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) = if let Some(node_addr) = self.shared_state.get_p2p_node_addr().await {
                    let p2p_id = node_addr.node_id.to_string();
                    let p2p_addrs: Vec<String> = node_addr.direct_addresses().map(|a| a.to_string()).collect();
                    let relay_url = node_addr.relay_url().map(|u| u.to_string());
                    (Some(p2p_id), p2p_addrs, relay_url)
                } else {
                    (None, Vec::new(), None)
                };
                
                // Create our peer info
                let our_peer_info = crate::node_shared::PeerInfo {
                    id: our_id,
                    address: our_addr,
                    is_connected: true,
                    p2p_node_id: our_p2p_node_id.clone(),
                    p2p_addresses: our_p2p_addresses.clone(),
                    p2p_relay_url: our_p2p_relay_url.clone(),
                };
                
                // Add ourselves to the peer list if not already there
                if !peers.iter().any(|p| p.id == our_id) {
                    peers.push(our_peer_info);
                }
                
                // Convert PeerInfo to NodeInfo
                let node_infos: Vec<NodeInfo> = peers.iter().map(|p| NodeInfo {
                    id: p.id,
                    address: p.address.clone(),
                    state: 0, // NodeStateUnknown
                    p2p_node_id: p.p2p_node_id.clone().unwrap_or_default(),
                    p2p_addresses: p.p2p_addresses.clone(),
                    p2p_relay_url: p.p2p_relay_url.clone().unwrap_or_default(),
                }).collect();
                
                // Get voters from Raft configuration
                // For now, we'll return all peer IDs as voters
                let voters: Vec<u64> = peers.iter().map(|p| p.id).collect();
                
                Ok((true, "Successfully joined cluster".to_string(), node_infos, voters))
            }
            Err(e) => {
                error!("Failed to join cluster: {}", e);
                Ok((false, format!("Failed to join cluster: {}", e), vec![], vec![]))
            }
        }
    }
    
    async fn leave_cluster(&self, node_id: u64) -> BlixardResult<(bool, String)> {
        debug!("Leave cluster request for node {}", node_id);
        
        // Propose configuration change through Raft
        match self.shared_state.propose_conf_change(ConfChangeType::RemoveNode, node_id, String::new()).await {
            Ok(_) => {
                info!("Node {} successfully left cluster", node_id);
                
                // Remove peer from our peer list
                self.shared_state.remove_peer(node_id).await;
                
                Ok((true, "Successfully left cluster".to_string()))
            }
            Err(e) => {
                error!("Failed to leave cluster: {}", e);
                Ok((false, format!("Failed to leave cluster: {}", e)))
            }
        }
    }
    
    async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<NodeInfo>, u64)> {
        debug!("Get cluster status request");
        
        // Get Raft status
        let raft_status = self.shared_state.get_raft_status().await?;
        
        // Get all peers and convert to NodeInfo
        let peers = self.shared_state.get_peers().await;
        let mut nodes: Vec<NodeInfo> = peers.iter().map(|p| NodeInfo {
            id: p.id,
            address: p.address.clone(),
            state: 0, // NodeStateUnknown
            p2p_node_id: p.p2p_node_id.clone().unwrap_or_default(),
            p2p_addresses: p.p2p_addresses.clone(),
            p2p_relay_url: p.p2p_relay_url.clone().unwrap_or_default(),
        }).collect();
        
        // Add ourselves to the list
        let config = &self.shared_state.config;
        let self_info = NodeInfo {
            id: config.id,
            address: config.bind_addr.to_string(),
            state: if raft_status.is_leader { 3 } else { 1 }, // Leader = 3, Follower = 1
            p2p_node_id: String::new(), // TODO: Get from P2P manager
            p2p_addresses: vec![],
            p2p_relay_url: String::new(),
        };
        nodes.push(self_info);
        
        // Sort by ID for consistent ordering
        nodes.sort_by_key(|n| n.id);
        
        let leader_id = raft_status.leader_id.unwrap_or(0);
        let term = raft_status.term;
        
        Ok((leader_id, nodes, term))
    }
    
    async fn submit_task(&self, task: TaskRequest) -> BlixardResult<(bool, String, u64)> {
        debug!("Submit task request: {}", task.task_id);
        
        // Convert TaskRequest to TaskSpec
        let task_spec = crate::raft_manager::TaskSpec {
            command: task.command,
            args: task.args,
            resources: crate::raft_manager::ResourceRequirements {
                cpu_cores: task.cpu_cores,
                memory_mb: task.memory_mb,
                disk_gb: task.disk_gb,
                required_features: task.required_features,
            },
            timeout_secs: task.timeout_secs,
        };
        
        match self.shared_state.submit_task(&task.task_id, task_spec).await {
            Ok(assigned_node) => {
                info!("Task submitted successfully, assigned to node {}", assigned_node);
                Ok((true, "Task submitted successfully".to_string(), assigned_node))
            }
            Err(e) => {
                error!("Failed to submit task: {}", e);
                Ok((false, format!("Failed to submit task: {}", e), 0))
            }
        }
    }
    
    async fn get_task_status(&self, task_id: String) -> BlixardResult<Option<(TaskStatus, String, String, u64)>> {
        debug!("Get task status request for task {}", task_id);
        
        match self.shared_state.get_task_status(&task_id).await {
            Ok(Some((status_str, result))) => {
                let status = match status_str.as_str() {
                    "running" => TaskStatus::Running,
                    "completed" => TaskStatus::Completed,
                    "failed" => TaskStatus::Failed,
                    _ => TaskStatus::Unknown,
                };
                
                let (output, error, execution_time_ms) = if let Some(result) = result {
                    let output = result.output.clone();
                    let error = result.error.unwrap_or_default();
                    let execution_time_ms = result.execution_time_ms;
                    (output, error, execution_time_ms)
                } else {
                    (String::new(), String::new(), 0)
                };
                
                Ok(Some((status, output, error, execution_time_ms)))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Failed to get task status: {}", e);
                Err(e)
            }
        }
    }
    
    async fn propose_task(&self, task: Task) -> BlixardResult<(bool, String)> {
        debug!("Propose task request: {}", task.id);
        
        // Convert to TaskSpec
        let task_spec = crate::raft_manager::TaskSpec {
            command: task.command,
            args: task.args,
            resources: crate::raft_manager::ResourceRequirements {
                cpu_cores: task.cpu_cores,
                memory_mb: task.memory_mb,
                disk_gb: 0, // Not included in the simple Task type
                required_features: vec![], // Not included in the simple Task type
            },
            timeout_secs: 60, // Default timeout
        };
        
        // Use submit_task which goes through Raft
        match self.shared_state.submit_task(&task.id, task_spec).await {
            Ok(_) => {
                info!("Task proposed successfully");
                Ok((true, "Task proposed successfully".to_string()))
            }
            Err(e) => {
                error!("Failed to propose task: {}", e);
                Ok((false, format!("Failed to propose task: {}", e)))
            }
        }
    }
}