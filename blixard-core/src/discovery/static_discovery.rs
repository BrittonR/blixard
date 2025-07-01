//! Static discovery provider that uses configuration to find nodes.
//!
//! This is the simplest discovery mechanism, reading node information
//! from configuration and making it available to the discovery system.

use async_trait::async_trait;
use iroh::NodeId;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::discovery::{DiscoveryEvent, DiscoveryProvider, DiscoveryState, IrohNodeInfo};
use crate::error::{BlixardError, BlixardResult};

/// Static discovery provider that reads nodes from configuration
pub struct StaticDiscoveryProvider {
    /// Configured static nodes (node_id -> addresses)
    static_nodes: Vec<(String, Vec<String>)>,
    
    /// Shared discovery state
    state: Arc<DiscoveryState>,
    
    /// Whether the provider is running
    running: Arc<RwLock<bool>>,
    
    /// Event channel for subscribers
    event_sender: mpsc::Sender<DiscoveryEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::Receiver<DiscoveryEvent>>>>,
}

impl StaticDiscoveryProvider {
    /// Create a new static discovery provider
    pub fn new(static_nodes: Vec<(String, Vec<String>)>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        
        Self {
            static_nodes,
            state: Arc::new(DiscoveryState::new()),
            running: Arc::new(RwLock::new(false)),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
        }
    }
    
    /// Parse and add static nodes to the discovery state
    async fn load_static_nodes(&self) -> BlixardResult<()> {
        for (node_id_str, addresses_str) in &self.static_nodes {
            // Parse node ID
            let node_id = match node_id_str.parse::<NodeId>() {
                Ok(id) => id,
                Err(e) => {
                    error!("Invalid node ID '{}': {}", node_id_str, e);
                    continue;
                }
            };
            
            // Parse addresses
            let mut addresses = Vec::new();
            for addr_str in addresses_str {
                match SocketAddr::from_str(addr_str) {
                    Ok(addr) => addresses.push(addr),
                    Err(e) => {
                        warn!("Invalid address '{}' for node {}: {}", addr_str, node_id_str, e);
                    }
                }
            }
            
            if addresses.is_empty() {
                warn!("No valid addresses found for node {}", node_id_str);
                continue;
            }
            
            // Create node info
            let mut info = IrohNodeInfo::new(node_id, addresses);
            info.name = Some(format!("static-{}", &node_id_str[..8]));
            info.metadata.insert("source".to_string(), "static".to_string());
            
            // Add to state
            self.state.add_node(info.clone()).await;
            
            // Send discovery event
            let _ = self.event_sender.send(DiscoveryEvent::NodeDiscovered(info)).await;
            
            debug!("Added static node: {} with {} addresses", node_id_str, addresses_str.len());
        }
        
        info!("Loaded {} static nodes", self.static_nodes.len());
        Ok(())
    }
}

#[async_trait]
impl DiscoveryProvider for StaticDiscoveryProvider {
    async fn start(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(BlixardError::Internal {
                message: "Discovery provider is already running".to_string(),
            });
        }
        
        info!("Starting static discovery provider");
        
        // Load static nodes
        self.load_static_nodes().await?;
        
        *running = true;
        Ok(())
    }
    
    async fn stop(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        info!("Stopping static discovery provider");
        
        // Clear all static nodes from state
        let nodes = self.state.nodes.read().await;
        let static_nodes: Vec<NodeId> = nodes
            .iter()
            .filter(|(_, info)| info.metadata.get("source") == Some(&"static".to_string()))
            .map(|(id, _)| *id)
            .collect();
        drop(nodes);
        
        for node_id in static_nodes {
            self.state.remove_node(node_id).await;
        }
        
        *running = false;
        Ok(())
    }
    
    async fn get_nodes(&self) -> BlixardResult<Vec<IrohNodeInfo>> {
        let nodes = self.state.nodes.read().await;
        Ok(nodes.values().cloned().collect())
    }
    
    async fn subscribe(&self) -> BlixardResult<mpsc::Receiver<DiscoveryEvent>> {
        let (tx, rx) = mpsc::channel(100);
        self.state.subscribers.write().await.push(tx);
        Ok(rx)
    }
    
    fn name(&self) -> &str {
        "static"
    }
    
    fn is_running(&self) -> bool {
        // Use block_in_place for sync context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                *self.running.read().await
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_static_discovery_basic() {
        // Create a valid node ID for testing
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        let node_id_str = node_id.to_string();
        
        let static_nodes = vec![
            (node_id_str.clone(), vec!["127.0.0.1:8080".to_string()]),
        ];
        
        let mut provider = StaticDiscoveryProvider::new(static_nodes);
        
        // Start the provider
        provider.start().await.unwrap();
        assert!(provider.is_running());
        
        // Get nodes
        let nodes = provider.get_nodes().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, node_id);
        assert_eq!(nodes[0].addresses.len(), 1);
        
        // Stop the provider
        provider.stop().await.unwrap();
        assert!(!provider.is_running());
    }
    
    #[tokio::test]
    async fn test_static_discovery_invalid_entries() {
        let static_nodes = vec![
            ("invalid-node-id".to_string(), vec!["127.0.0.1:8080".to_string()]),
            (NodeId::from_bytes(&[2u8; 32]).unwrap().to_string(), vec!["invalid-address".to_string()]),
        ];
        
        let mut provider = StaticDiscoveryProvider::new(static_nodes);
        
        // Should start successfully even with invalid entries
        provider.start().await.unwrap();
        
        // Should have no valid nodes
        let nodes = provider.get_nodes().await.unwrap();
        assert_eq!(nodes.len(), 0);
    }
    
    #[tokio::test]
    async fn test_static_discovery_events() {
        let node_id = NodeId::from_bytes(&[3u8; 32]).unwrap();
        let static_nodes = vec![
            (node_id.to_string(), vec!["127.0.0.1:8080".to_string()]),
        ];
        
        let mut provider = StaticDiscoveryProvider::new(static_nodes);
        
        // Subscribe before starting
        let mut events = provider.subscribe().await.unwrap();
        
        // Start the provider
        provider.start().await.unwrap();
        
        // Should receive discovery event
        if let Some(event) = events.recv().await {
            match event {
                DiscoveryEvent::NodeDiscovered(info) => {
                    assert_eq!(info.node_id, node_id);
                }
                _ => panic!("Expected NodeDiscovered event"),
            }
        } else {
            panic!("Expected to receive an event");
        }
    }
}