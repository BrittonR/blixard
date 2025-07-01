//! Node discovery mechanism for Iroh connections
//!
//! This module provides ways to discover Iroh node information
//! from various sources.

use blixard_core::{
    error::{BlixardError, BlixardResult},
};
use std::path::Path;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Node registry entry containing connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRegistryEntry {
    /// Node ID in the cluster
    pub cluster_node_id: u64,
    /// Iroh node ID (base64 encoded)
    pub iroh_node_id: String,
    /// Direct addresses for Iroh connection
    pub direct_addresses: Vec<String>,
    /// Relay URL if available
    pub relay_url: Option<String>,
    /// Address for backward compatibility
    pub address: String,
}

/// Node discovery mechanism
pub struct NodeDiscovery {
    /// In-memory cache of known nodes
    cache: HashMap<String, NodeRegistryEntry>,
}

impl NodeDiscovery {
    /// Create a new node discovery instance
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Discover node information from an address string
    /// 
    /// Supports formats:
    /// - File path to node registry JSON
    /// - TODO: Support direct Iroh discovery via mDNS or other mechanisms
    pub async fn discover_node(&mut self, address: &str) -> BlixardResult<NodeRegistryEntry> {
        // Check cache first
        if let Some(entry) = self.cache.get(address) {
            return Ok(entry.clone());
        }

        // Try to parse as file path
        if Path::new(address).exists() {
            return self.load_from_file(address).await;
        }

        // For now, return a dummy entry for development
        // TODO: Implement actual Iroh node discovery
        let entry = NodeRegistryEntry {
            cluster_node_id: 1,
            iroh_node_id: "dummy_node_id".to_string(),
            direct_addresses: vec![address.to_string()],
            relay_url: Some("https://relay.iroh.network".to_string()),
            address: address.to_string(),
        };

        // Cache the entry
        self.cache.insert(address.to_string(), entry.clone());

        Ok(entry)
    }

    /// Load node registry from a file
    async fn load_from_file(&mut self, path: &str) -> BlixardResult<NodeRegistryEntry> {
        let contents = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read node registry file: {}", e),
            })?;

        let entry: NodeRegistryEntry = serde_json::from_str(&contents)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to parse node registry: {}", e),
            })?;

        // Cache the entry
        self.cache.insert(path.to_string(), entry.clone());
        self.cache.insert(entry.address.clone(), entry.clone());

        Ok(entry)
    }

    /// Create an Iroh NodeAddr from registry entry
    pub fn create_node_addr(&self, entry: &NodeRegistryEntry) -> BlixardResult<iroh::NodeAddr> {
        // Parse Iroh node ID from base64
        let node_id_bytes = base64::decode(&entry.iroh_node_id)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to decode Iroh node ID: {}", e),
            })?;

        let node_id = iroh::NodeId::from_bytes(&node_id_bytes)
            .map_err(|e| BlixardError::Internal {
                message: format!("Invalid Iroh node ID: {}", e),
            })?;

        // Parse direct addresses
        let direct_addrs: Vec<std::net::SocketAddr> = entry.direct_addresses
            .iter()
            .filter_map(|addr| addr.parse().ok())
            .collect();

        // Create NodeAddr
        let mut builder = iroh::NodeAddr::new(node_id);
        
        // Add direct addresses
        for addr in direct_addrs {
            builder = builder.with_direct_addr(addr);
        }

        // Add relay URL if available
        if let Some(relay_url) = &entry.relay_url {
            if let Ok(url) = relay_url.parse() {
                builder = builder.with_relay_url(url);
            }
        }

        Ok(builder.build())
    }
}

/// Save node registry entry to a file
pub async fn save_node_registry(path: &str, entry: &NodeRegistryEntry) -> BlixardResult<()> {
    let json = serde_json::to_string_pretty(entry)
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to serialize node registry: {}", e),
        })?;

    tokio::fs::write(path, json)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to write node registry: {}", e),
        })?;

    Ok(())
}