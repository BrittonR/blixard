//! Node discovery mechanism for Iroh connections
//!
//! This module provides ways to discover Iroh node information
//! from various sources.

use blixard_core::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
    /// - Address in format host:port (will look for registry file in default locations)
    pub async fn discover_node(&mut self, address: &str) -> BlixardResult<NodeRegistryEntry> {
        // Check cache first
        if let Some(entry) = self.cache.get(address) {
            return Ok(entry.clone());
        }

        // Try to parse as file path
        if Path::new(address).exists() {
            return self.load_from_file(address).await;
        }

        // Try to find registry file based on address
        // For addresses like "127.0.0.1:7001", look for registry files in common locations
        let registry_paths = vec![
            "./data/node1/node-1-registry.json".to_string(),
            "./data/node2/node-2-registry.json".to_string(),
            "./data/node3/node-3-registry.json".to_string(),
            format!("./data/node-1-registry.json"),
            format!("./node-1-registry.json"),
        ];

        for path in &registry_paths {
            if Path::new(path).exists() {
                match self.load_from_file(path).await {
                    Ok(entry) => {
                        // Check if the address matches
                        if entry.address == address
                            || entry.direct_addresses.contains(&address.to_string())
                        {
                            self.cache.insert(address.to_string(), entry.clone());
                            return Ok(entry);
                        }
                    }
                    Err(_) => continue,
                }
            }
        }

        // If no registry file found, return error
        Err(BlixardError::Internal {
            message: format!(
                "Failed to discover node at {}. No registry file found. \
                Make sure the node is running and has written its registry file.",
                address
            ),
        })
    }

    /// Load node registry from a file
    async fn load_from_file(&mut self, path: &str) -> BlixardResult<NodeRegistryEntry> {
        let contents =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to read node registry file: {}", e),
                })?;

        let entry: NodeRegistryEntry =
            serde_json::from_str(&contents).map_err(|e| BlixardError::Internal {
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
        let node_id_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &entry.iroh_node_id,
        )
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to decode Iroh node ID: {}", e),
        })?;

        // Convert Vec<u8> to [u8; 32]
        let node_id_array: [u8; 32] =
            node_id_bytes
                .try_into()
                .map_err(|bytes: Vec<u8>| BlixardError::Internal {
                    message: format!(
                        "Invalid Iroh node ID length: expected 32 bytes, got {}",
                        bytes.len()
                    ),
                })?;

        let node_id =
            iroh::NodeId::from_bytes(&node_id_array).map_err(|e| BlixardError::Internal {
                message: format!("Invalid Iroh node ID: {}", e),
            })?;

        // Parse direct addresses, converting wildcard addresses to localhost
        let direct_addrs: Vec<std::net::SocketAddr> = entry
            .direct_addresses
            .iter()
            .filter_map(|addr| {
                match addr.parse::<std::net::SocketAddr>() {
                    Ok(mut socket_addr) => {
                        // Convert 0.0.0.0 to 127.0.0.1 for local connections
                        if socket_addr.ip().is_unspecified() {
                            socket_addr.set_ip(if socket_addr.is_ipv4() {
                                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
                            } else {
                                std::net::IpAddr::V6(std::net::Ipv6Addr::new(
                                    0, 0, 0, 0, 0, 0, 0, 1,
                                ))
                            });
                        }
                        Some(socket_addr)
                    }
                    Err(_) => None,
                }
            })
            .collect();

        // Create NodeAddr
        let mut builder = iroh::NodeAddr::new(node_id);

        // Add direct addresses
        if !direct_addrs.is_empty() {
            builder = builder.with_direct_addresses(direct_addrs);
        }

        // Add relay URL if available
        if let Some(relay_url) = &entry.relay_url {
            if let Ok(url) = relay_url.parse() {
                builder = builder.with_relay_url(url);
            }
        }

        Ok(builder)
    }
}

/// Save node registry entry to a file
pub async fn save_node_registry(path: &str, entry: &NodeRegistryEntry) -> BlixardResult<()> {
    let json = serde_json::to_string_pretty(entry).map_err(|e| BlixardError::Internal {
        message: format!("Failed to serialize node registry: {}", e),
    })?;

    tokio::fs::write(path, json)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to write node registry: {}", e),
        })?;

    Ok(())
}
