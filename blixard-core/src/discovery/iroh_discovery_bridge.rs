//! Bridge between Blixard's discovery system and Iroh's discovery API
//!
//! This module provides a custom discovery service that integrates Blixard's
//! existing discovery providers with Iroh 0.90's discovery mechanism.

use std::collections::{HashMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::pin::Pin;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use iroh::{NodeAddr, NodeId};
use iroh::discovery::{Discovery, DiscoveryItem, NodeData, NodeInfo};
use futures::Stream;

use crate::discovery::{DiscoveryEvent, DiscoveryManager, IrohNodeInfo};
use crate::error::BlixardResult;

/// Bridge that connects Blixard's discovery system to Iroh's discovery API
#[derive(Clone)]
pub struct IrohDiscoveryBridge {
    /// Reference to Blixard's discovery manager
    discovery_manager: Arc<DiscoveryManager>,
    
    /// Cache of discovered nodes in Iroh format
    node_cache: Arc<RwLock<HashMap<NodeId, NodeAddr>>>,
    
    /// Whether the bridge is running
    running: Arc<RwLock<bool>>,
}

impl IrohDiscoveryBridge {
    /// Create a new discovery bridge
    pub fn new(discovery_manager: Arc<DiscoveryManager>) -> Self {
        Self {
            discovery_manager,
            node_cache: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start the bridge and begin syncing discovery information
    pub async fn start(&self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);
        
        info!("Starting Iroh discovery bridge");
        
        // Subscribe to Blixard discovery events
        let mut event_rx = self.discovery_manager.subscribe().await;
        
        // Clone for the background task
        let node_cache = self.node_cache.clone();
        let running = self.running.clone();
        
        // Spawn background task to process discovery events
        tokio::spawn(async move {
            while *running.read().await {
                match event_rx.recv().await {
                    Some(event) => {
                        match event {
                            DiscoveryEvent::NodeDiscovered(info) | DiscoveryEvent::NodeUpdated(info) => {
                                // Convert Blixard node info to Iroh NodeAddr
                                if let Ok(node_addr) = Self::convert_to_node_addr(&info) {
                                    let mut cache = node_cache.write().await;
                                    cache.insert(info.node_id, node_addr);
                                    debug!("Added/updated node {} in Iroh discovery cache", info.node_id);
                                }
                            }
                            DiscoveryEvent::NodeRemoved(node_id) => {
                                let mut cache = node_cache.write().await;
                                cache.remove(&node_id);
                                debug!("Removed node {} from Iroh discovery cache", node_id);
                            }
                        }
                    }
                    None => {
                        warn!("Discovery event channel closed");
                        break;
                    }
                }
            }
        });
        
        // Load initial nodes
        self.sync_initial_nodes().await?;
        
        Ok(())
    }
    
    /// Stop the bridge
    pub async fn stop(&self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopped Iroh discovery bridge");
        Ok(())
    }
    
    /// Sync initial nodes from Blixard discovery to Iroh cache
    async fn sync_initial_nodes(&self) -> BlixardResult<()> {
        let nodes = self.discovery_manager.get_all_nodes().await?;
        let mut cache = self.node_cache.write().await;
        
        for info in nodes {
            if let Ok(node_addr) = Self::convert_to_node_addr(&info) {
                cache.insert(info.node_id, node_addr);
            }
        }
        
        info!("Synced {} nodes to Iroh discovery cache", cache.len());
        Ok(())
    }
    
    /// Convert Blixard's IrohNodeInfo to Iroh's NodeAddr
    fn convert_to_node_addr(info: &IrohNodeInfo) -> BlixardResult<NodeAddr> {
        // Create NodeAddr with the node ID and addresses
        let direct_addresses: Vec<std::net::SocketAddr> = info.addresses.clone();
        
        let mut node_addr = NodeAddr::from_parts(info.node_id, None, direct_addresses);
        
        // Add relay URL if present in metadata
        if let Some(relay_url) = info.metadata.get("relay_url") {
            if let Ok(url) = relay_url.parse() {
                node_addr = node_addr.with_relay_url(url);
            }
        }
        
        Ok(node_addr)
    }
}

impl fmt::Debug for IrohDiscoveryBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IrohDiscoveryBridge")
            .field("running", &self.running)
            .finish()
    }
}

