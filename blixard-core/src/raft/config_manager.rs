//! Raft configuration management and membership handling
//!
//! This module handles Raft cluster membership changes including adding and
//! removing nodes, managing peer connections, and maintaining configuration consistency.

use crate::error::{BlixardError, BlixardResult};
use crate::common::error_context::StorageContext;
use crate::raft_storage::RedbRaftStorage;

use super::messages::{RaftConfChange, ConfChangeType, ConfChangeContext};

use raft::prelude::{ConfChange, ConfChangeType as RaftConfChangeType, Entry, RawNode};
use redb::Database;
use slog::{info, warn, error, Logger};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;
use tracing::{debug, instrument};

/// Configuration manager for Raft membership changes
pub struct RaftConfigManager {
    node_id: u64,
    database: Arc<Database>,
    peers: Arc<RwLock<HashMap<u64, String>>>,
    logger: Logger,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    
    // Flag to trigger replication after config changes
    needs_replication_trigger: Arc<RwLock<bool>>,
}

impl RaftConfigManager {
    pub fn new(
        node_id: u64,
        database: Arc<Database>,
        peers: Arc<RwLock<HashMap<u64, String>>>,
        logger: Logger,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
    ) -> Self {
        Self {
            node_id,
            database,
            peers,
            logger,
            shared_state,
            needs_replication_trigger: Arc::new(RwLock::new(false)),
        }
    }

    /// Handle a configuration change request
    #[instrument(skip(self, raft_node), fields(node_id = self.node_id, change_type = ?conf_change.change_type, target_node = conf_change.node_id))]
    pub async fn handle_conf_change(
        &self,
        conf_change: RaftConfChange,
        raft_node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        info!(
            self.logger,
            "[CONF-CHANGE] Handling configuration change";
            "type" => ?conf_change.change_type,
            "node_id" => conf_change.node_id,
            "address" => &conf_change.address
        );

        // Only leaders can propose configuration changes
        if !raft_node.raft.state.eq(&raft::StateRole::Leader) {
            let error_msg = format!(
                "Configuration changes can only be proposed by the leader (current state: {:?})",
                raft_node.raft.state
            );
            warn!(self.logger, "{}", error_msg);
            
            if let Some(response_tx) = conf_change.response_tx {
                let _ = response_tx.send(Err(BlixardError::NotLeader {
                    operation: "propose configuration change".to_string(),
                    leader_id: raft_node.raft.leader_id,
                }));
            }
            return Ok(());
        }

        // Check if this node is the one being removed
        if matches!(conf_change.change_type, ConfChangeType::RemoveNode) && conf_change.node_id == self.node_id {
            warn!(self.logger, "Node is removing itself from cluster");
            // Allow self-removal, but we'll shut down after processing
        }

        // Create the Raft configuration change
        let raft_conf_change_type = match conf_change.change_type {
            ConfChangeType::AddNode => RaftConfChangeType::AddNode,
            ConfChangeType::RemoveNode => RaftConfChangeType::RemoveNode,
        };

        // Serialize the P2P context
        let context = ConfChangeContext {
            address: conf_change.address.clone(),
            p2p_node_id: conf_change.p2p_node_id.clone(),
            p2p_addresses: conf_change.p2p_addresses.clone(),
            p2p_relay_url: conf_change.p2p_relay_url.clone(),
        };

        let context_data = bincode::serialize(&context).map_err(|e| BlixardError::Serialization {
            operation: "serialize conf change context".to_string(),
            source: Box::new(e),
        })?;

        let cc = ConfChange {
            id: 0, // Will be assigned by Raft
            change_type: raft_conf_change_type.into(),
            node_id: conf_change.node_id,
            context: context_data,
        };

        // Propose the configuration change
        match raft_node.propose_conf_change(vec![], cc.clone()) {
            Ok(_) => {
                info!(
                    self.logger,
                    "[CONF-CHANGE] Proposed configuration change successfully"
                );
                
                // For single-node clusters, immediately handle the change
                if self.is_single_node_cluster(raft_node) {
                    info!(self.logger, "Single-node cluster detected, applying configuration change immediately");
                    self.apply_conf_change_immediately(&cc, raft_node).await?;
                }
                
                // Handle successful addition by updating local peer list and establishing connections
                if matches!(conf_change.change_type, ConfChangeType::AddNode) {
                    self.handle_successful_node_addition(&conf_change).await?;
                }
                
                if let Some(response_tx) = conf_change.response_tx {
                    let _ = response_tx.send(Ok(()));
                }
            }
            Err(e) => {
                error!(
                    self.logger,
                    "[CONF-CHANGE] Failed to propose configuration change";
                    "error" => %e
                );
                
                if let Some(response_tx) = conf_change.response_tx {
                    let _ = response_tx.send(Err(BlixardError::Raft {
                        operation: "propose configuration change".to_string(),
                        source: Box::new(e),
                    }));
                }
            }
        }

        Ok(())
    }

