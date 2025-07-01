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
    /// gRPC address for backward compatibility
    pub grpc_address: String,
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
    /// - "127.0.0.1:7001" - Will try to query the node for its Iroh info
    /// - "node_id@127.0.0.1:7001" - Same as above but with explicit node ID
    /// - File path to node registry JSON
    pub async fn discover_node(&mut self, address: &str) -> BlixardResult<NodeRegistryEntry> {
        // Check cache first
        if let Some(entry) = self.cache.get(address) {
            return Ok(entry.clone());
        }

        // Try to parse as file path
        if Path::new(address).exists() {
            return self.load_from_file(address).await;
        }

        // Try to connect via gRPC and query node info
        self.query_node_info(address).await
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
        self.cache.insert(entry.grpc_address.clone(), entry.clone());

        Ok(entry)
    }

    /// Query node information via gRPC
    async fn query_node_info(&mut self, address: &str) -> BlixardResult<NodeRegistryEntry> {
        use blixard_core::proto::{
            cluster_service_client::ClusterServiceClient,
            GetP2pStatusRequest,
        };

        // Parse address
        let (node_id, grpc_addr) = if address.contains('@') {
            let parts: Vec<&str> = address.split('@').collect();
            if parts.len() != 2 {
                return Err(BlixardError::Internal {
                    message: format!("Invalid address format: {}", address),
                });
            }
            let id = parts[0].parse::<u64>().map_err(|e| BlixardError::Internal {
                message: format!("Invalid node ID: {}", e),
            })?;
            (id, parts[1].to_string())
        } else {
            // Will discover node ID from the node itself
            (0, address.to_string())
        };

        // Connect via gRPC to get P2P status
        let mut client = ClusterServiceClient::connect(format!("http://{}", grpc_addr))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect to node via gRPC: {}", e),
            })?;

        let response = client.get_p2p_status(tonic::Request::new(GetP2pStatusRequest {}))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get P2P status: {}", e),
            })?;

        let status = response.into_inner();
        
        if !status.enabled {
            return Err(BlixardError::Internal {
                message: "Node does not have P2P/Iroh enabled".to_string(),
            });
        }

        let entry = NodeRegistryEntry {
            cluster_node_id: if node_id > 0 { node_id } else { status.node_id },
            iroh_node_id: status.node_id_base64,
            direct_addresses: status.direct_addresses,
            relay_url: if status.relay_url.is_empty() { None } else { Some(status.relay_url) },
            grpc_address: grpc_addr.clone(),
        };

        // Cache the entry
        self.cache.insert(address.to_string(), entry.clone());
        self.cache.insert(grpc_addr, entry.clone());

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