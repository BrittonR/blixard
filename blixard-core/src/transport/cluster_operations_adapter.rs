//! Adapter for cluster operations to integrate with SharedNodeState
//!
//! This module provides an implementation of ClusterOperations that delegates
//! to the existing SharedNodeState functionality.

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_cluster_service::{ClusterOperations, Task, TaskRequest, TaskStatus};
use crate::iroh_types::{NodeInfo, NodeState};
use crate::raft_manager::{RaftConfChange, ConfChangeType};
use async_trait::async_trait;
use std::sync::Arc;
use std::net::SocketAddr;
use tracing::{debug, error, info};

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
    async fn join_cluster(&self, node_id: u64, bind_address: String) -> BlixardResult<(bool, String, Vec<NodeInfo>, Vec<u64>)> {
        debug!("Join cluster request for node {} at {}", node_id, bind_address);
        
        // Parse the bind address
        let addr: SocketAddr = bind_address.parse().map_err(|e| BlixardError::Internal {
            message: format!("Invalid bind address: {}", e),
        })?;
        
        // Add peer to our peer list
        self.shared_state.add_peer(node_id, addr).await;
        
        // Propose configuration change through Raft
        let conf_change = RaftConfChange {
            change_type: ConfChangeType::AddNode,
            node_id,
            node_addr: Some(addr),
        };
        
        match self.shared_state.propose_conf_change(conf_change).await {
            Ok(_) => {
                info!("Node {} successfully joined cluster", node_id);
                
                // Get current cluster information
                let peers = self.shared_state.get_peers().await;
                let status = self.shared_state.get_raft_status().await?;
                
                // Get voters from Raft configuration
                // For now, we'll return all peer IDs as voters
                let voters: Vec<u64> = peers.iter().map(|p| p.id).collect();
                
                Ok((true, "Successfully joined cluster".to_string(), peers, voters))
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
        let conf_change = RaftConfChange {
            change_type: ConfChangeType::RemoveNode,
            node_id,
            node_addr: None,
        };
        
        match self.shared_state.propose_conf_change(conf_change).await {
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
        
        // Get all peers
        let mut nodes = self.shared_state.get_peers().await;
        
        // Add ourselves to the list
        let config = &self.shared_state.config;
        let self_info = NodeInfo {
            id: config.id,
            address: config.bind_address.to_string(),
            state: if raft_status.is_leader {
                NodeState::Leader
            } else {
                NodeState::Follower
            },
            p2p_node_id: String::new(), // TODO: Get from P2P manager
            p2p_addresses: vec![],
            p2p_relay_url: None,
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
            resources: crate::raft_manager::TaskRequirements {
                cpu_cores: task.cpu_cores as u8,
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
                    let output = result.output.unwrap_or_default();
                    let error = result.error.unwrap_or_default();
                    let execution_time_ms = result.end_time.saturating_sub(result.start_time);
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
            resources: crate::raft_manager::TaskRequirements {
                cpu_cores: task.cpu_cores as u8,
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