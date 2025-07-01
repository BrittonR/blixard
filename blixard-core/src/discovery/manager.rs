//! Discovery manager that coordinates multiple discovery providers.
//!
//! The manager handles the lifecycle of all configured discovery providers
//! and provides a unified interface for node discovery.

use iroh::NodeId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::discovery::{
    DiscoveryConfig, DiscoveryEvent, DiscoveryProvider, IrohNodeInfo,
    dns_discovery::DnsDiscoveryProvider,
    mdns_discovery::MdnsDiscoveryProvider,
    static_discovery::StaticDiscoveryProvider,
};
use crate::error::{BlixardError, BlixardResult};

/// Discovery manager that coordinates multiple discovery providers
pub struct DiscoveryManager {
    /// Configuration
    config: DiscoveryConfig,
    
    /// Active discovery providers
    providers: Arc<RwLock<Vec<Box<dyn DiscoveryProvider>>>>,
    
    /// Aggregated node information from all providers
    nodes: Arc<RwLock<HashMap<NodeId, IrohNodeInfo>>>,
    
    /// Event subscribers
    subscribers: Arc<RwLock<Vec<mpsc::Sender<DiscoveryEvent>>>>,
    
    /// Background task handle for event aggregation
    task_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    
    /// Whether the manager is running
    running: Arc<RwLock<bool>>,
}

impl DiscoveryManager {
    /// Create a new discovery manager with the given configuration
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            providers: Arc::new(RwLock::new(Vec::new())),
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            task_handle: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Configure the manager for a specific node
    pub fn configure_for_node(&mut self, node_id: NodeId, addresses: Vec<std::net::SocketAddr>) {
        // Update mDNS provider if it will be enabled
        if self.config.enable_mdns {
            // This will be set on the provider when it's created
            self.config.static_nodes.insert(
                "_mdns_self".to_string(),
                vec![node_id.to_string(), addresses[0].to_string()],
            );
        }
    }
    
    /// Start all configured discovery providers
    pub async fn start(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(BlixardError::Internal {
                message: "Discovery manager is already running".to_string(),
            });
        }
        
        info!("Starting discovery manager");
        
        let mut providers = self.providers.write().await;
        
        // Create and start static discovery provider
        if self.config.enable_static {
            let static_nodes: Vec<(String, Vec<String>)> = self.config.static_nodes
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))  // Skip internal entries
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            
            if !static_nodes.is_empty() {
                info!("Enabling static discovery with {} nodes", static_nodes.len());
                let mut provider = StaticDiscoveryProvider::new(static_nodes);
                provider.start().await?;
                providers.push(Box::new(provider) as Box<dyn DiscoveryProvider>);
            }
        }
        
        // Create and start DNS discovery provider
        if self.config.enable_dns && !self.config.dns_domains.is_empty() {
            info!("Enabling DNS discovery for domains: {:?}", self.config.dns_domains);
            let mut provider = DnsDiscoveryProvider::new(
                self.config.dns_domains.clone(),
                Duration::from_secs(self.config.refresh_interval),
            )?;
            provider.start().await?;
            providers.push(Box::new(provider) as Box<dyn DiscoveryProvider>);
        }
        
        // Create and start mDNS discovery provider
        if self.config.enable_mdns {
            info!("Enabling mDNS discovery with service: {}", self.config.mdns_service_name);
            let mut provider = MdnsDiscoveryProvider::new(self.config.mdns_service_name.clone());
            
            // Configure our node info if available
            if let Some(node_info) = self.config.static_nodes.get("_mdns_self") {
                if node_info.len() >= 2 {
                    if let Ok(node_id) = node_info[0].parse::<NodeId>() {
                        if let Ok(addr) = node_info[1].parse() {
                            provider.set_our_node_info(node_id, vec![addr]);
                        }
                    }
                }
            }
            
            provider.start().await?;
            providers.push(Box::new(provider) as Box<dyn DiscoveryProvider>);
        }
        
        if providers.is_empty() {
            warn!("No discovery providers enabled");
        }
        
        *running = true;
        drop(providers);
        
        // Start background task to aggregate events
        let manager = Arc::new(RwLock::new(self.clone()));
        let handle = tokio::spawn(Self::event_aggregation_task(manager));
        *self.task_handle.write().await = Some(handle);
        
        Ok(())
    }
    
    /// Stop all discovery providers
    pub async fn stop(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        info!("Stopping discovery manager");
        
        *running = false;
        
        // Cancel background task
        if let Some(handle) = self.task_handle.write().await.take() {
            handle.abort();
        }
        
        // Stop all providers
        let mut providers = self.providers.write().await;
        for provider in providers.iter_mut() {
            if let Err(e) = provider.stop().await {
                error!("Failed to stop provider {}: {}", provider.name(), e);
            }
        }
        providers.clear();
        
        // Clear nodes
        self.nodes.write().await.clear();
        
        Ok(())
    }
    
    /// Get all discovered nodes
    pub async fn get_nodes(&self) -> Vec<IrohNodeInfo> {
        self.nodes.read().await.values().cloned().collect()
    }
    
    /// Get a specific node by ID
    pub async fn get_node(&self, node_id: &NodeId) -> Option<IrohNodeInfo> {
        self.nodes.read().await.get(node_id).cloned()
    }
    
    /// Subscribe to discovery events
    pub async fn subscribe(&self) -> mpsc::Receiver<DiscoveryEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.subscribers.write().await.push(tx);
        rx
    }
    
    /// Check if the manager is running
    pub fn is_running(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                *self.running.read().await
            })
        })
    }
    
    /// Add a discovered node directly (useful for testing and manual discovery)
    pub async fn add_discovered_node(&self, node_info: IrohNodeInfo) -> BlixardResult<()> {
        let mut nodes = self.nodes.write().await;
        let is_new = !nodes.contains_key(&node_info.node_id);
        nodes.insert(node_info.node_id, node_info.clone());
        drop(nodes);
        
        // Notify subscribers
        let event = if is_new {
            DiscoveryEvent::NodeDiscovered(node_info)
        } else {
            DiscoveryEvent::NodeUpdated(node_info)
        };
        
        let subscribers = self.subscribers.read().await;
        for sender in subscribers.iter() {
            let _ = sender.send(event.clone()).await;
        }
        
        Ok(())
    }
    
    /// Get all currently discovered nodes
    pub async fn get_all_nodes(&self) -> BlixardResult<Vec<IrohNodeInfo>> {
        let nodes = self.nodes.read().await;
        Ok(nodes.values().cloned().collect())
    }
    
    /// Background task to aggregate events from all providers
    async fn event_aggregation_task(manager: Arc<RwLock<DiscoveryManager>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        
        loop {
            interval.tick().await;
            
            let manager_read = manager.read().await;
            if !*manager_read.running.read().await {
                break;
            }
            
            // Collect all nodes from all providers
            let mut all_nodes = HashMap::new();
            let providers = manager_read.providers.read().await;
            
            for provider in providers.iter() {
                match provider.get_nodes().await {
                    Ok(nodes) => {
                        for node in nodes {
                            all_nodes.insert(node.node_id, node);
                        }
                    }
                    Err(e) => {
                        error!("Failed to get nodes from provider {}: {}", provider.name(), e);
                    }
                }
            }
            
            // Update our aggregated view
            let mut current_nodes = manager_read.nodes.write().await;
            
            // Find new and updated nodes
            for (node_id, node_info) in &all_nodes {
                let event = if !current_nodes.contains_key(node_id) {
                    Some(DiscoveryEvent::NodeDiscovered(node_info.clone()))
                } else if current_nodes.get(node_id) != Some(node_info) {
                    Some(DiscoveryEvent::NodeUpdated(node_info.clone()))
                } else {
                    None
                };
                
                if let Some(event) = event {
                    // Notify subscribers
                    let subscribers = manager_read.subscribers.read().await;
                    for sender in subscribers.iter() {
                        let _ = sender.send(event.clone()).await;
                    }
                }
            }
            
            // Find removed nodes
            let removed_nodes: Vec<NodeId> = current_nodes
                .keys()
                .filter(|id| !all_nodes.contains_key(*id))
                .cloned()
                .collect();
            
            for node_id in removed_nodes {
                let subscribers = manager_read.subscribers.read().await;
                for sender in subscribers.iter() {
                    let _ = sender.send(DiscoveryEvent::NodeRemoved(node_id)).await;
                }
            }
            
            // Update the current nodes
            *current_nodes = all_nodes;
            
            // Clean up stale subscribers
            let mut subscribers = manager_read.subscribers.write().await;
            subscribers.retain(|sender| !sender.is_closed());
        }
    }
}

