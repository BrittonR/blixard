//! Node registry for mapping cluster node IDs to Iroh node IDs
//! 
//! This is a stub implementation to maintain compilation compatibility.

use crate::error::{BlixardError, BlixardResult};
use iroh::{NodeAddr, NodeId};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Entry in the node registry
#[derive(Debug, Clone)]
pub struct NodeRegistryEntry {
    pub cluster_node_id: u64,
    pub iroh_node_id: NodeId,
    pub node_addr: NodeAddr,
    pub bind_address: Option<String>,
}

/// Registry for mapping cluster node IDs to Iroh node IDs and addresses
pub struct NodeRegistry {
    entries: RwLock<HashMap<u64, NodeRegistryEntry>>,
}

impl NodeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register a node in the registry
    pub async fn register_node(
        &self,
        cluster_node_id: u64,
        iroh_node_id: NodeId,
        node_addr: NodeAddr,
        bind_address: Option<String>,
    ) -> BlixardResult<()> {
        let mut entries = self.entries.write().await;
        entries.insert(
            cluster_node_id,
            NodeRegistryEntry {
                cluster_node_id,
                iroh_node_id,
                node_addr,
                bind_address,
            },
        );
        Ok(())
    }

    /// Get a node's address from the registry
    pub async fn get_node_addr(&self, cluster_node_id: u64) -> BlixardResult<NodeAddr> {
        let entries = self.entries.read().await;
        entries
            .get(&cluster_node_id)
            .map(|e| e.node_addr.clone())
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Node {} in registry", cluster_node_id),
            })
    }

    /// List all nodes in the registry
    pub async fn list_nodes(&self) -> Vec<NodeRegistryEntry> {
        let entries = self.entries.read().await;
        entries.values().cloned().collect()
    }
}