impl Discovery for IrohDiscoveryBridge {
    /// Resolve a node ID to its addresses
    fn resolve(
        &self,
        _endpoint: iroh::Endpoint,
        node_id: iroh::PublicKey,
    ) -> Option<Pin<Box<dyn Stream<Item = Result<DiscoveryItem, anyhow::Error>> + Send + 'static>>> {
        let node_cache = self.node_cache.clone();
        
        // Create a stream that will yield discovery items for this node
        let stream = futures::stream::unfold(
            (node_cache, node_id, false),
            move |(cache, node_id, done)| async move {
                if done {
                    return None;
                }
                
                // Look up the node in our cache
                let nodes = cache.read().await;
                match nodes.get(&node_id) {
                    Some(node_addr) => {
                        debug!("Found node {} in Blixard discovery cache", node_id);
                        // Create a NodeInfo from the NodeAddr
                        let direct_addresses: BTreeSet<_> = node_addr.direct_addresses
                            .iter()
                            .cloned()
                            .collect();
                        let mut node_info = NodeInfo::new(node_id)
                            .with_direct_addresses(direct_addresses);
                        
                        // Add relay URL if present
                        if let Some(relay_url) = &node_addr.relay_url {
                            node_info = node_info.with_relay_url(Some(relay_url.clone()));
                        }
                        
                        // Create a DiscoveryItem
                        let item = DiscoveryItem::new(node_info, "blixard", None);
                        Some((Ok(item), (cache.clone(), node_id, true)))
                    }
                    None => {
                        debug!("Node {} not found in Blixard discovery cache", node_id);
                        // Return an empty result to indicate not found
                        None
                    }
                }
            }
        );
        
        Some(Box::pin(stream))
    }
    
    /// Publish our own node information (not used by Blixard discovery)
    fn publish(&self, _data: &NodeData) {
        // Blixard discovery is read-only from Iroh's perspective
        // Publishing is handled by Blixard's discovery providers
        debug!("Publish called on Blixard discovery bridge (no-op)");
    }
}

/// Create a combined discovery service with Blixard and Iroh's default discovery
pub fn create_combined_discovery(
    discovery_manager: Arc<DiscoveryManager>,
) -> impl Discovery {
    // For now, just return the bridge
    // In the future, we could implement a custom combiner
    IrohDiscoveryBridge::new(discovery_manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DiscoveryConfig;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::time::Duration;
    
    #[tokio::test]
    async fn test_iroh_discovery_bridge() {
        // Create a discovery manager
        let config = DiscoveryConfig::default();
        let manager = Arc::new(DiscoveryManager::new(config));
        
        // Create and start the bridge
        let bridge = IrohDiscoveryBridge::new(manager.clone());
        bridge.start().await.unwrap();
        
        // Create a test node info
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 9000);
        let info = IrohNodeInfo::new(node_id, vec![addr]);
        
        // Manually add to discovery manager's state
        manager.add_discovered_node(info).await.unwrap();
        
        // Give time for event processing
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Check if node is in cache
        let cache = bridge.node_cache.read().await;
        assert!(cache.contains_key(&node_id));
        
        // Stop the bridge
        bridge.stop().await.unwrap();
    }
    
    #[test]
    fn test_node_addr_conversion() {
        let node_id = NodeId::from_bytes(&[2u8; 32]).unwrap();
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 8080);
        
        let mut info = IrohNodeInfo::new(node_id, vec![addr1, addr2]);
        info.metadata.insert("relay_url".to_string(), "https://relay.example.com".to_string());
        
        let node_addr = IrohDiscoveryBridge::convert_to_node_addr(&info).unwrap();
        
        assert_eq!(node_addr.node_id, node_id);
        assert_eq!(node_addr.direct_addresses.len(), 2);
        assert_eq!(node_addr.relay_url.as_ref().unwrap().as_str(), "https://relay.example.com");
    }
}