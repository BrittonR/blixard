//! Discovery module for finding and managing Iroh nodes in the Blixard network.
//!
//! This module provides various discovery mechanisms including:
//! - Static configuration-based discovery
//! - DNS-based discovery
//! - mDNS (multicast DNS) for local network discovery
//!
//! The discovery system is pluggable, allowing multiple providers to work together
//! to maintain a comprehensive view of available nodes in the network.

use async_trait::async_trait;
use iroh::NodeId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::BlixardResult;

// Re-export submodules
pub mod dns_discovery;
pub mod iroh_discovery_bridge;
pub mod manager;
pub mod mdns_discovery;
pub mod static_discovery;

// Re-export commonly used types
pub use manager::DiscoveryManager;
pub use iroh_discovery_bridge::{IrohDiscoveryBridge, create_combined_discovery};

/// Information about an Iroh node discovered in the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohNodeInfo {
    /// The unique node ID
    pub node_id: NodeId,
    
    /// Network addresses where the node can be reached
    pub addresses: Vec<SocketAddr>,
    
    /// Optional human-readable name for the node
    pub name: Option<String>,
    
    /// Additional metadata about the node
    pub metadata: HashMap<String, String>,
    
    /// Timestamp when this node was last seen (Unix timestamp)
    pub last_seen: u64,
}

impl IrohNodeInfo {
    /// Create a new IrohNodeInfo with minimal information
    pub fn new(node_id: NodeId, addresses: Vec<SocketAddr>) -> Self {
        Self {
            node_id,
            addresses,
            name: None,
            metadata: HashMap::new(),
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    
    /// Update the last seen timestamp to now
    pub fn update_last_seen(&mut self) {
        self.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
    
    /// Check if the node information is considered stale
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.last_seen > max_age_secs
    }
}

/// Event emitted by discovery providers when nodes are discovered or removed
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new node was discovered or an existing node was updated
    NodeDiscovered(IrohNodeInfo),
    
    /// A node is no longer reachable
    NodeRemoved(NodeId),
    
    /// The addresses of a node have changed
    NodeUpdated(IrohNodeInfo),
}

/// Trait for implementing node discovery mechanisms
#[async_trait]
pub trait DiscoveryProvider: Send + Sync {
    /// Start the discovery provider
    async fn start(&mut self) -> BlixardResult<()>;
    
    /// Stop the discovery provider
    async fn stop(&mut self) -> BlixardResult<()>;
    
    /// Get all currently known nodes
    async fn get_nodes(&self) -> BlixardResult<Vec<IrohNodeInfo>>;
    
    /// Subscribe to discovery events
    /// Returns a channel receiver for discovery events
    async fn subscribe(&self) -> BlixardResult<tokio::sync::mpsc::Receiver<DiscoveryEvent>>;
    
    /// Get the name of this discovery provider
    fn name(&self) -> &str;
    
    /// Check if the provider is currently running
    fn is_running(&self) -> bool;
}

/// Configuration for discovery providers
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Enable static discovery from configuration
    pub enable_static: bool,
    
    /// Enable DNS-based discovery
    pub enable_dns: bool,
    
    /// Enable mDNS for local network discovery
    pub enable_mdns: bool,
    
    /// How often to refresh discovery information (in seconds)
    pub refresh_interval: u64,
    
    /// Maximum age before considering a node stale (in seconds)
    pub node_stale_timeout: u64,
    
    /// Static nodes configuration (node_id -> addresses)
    pub static_nodes: HashMap<String, Vec<String>>,
    
    /// DNS domains to query for SRV records
    pub dns_domains: Vec<String>,
    
    /// mDNS service name to advertise and discover
    pub mdns_service_name: String,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_static: true,
            enable_dns: false,
            enable_mdns: true,
            refresh_interval: 30,
            node_stale_timeout: 300, // 5 minutes
            static_nodes: HashMap::new(),
            dns_domains: vec!["_iroh._tcp.local".to_string()],
            mdns_service_name: "_blixard-iroh._tcp.local".to_string(),
        }
    }
}

/// Shared state for discovery providers
pub(crate) struct DiscoveryState {
    /// All discovered nodes
    pub nodes: Arc<RwLock<HashMap<NodeId, IrohNodeInfo>>>,
    
    /// Event subscribers
    pub subscribers: Arc<RwLock<Vec<tokio::sync::mpsc::Sender<DiscoveryEvent>>>>,
}

impl DiscoveryState {
    /// Create a new discovery state
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Notify all subscribers of a discovery event
    pub async fn notify(&self, event: DiscoveryEvent) {
        let subscribers = self.subscribers.read().await;
        for sender in subscribers.iter() {
            // Ignore send errors (subscriber may have dropped)
            let _ = sender.send(event.clone()).await;
        }
    }
    
    /// Add or update a node
    pub async fn add_node(&self, mut info: IrohNodeInfo) {
        info.update_last_seen();
        let node_id = info.node_id;
        
        let mut nodes = self.nodes.write().await;
        let is_new = !nodes.contains_key(&node_id);
        nodes.insert(node_id, info.clone());
        drop(nodes);
        
        // Notify subscribers
        let event = if is_new {
            DiscoveryEvent::NodeDiscovered(info)
        } else {
            DiscoveryEvent::NodeUpdated(info)
        };
        self.notify(event).await;
    }
    
    /// Remove a node
    pub async fn remove_node(&self, node_id: NodeId) {
        let mut nodes = self.nodes.write().await;
        if nodes.remove(&node_id).is_some() {
            drop(nodes);
            self.notify(DiscoveryEvent::NodeRemoved(node_id)).await;
        }
    }
    
    /// Clean up stale nodes
    pub async fn cleanup_stale_nodes(&self, max_age_secs: u64) {
        let nodes = self.nodes.read().await;
        let stale_nodes: Vec<NodeId> = nodes
            .iter()
            .filter(|(_, info)| info.is_stale(max_age_secs))
            .map(|(id, _)| *id)
            .collect();
        drop(nodes);
        
        for node_id in stale_nodes {
            self.remove_node(node_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_iroh_node_info_staleness() {
        let node_id = NodeId::from_bytes(&[0u8; 32]).unwrap();
        let mut info = IrohNodeInfo::new(node_id, vec![]);
        
        // Fresh node should not be stale
        assert!(!info.is_stale(60));
        
        // Manually set old timestamp
        info.last_seen = info.last_seen - 120;
        assert!(info.is_stale(60));
        
        // Update and check again
        info.update_last_seen();
        assert!(!info.is_stale(60));
    }
    
    #[tokio::test]
    async fn test_discovery_state() {
        let state = DiscoveryState::new();
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        let info = IrohNodeInfo::new(node_id, vec![]);
        
        // Add node
        state.add_node(info.clone()).await;
        
        // Verify node exists
        let nodes = state.nodes.read().await;
        assert!(nodes.contains_key(&node_id));
        drop(nodes);
        
        // Remove node
        state.remove_node(node_id).await;
        
        // Verify node is gone
        let nodes = state.nodes.read().await;
        assert!(!nodes.contains_key(&node_id));
    }
}