    /// Apply a configuration change that has been committed by Raft
    #[instrument(skip(self, raft_node), fields(node_id = self.node_id, cc_id = cc.id, change_type = ?cc.change_type, target_node = cc.node_id))]
    pub async fn apply_conf_change(
        &self,
        cc: &ConfChange,
        _entry: &Entry,
        raft_node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        info!(
            self.logger,
            "[CONF-CHANGE] Applying configuration change";
            "type" => ?cc.change_type,
            "node_id" => cc.node_id
        );

        // Apply the configuration change to the Raft node
        let conf_state = raft_node.apply_conf_change(cc)?;

        // Save the new configuration state to storage
        self.save_conf_state(&conf_state).await?;

        // Update local peer tracking based on the change
        self.update_local_peers_after_conf_change(cc).await?;

        // Trigger replication if this was an addition
        if cc.change_type == RaftConfChangeType::AddNode.into() {
            let mut needs_replication = self.needs_replication_trigger.write().await;
            *needs_replication = true;
            debug!("Set replication trigger flag for new node {}", cc.node_id);
        }

        Ok(())
    }

    /// Handle successful node addition by updating peers and establishing connections
    async fn handle_successful_node_addition(&self, conf_change: &RaftConfChange) -> BlixardResult<()> {
        // Update local peer list
        {
            let mut peers = self.peers.write().await;
            peers.insert(conf_change.node_id, conf_change.address.clone());
        }

        // Establish P2P connection to the new node if we have P2P info
        if let Some(shared) = self.shared_state.upgrade() {
            if let Some(ref p2p_node_id) = conf_change.p2p_node_id {
                if let Some(peer_connector) = shared.get_peer_connector() {
                    let node_addr = if let Some(relay_url) = &conf_change.p2p_relay_url {
                        // Use relay for connection
                        self.create_node_addr_with_relay(p2p_node_id, &conf_change.p2p_addresses, relay_url)?
                    } else {
                        // Direct connection
                        self.create_node_addr_direct(p2p_node_id, &conf_change.p2p_addresses)?
                    };

                    // Attempt to connect to the new peer
                    match peer_connector.connect_to_peer(conf_change.node_id, node_addr).await {
                        Ok(_) => {
                            info!(
                                self.logger,
                                "Successfully established P2P connection to new node {}",
                                conf_change.node_id
                            );
                        }
                        Err(e) => {
                            warn!(
                                self.logger,
                                "Failed to establish P2P connection to new node {}: {}",
                                conf_change.node_id,
                                e
                            );
                            // Don't fail the entire operation due to P2P connection issues
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Update local peer tracking after a configuration change is applied
    async fn update_local_peers_after_conf_change(&self, cc: &ConfChange) -> BlixardResult<()> {
        match cc.change_type {
            change_type if change_type == RaftConfChangeType::AddNode.into() => {
                // Parse the context to get the address
                if !cc.context.is_empty() {
                    match bincode::deserialize::<ConfChangeContext>(&cc.context) {
                        Ok(context) => {
                            let mut peers = self.peers.write().await;
                            peers.insert(cc.node_id, context.address);
                            info!(
                                self.logger,
                                "Added node {} with address {} to local peer list",
                                cc.node_id,
                                context.address
                            );
                        }
                        Err(e) => {
                            warn!(
                                self.logger,
                                "Failed to parse conf change context for node {}: {}",
                                cc.node_id,
                                e
                            );
                        }
                    }
                }
            }
            change_type if change_type == RaftConfChangeType::RemoveNode.into() => {
                let mut peers = self.peers.write().await;
                if let Some(address) = peers.remove(&cc.node_id) {
                    info!(
                        self.logger,
                        "Removed node {} with address {} from local peer list",
                        cc.node_id,
                        address
                    );
                }

                // If we're removing ourselves, we should prepare for shutdown
                if cc.node_id == self.node_id {
                    warn!(self.logger, "This node has been removed from the cluster");
                    // TODO: Trigger graceful shutdown
                }
            }
            _ => {
                warn!(self.logger, "Unknown conf change type: {:?}", cc.change_type);
            }
        }

        Ok(())
    }

    /// Save the configuration state to persistent storage
    async fn save_conf_state(&self, conf_state: &raft::prelude::ConfState) -> BlixardResult<()> {
        let write_txn = self.database.begin_write().storage_context("begin conf state transaction")?;
        
        {
            let mut table = write_txn.open_table(crate::raft_storage::CLUSTER_STATE_TABLE)?;
            let conf_data = bincode::serialize(conf_state).map_err(|e| BlixardError::Serialization {
                operation: "serialize conf state".to_string(),
                source: Box::new(e),
            })?;
            
            table.insert("conf_state", conf_data.as_slice())?;
        }
        
        write_txn.commit().storage_context("commit conf state")?;
        
        debug!(
            "Saved configuration state with {} voters and {} learners",
            conf_state.voters.len(),
            conf_state.learners.len()
        );
        
        Ok(())
    }

    /// Apply configuration change immediately for single-node clusters
    async fn apply_conf_change_immediately(
        &self,
        cc: &ConfChange,
        raft_node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        let conf_state = raft_node.apply_conf_change(cc)?;
        self.save_conf_state(&conf_state).await?;
        self.update_local_peers_after_conf_change(cc).await?;
        
        info!(
            self.logger,
            "Applied configuration change immediately for single-node cluster"
        );
        
        Ok(())
    }

    /// Check if this is a single-node cluster
    fn is_single_node_cluster(&self, raft_node: &RawNode<RedbRaftStorage>) -> bool {
        // A single-node cluster has only one voter (this node) and no learners
        let progress = &raft_node.raft.prs().voters();
        progress.len() == 1 && progress.contains_key(&self.node_id)
    }

    /// Create a node address with relay
    fn create_node_addr_with_relay(
        &self,
        p2p_node_id: &str,
        p2p_addresses: &[String],
        relay_url: &str,
    ) -> BlixardResult<iroh::NodeAddr> {
        use std::str::FromStr;
        
        let node_id = iroh::NodeId::from_str(p2p_node_id).map_err(|e| BlixardError::Internal {
            message: format!("Invalid P2P node ID {}: {}", p2p_node_id, e),
        })?;
        
        let mut addr = iroh::NodeAddr::new(node_id);
        
        // Add direct addresses
        for addr_str in p2p_addresses {
            if let Ok(socket_addr) = addr_str.parse::<std::net::SocketAddr>() {
                addr = addr.with_direct_addresses([socket_addr]);
            }
        }
        
        // Add relay
        if let Ok(relay_url_parsed) = relay_url.parse() {
            addr = addr.with_relay_url(relay_url_parsed);
        }
        
        Ok(addr)
    }

    /// Create a node address for direct connection
    fn create_node_addr_direct(
        &self,
        p2p_node_id: &str,
        p2p_addresses: &[String],
    ) -> BlixardResult<iroh::NodeAddr> {
        use std::str::FromStr;
        
        let node_id = iroh::NodeId::from_str(p2p_node_id).map_err(|e| BlixardError::Internal {
            message: format!("Invalid P2P node ID {}: {}", p2p_node_id, e),
        })?;
        
        let mut addr = iroh::NodeAddr::new(node_id);
        
        // Add direct addresses
        let socket_addrs: Result<Vec<std::net::SocketAddr>, _> = p2p_addresses
            .iter()
            .map(|addr_str| addr_str.parse())
            .collect();
            
        match socket_addrs {
            Ok(addrs) => {
                addr = addr.with_direct_addresses(addrs);
                Ok(addr)
            }
            Err(e) => Err(BlixardError::Internal {
                message: format!("Failed to parse P2P addresses: {}", e),
            })
        }
    }

    /// Check if replication trigger is needed
    pub async fn needs_replication_trigger(&self) -> bool {
        let mut needs_replication = self.needs_replication_trigger.write().await;
        let result = *needs_replication;
        if result {
            *needs_replication = false; // Reset the flag
        }
        result
    }

    /// Get current peer list
    pub async fn get_peers(&self) -> HashMap<u64, String> {
        self.peers.read().await.clone()
    }

    /// Update peer address
    pub async fn update_peer_address(&self, node_id: u64, address: String) {
        let mut peers = self.peers.write().await;
        peers.insert(node_id, address);
    }

    /// Remove peer
    pub async fn remove_peer(&self, node_id: u64) -> Option<String> {
        let mut peers = self.peers.write().await;
        peers.remove(&node_id)
    }
}