// Implement Clone manually
impl Clone for DiscoveryManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            providers: Arc::new(RwLock::new(Vec::new())),
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            task_handle: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    
    #[tokio::test]
    async fn test_discovery_manager_lifecycle() {
        let config = DiscoveryConfig {
            enable_static: true,
            enable_dns: false,
            enable_mdns: false,
            ..Default::default()
        };
        
        let mut manager = DiscoveryManager::new(config);
        
        // Should start successfully
        manager.start().await.unwrap();
        assert!(manager.is_running());
        
        // Should get empty nodes initially
        let nodes = manager.get_nodes().await;
        assert_eq!(nodes.len(), 0);
        
        // Should stop successfully
        manager.stop().await.unwrap();
        assert!(!manager.is_running());
    }
    
    #[tokio::test]
    async fn test_discovery_manager_with_static_nodes() {
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        let mut static_nodes = HashMap::new();
        static_nodes.insert(
            node_id.to_string(),
            vec!["127.0.0.1:8080".to_string()],
        );
        
        let config = DiscoveryConfig {
            enable_static: true,
            enable_dns: false,
            enable_mdns: false,
            static_nodes,
            ..Default::default()
        };
        
        let mut manager = DiscoveryManager::new(config);
        
        // Subscribe before starting
        let mut events = manager.subscribe().await;
        
        // Start the manager
        manager.start().await.unwrap();
        
        // Wait a bit for event aggregation
        tokio::time::sleep(Duration::from_millis(1500)).await;
        
        // Should have discovered the static node
        let nodes = manager.get_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, node_id);
        
        // Should have received discovery event
        tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(event) = events.recv().await {
                if let DiscoveryEvent::NodeDiscovered(info) = event {
                    assert_eq!(info.node_id, node_id);
                    break;
                }
            }
        }).await.expect("Should receive discovery event");
        
        manager.stop().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_discovery_manager_configure_for_node() {
        let node_id = NodeId::from_bytes(&[2u8; 32]).unwrap();
        let addr: SocketAddr = "192.168.1.100:9000".parse().unwrap();
        
        let mut config = DiscoveryConfig {
            enable_static: false,
            enable_dns: false,
            enable_mdns: true,
            ..Default::default()
        };
        
        let mut manager = DiscoveryManager::new(config.clone());
        manager.configure_for_node(node_id, vec![addr]);
        
        // Should have added internal config
        assert!(manager.config.static_nodes.contains_key("_mdns_self"));
    }
}