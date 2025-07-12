//! Adapter for cluster operations to integrate with SharedNodeState
//!
//! This module provides an implementation of ClusterOperations that delegates
//! to the existing SharedNodeState functionality.

use crate::error::{BlixardError, BlixardResult};
use crate::iroh_types::NodeInfo;
use crate::node_shared::SharedNodeState;
use crate::raft_manager::ConfChangeType;
use crate::transport::iroh_cluster_service::{ClusterOperations, Task, TaskRequest, TaskStatus};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
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
    async fn join_cluster(
        &self,
        node_id: u64,
        bind_address: String,
        p2p_node_id: Option<String>,
        p2p_addresses: Vec<String>,
        p2p_relay_url: Option<String>,
    ) -> BlixardResult<(bool, String, Vec<NodeInfo>, Vec<u64>)> {
        debug!(
            "Join cluster request for node {} at {} with P2P info: {:?}",
            node_id, bind_address, p2p_node_id
        );

        // Parse the bind address
        let addr: SocketAddr = bind_address.parse().map_err(|e| BlixardError::Internal {
            message: format!("Invalid bind address: {}", e),
        })?;

        // Get our own info upfront
        let our_id = self.shared_state.get_id();
        let our_addr = self.shared_state.get_bind_addr().to_string();

        // Get our P2P info if available
        let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) =
            // TODO: Fix API mismatch - get_p2p_node_addr() returns Option<String> not iroh::NodeAddr
            if let Some(node_addr_str) = self.shared_state.get_p2p_node_addr() {
                // For now, return the string as node ID and empty addresses
                (Some(node_addr_str), Vec::new(), None)
            } else {
                (None, Vec::new(), None)
            };

        // Add peer to our peer list with P2P info (only if it doesn't already exist)
        if self.shared_state.get_peer(node_id).is_none() {
            if p2p_node_id.is_some() || !p2p_addresses.is_empty() {
                let peer_info = crate::p2p_manager::PeerInfo {
                    node_id: p2p_node_id.clone().unwrap_or_else(|| node_id.to_string()),
                    address: addr.to_string(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: std::collections::HashMap::new(),
                    connection_quality: crate::p2p_manager::ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    p2p_node_id: p2p_node_id.clone(),
                    p2p_addresses: p2p_addresses.clone(),
                    p2p_relay_url: p2p_relay_url.clone(),
                };
                self.shared_state.add_peer_with_p2p(node_id, peer_info);
            } else {
                let peer_info = crate::p2p_manager::PeerInfo {
                    node_id: node_id.to_string(),
                    address: addr.to_string(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: std::collections::HashMap::new(),
                    connection_quality: crate::p2p_manager::ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    p2p_node_id: None,
                    p2p_addresses: Vec::new(),
                    p2p_relay_url: None,
                };
                self.shared_state.add_peer(node_id, peer_info);
            }
        } else {
            debug!("Peer {} already exists, updating P2P info", node_id);
            // Update P2P info if provided
            if p2p_node_id.is_some() || !p2p_addresses.is_empty() {
                // Remove and re-add to update P2P info
                self.shared_state.remove_peer(node_id);
                let peer_info = crate::p2p_manager::PeerInfo {
                    node_id: p2p_node_id.clone().unwrap_or_else(|| node_id.to_string()),
                    address: addr.to_string(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: std::collections::HashMap::new(),
                    connection_quality: crate::p2p_manager::ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    p2p_node_id: p2p_node_id.clone(),
                    p2p_addresses: p2p_addresses.clone(),
                    p2p_relay_url: p2p_relay_url.clone(),
                };
                self.shared_state.add_peer_with_p2p(node_id, peer_info);
            }
        }

        // If the joining node has P2P info, try to connect
        if let Some(ref p2p_id) = p2p_node_id {
            if let Ok(node_id_parsed) = p2p_id.parse::<iroh::NodeId>() {
                if let Some(p2p_manager) = self.shared_state.get_p2p_manager() {
                    // Create NodeAddr from the P2P info
                    let mut node_addr = iroh::NodeAddr::new(node_id_parsed);

                    // Add relay URL if available
                    if let Some(ref relay_url) = p2p_relay_url {
                        if let Ok(relay) = relay_url.parse() {
                            node_addr = node_addr.with_relay_url(relay);
                        }
                    }

                    // Collect all valid addresses and add them at once
                    let addrs: Vec<SocketAddr> = p2p_addresses
                        .iter()
                        .filter_map(|addr_str| addr_str.parse().ok())
                        .collect();
                    if !addrs.is_empty() {
                        node_addr = node_addr.with_direct_addresses(addrs);
                    }

                    // Try to connect
                    info!("Attempting to connect to P2P peer {}", node_id);
                    if let Err(e) = p2p_manager.connect_p2p_peer(node_id, &node_addr).await {
                        warn!(
                            "Failed to establish P2P connection to node {}: {}",
                            node_id, e
                        );
                    }
                }
            }
        }

        // Propose configuration change through Raft with P2P info
        match self
            .shared_state
            .propose_conf_change_with_p2p(
                ConfChangeType::AddNode,
                node_id,
                addr.to_string(),
                p2p_node_id.clone(),
                p2p_addresses.clone(),
                p2p_relay_url.clone(),
            )
            .await
        {
            Ok(_) => {
                info!(
                    "Node {} configuration change proposed and committed",
                    node_id
                );

                // Wait for the configuration change to be reflected in Raft's configuration
                // The propose_conf_change already waits for the change to be committed, but
                // we need to ensure the new node appears in the voter list
                let mut retries = 0;
                let max_retries = 10;
                let mut found_node = false;

                while retries < max_retries && !found_node {
                    // Get the authoritative list of voters from Raft
                    let voters = self.shared_state.get_current_voters().await?;
                    found_node = voters.contains(&node_id);

                    if !found_node {
                        debug!(
                            "Waiting for node {} to appear in Raft configuration, attempt {}/{}",
                            node_id,
                            retries + 1,
                            max_retries
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        retries += 1;
                    }
                }

                if !found_node {
                    warn!(
                        "Node {} did not appear in Raft configuration after {} retries",
                        node_id, max_retries
                    );
                }

                // Get current cluster information using the authoritative voter list
                let voters = self.shared_state.get_current_voters().await?;
                let status = self.shared_state.get_raft_status();

                // Build peer information from the voter list and our local peer cache
                let mut node_infos = Vec::new();
                for voter_id in &voters {
                    if let Some(peer_info) = self.shared_state.get_peer(*voter_id) {
                        node_infos.push(NodeInfo {
                            id: peer_info.node_id.parse::<u64>().unwrap_or(*voter_id),
                            address: peer_info.address.clone(),
                            state: if *voter_id == our_id {
                                if status.is_leader {
                                    3
                                } else {
                                    1
                                } // Leader = 3, Follower = 1
                            } else {
                                0
                            }, // NodeStateUnknown for peers
                            p2p_node_id: peer_info.p2p_node_id.clone().unwrap_or_default(),
                            p2p_addresses: peer_info.p2p_addresses.clone(),
                            p2p_relay_url: peer_info.p2p_relay_url.clone().unwrap_or_default(),
                        });
                    } else if *voter_id == our_id {
                        // Add ourselves if not in peer list
                        node_infos.push(NodeInfo {
                            id: our_id,
                            address: our_addr.clone(),
                            state: if status.is_leader { 3 } else { 1 },
                            p2p_node_id: our_p2p_node_id.clone().unwrap_or_default(),
                            p2p_addresses: our_p2p_addresses.clone(),
                            p2p_relay_url: our_p2p_relay_url.clone().unwrap_or_default(),
                        });
                    }
                }

                // Sort by ID for consistent ordering
                node_infos.sort_by_key(|n| n.id);

                // The voters list is the authoritative list from Raft
                // Return it directly instead of building from the local peer cache

                Ok((
                    true,
                    "Successfully joined cluster".to_string(),
                    node_infos,
                    voters,
                ))
            }
            Err(e) => {
                error!("Failed to join cluster: {}", e);
                Ok((
                    false,
                    format!("Failed to join cluster: {}", e),
                    vec![],
                    vec![],
                ))
            }
        }
    }

    async fn leave_cluster(&self, node_id: u64) -> BlixardResult<(bool, String)> {
        debug!("Leave cluster request for node {}", node_id);

        // Propose configuration change through Raft
        match self
            .shared_state
            .propose_conf_change(ConfChangeType::RemoveNode, node_id, String::new())
            .await
        {
            Ok(_) => {
                info!("Node {} successfully left cluster", node_id);

                // Remove peer from our peer list
                self.shared_state.remove_peer(node_id);

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
        let raft_status = self.shared_state.get_raft_status();

        // Get the authoritative list of voters from Raft
        let voters = self.shared_state.get_current_voters().await?;

        // Get our own info
        let our_id = self.shared_state.get_id();
        let our_addr = self.shared_state.get_bind_addr().to_string();

        // Get our P2P info if available
        let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) =
            // TODO: Fix API mismatch - get_p2p_node_addr() returns Option<String> not iroh::NodeAddr
            if let Some(node_addr_str) = self.shared_state.get_p2p_node_addr() {
                // For now, return the string as node ID and empty addresses
                (Some(node_addr_str), Vec::new(), None)
            } else {
                (None, Vec::new(), None)
            };

        // Build node information from the voter list
        let mut nodes: Vec<NodeInfo> = Vec::new();
        for voter_id in &voters {
            if let Some(peer_info) = self.shared_state.get_peer(*voter_id) {
                nodes.push(NodeInfo {
                    id: peer_info.node_id.parse::<u64>().unwrap_or(*voter_id),
                    address: peer_info.address.clone(),
                    state: if *voter_id == our_id {
                        if raft_status.is_leader {
                            3
                        } else {
                            1
                        } // Leader = 3, Follower = 1
                    } else if raft_status.leader_id == Some(*voter_id) {
                        3 // Leader
                    } else {
                        1 // Follower
                    },
                    p2p_node_id: peer_info.p2p_node_id.clone().unwrap_or_default(),
                    p2p_addresses: peer_info.p2p_addresses.clone(),
                    p2p_relay_url: peer_info.p2p_relay_url.clone().unwrap_or_default(),
                });
            } else if *voter_id == our_id {
                // Add ourselves if not in peer list
                nodes.push(NodeInfo {
                    id: our_id,
                    address: our_addr.clone(),
                    state: if raft_status.is_leader { 3 } else { 1 },
                    p2p_node_id: our_p2p_node_id.clone().unwrap_or_default(),
                    p2p_addresses: our_p2p_addresses.clone(),
                    p2p_relay_url: our_p2p_relay_url.clone().unwrap_or_default(),
                });
            }
        }

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

        match self
            .shared_state
            .submit_task(&task.task_id, task_spec)
            .await
        {
            Ok(assigned_node) => {
                info!(
                    "Task submitted successfully, assigned to node {}",
                    assigned_node
                );
                Ok((
                    true,
                    "Task submitted successfully".to_string(),
                    assigned_node,
                ))
            }
            Err(e) => {
                error!("Failed to submit task: {}", e);
                Ok((false, format!("Failed to submit task: {}", e), 0))
            }
        }
    }

    async fn get_task_status(
        &self,
        task_id: String,
    ) -> BlixardResult<Option<(TaskStatus, String, String, u64)>> {
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
                disk_gb: 0,                // Not included in the simple Task type
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
