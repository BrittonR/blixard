//! mDNS (multicast DNS) discovery provider for local network discovery.
//!
//! This provider uses mDNS to announce and discover Iroh nodes on the local network.
//! It's particularly useful for development and local deployments.

use async_trait::async_trait;
use iroh::NodeId;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, info};

use crate::discovery::{DiscoveryEvent, DiscoveryProvider, DiscoveryState, IrohNodeInfo};
use crate::error::{BlixardError, BlixardResult};

/// mDNS discovery provider for local network node discovery
pub struct MdnsDiscoveryProvider {
    /// Service name to advertise and discover
    service_name: String,
    
    /// Our own node information to advertise
    our_node_id: Option<NodeId>,
    our_addresses: Vec<SocketAddr>,
    
    /// mDNS service daemon
    mdns: Arc<RwLock<Option<ServiceDaemon>>>,
    
    /// Shared discovery state
    state: Arc<DiscoveryState>,
    
    /// Whether the provider is running
    running: Arc<RwLock<bool>>,
    
    /// Background task handle
    task_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    
    /// Event channel for subscribers
    event_sender: mpsc::Sender<DiscoveryEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::Receiver<DiscoveryEvent>>>>,
}

impl MdnsDiscoveryProvider {
    /// Create a new mDNS discovery provider
    pub fn new(service_name: String) -> Self {
        let (tx, rx) = mpsc::channel(100);
        
        Self {
            service_name,
            our_node_id: None,
            our_addresses: Vec::new(),
            mdns: Arc::new(RwLock::new(None)),
            state: Arc::new(DiscoveryState::new()),
            running: Arc::new(RwLock::new(false)),
            task_handle: Arc::new(RwLock::new(None)),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
        }
    }
    
    /// Set our own node information to advertise
    pub fn set_our_node_info(&mut self, node_id: NodeId, addresses: Vec<SocketAddr>) {
        self.our_node_id = Some(node_id);
        self.our_addresses = addresses;
    }
    
    /// Create service info for advertising our node
    fn create_service_info(&self) -> BlixardResult<ServiceInfo> {
        let node_id = self.our_node_id.ok_or(BlixardError::Configuration {
            message: "Node ID not set for mDNS advertisement".to_string(),
        })?;
        
        if self.our_addresses.is_empty() {
            return Err(BlixardError::Configuration {
                message: "No addresses configured for mDNS advertisement".to_string(),
            });
        }
        
        // Use the node ID as the instance name
        let instance_name = node_id.to_string();
        
        // Extract service type and protocol from service name
        // Expected format: _service._protocol.local
        let parts: Vec<&str> = self.service_name.split('.').collect();
        if parts.len() < 2 {
            return Err(BlixardError::Configuration {
                message: format!("Invalid mDNS service name: {}", self.service_name),
            });
        }
        
        let service_type = parts[0].trim_start_matches('_');
        let protocol = parts[1].trim_start_matches('_');
        
        // Use the first address for the primary advertisement
        let primary_addr = self.our_addresses[0];
        let port = primary_addr.port();
        
        // Create properties with additional addresses
        let mut properties = HashMap::new();
        properties.insert("node_id".to_string(), node_id.to_string());
        
        // Add all addresses as properties
        for (i, addr) in self.our_addresses.iter().enumerate() {
            properties.insert(format!("addr{}", i), addr.to_string());
        }
        
        // Create service info
        let service_info = ServiceInfo::new(
            &format!("_{}._{}", service_type, protocol),
            &instance_name,
            "",  // hostname will be filled by mDNS
            primary_addr.ip(),
            port,
            Some(properties),
        ).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create service info: {}", e),
        })?;
        
        Ok(service_info)
    }
    
    /// Parse service info into IrohNodeInfo
    fn parse_service_info(&self, info: &ServiceInfo) -> Option<IrohNodeInfo> {
        // Get node ID from properties
        let node_id = info.get_property("node_id")
            .and_then(|val| Some(val.val_str()))
            .and_then(|s| s.parse::<NodeId>().ok())?;
        
        // Collect all addresses
        let mut addresses = Vec::new();
        
        // Add the primary address from service info
        // mdns-sd returns addresses as a set of IpAddr
        for addr in info.get_addresses() {
            addresses.push(SocketAddr::new(*addr, info.get_port()));
        }
        
        // Add additional addresses from properties
        for i in 0..10 {  // Check up to 10 additional addresses
            if let Some(addr_str) = info.get_property(&format!("addr{}", i))
                .and_then(|val| Some(val.val_str())) {
                if let Ok(addr) = addr_str.parse::<SocketAddr>() {
                    if !addresses.contains(&addr) {
                        addresses.push(addr);
                    }
                }
            }
        }
        
        if addresses.is_empty() {
            return None;
        }
        
        let mut node_info = IrohNodeInfo::new(node_id, addresses);
        node_info.name = Some(info.get_fullname().to_string());
        node_info.metadata.insert("source".to_string(), "mdns".to_string());
        
        Some(node_info)
    }
    
    /// Background task to handle mDNS events
    async fn mdns_task(
        provider: Arc<RwLock<Self>>,
        mut receiver: mpsc::Receiver<ServiceEvent>,
    ) {
        while let Some(event) = receiver.recv().await {
            let provider = provider.read().await;
            
            if !*provider.running.read().await {
                break;
            }
            
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    debug!("mDNS service resolved: {}", info.get_fullname());
                    
                    if let Some(node_info) = provider.parse_service_info(&info) {
                        // Don't add our own node
                        if Some(node_info.node_id) != provider.our_node_id {
                            info!("Discovered node via mDNS: {}", node_info.node_id);
                            provider.state.add_node(node_info.clone()).await;
                            let _ = provider.event_sender.send(
                                DiscoveryEvent::NodeDiscovered(node_info)
                            ).await;
                        }
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    debug!("mDNS service removed: {}", fullname);
                    
                    // Find and remove the node by name
                    let nodes = provider.state.nodes.read().await;
                    let node_to_remove = nodes.iter()
                        .find(|(_, info)| info.name.as_deref() == Some(&fullname))
                        .map(|(id, _)| *id);
                    drop(nodes);
                    
                    if let Some(node_id) = node_to_remove {
                        info!("Node removed via mDNS: {}", node_id);
                        provider.state.remove_node(node_id).await;
                    }
                }
                _ => {
                    // Ignore other events
                }
            }
        }
    }
}

#[async_trait]
impl DiscoveryProvider for MdnsDiscoveryProvider {
    async fn start(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(BlixardError::Internal {
                message: "mDNS discovery provider is already running".to_string(),
            });
        }
        
        info!("Starting mDNS discovery provider with service: {}", self.service_name);
        
        // Create mDNS daemon
        let mdns = ServiceDaemon::new().map_err(|e| BlixardError::Internal {
            message: format!("Failed to create mDNS daemon: {}", e),
        })?;
        
        // Start browsing for services
        let service_receiver = mdns.browse(&self.service_name).map_err(|e| BlixardError::Internal {
            message: format!("Failed to browse mDNS services: {}", e),
        })?;
        
        // Create a channel to convert ServiceEvent stream to mpsc
        let (tx, rx) = mpsc::channel(100);
        
        // Spawn a task to forward events
        tokio::spawn(async move {
            let mut service_rx = service_receiver;
            while let Ok(event) = service_rx.recv_async().await {
                let _ = tx.send(event).await;
            }
        });
        
        // Register our own service if we have node info
        if self.our_node_id.is_some() {
            let service_info = self.create_service_info()?;
            mdns.register(service_info).map_err(|e| BlixardError::Internal {
                message: format!("Failed to register mDNS service: {}", e),
            })?;
            
            info!("Registered mDNS service for node: {:?}", self.our_node_id);
        }
        
        *self.mdns.write().await = Some(mdns);
        *running = true;
        
        // Start background task to handle events
        let provider = Arc::new(RwLock::new(self.clone()));
        let handle = tokio::spawn(Self::mdns_task(provider, rx));
        *self.task_handle.write().await = Some(handle);
        
        Ok(())
    }
    
    async fn stop(&mut self) -> BlixardResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        info!("Stopping mDNS discovery provider");
        
        *running = false;
        
        // Cancel background task
        if let Some(handle) = self.task_handle.write().await.take() {
            handle.abort();
        }
        
        // Shutdown mDNS daemon
        if let Some(mdns) = self.mdns.write().await.take() {
            // Give it a moment to send unregister messages
            sleep(Duration::from_millis(100)).await;
            drop(mdns);
        }
        
        // Clear mDNS-discovered nodes
        let nodes = self.state.nodes.read().await;
        let mdns_nodes: Vec<NodeId> = nodes
            .iter()
            .filter(|(_, info)| info.metadata.get("source") == Some(&"mdns".to_string()))
            .map(|(id, _)| *id)
            .collect();
        drop(nodes);
        
        for node_id in mdns_nodes {
            self.state.remove_node(node_id).await;
        }
        
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
        "mdns"
    }
    
    fn is_running(&self) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                *self.running.read().await
            })
        })
    }
}

// Implement Clone manually
impl Clone for MdnsDiscoveryProvider {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(100);
        
        Self {
            service_name: self.service_name.clone(),
            our_node_id: self.our_node_id,
            our_addresses: self.our_addresses.clone(),
            mdns: Arc::new(RwLock::new(None)),
            state: self.state.clone(),
            running: self.running.clone(),
            task_handle: Arc::new(RwLock::new(None)),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    
    #[test]
    fn test_mdns_discovery_creation() {
        let provider = MdnsDiscoveryProvider::new("_blixard-iroh._tcp.local".to_string());
        
        assert!(!provider.is_running());
        assert_eq!(provider.name(), "mdns");
    }
    
    #[tokio::test]
    async fn test_service_info_creation() {
        let mut provider = MdnsDiscoveryProvider::new("_blixard-iroh._tcp.local".to_string());
        
        // Should fail without node info
        assert!(provider.create_service_info().is_err());
        
        // Set node info
        let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
        provider.set_our_node_info(node_id, vec![
            SocketAddr::from_str("127.0.0.1:8080").unwrap(),
            SocketAddr::from_str("[::1]:8080").unwrap(),
        ]);
        
        // Should succeed now
        let info = provider.create_service_info().unwrap();
        assert_eq!(info.get_properties().get("node_id").unwrap().to_string(), node_id.to_string());
    }
    
    #[test]
    fn test_service_info_parsing() {
        let provider = MdnsDiscoveryProvider::new("_blixard-iroh._tcp.local".to_string());
        
        let node_id = NodeId::from_bytes(&[2u8; 32]).unwrap();
        let mut properties = HashMap::new();
        properties.insert("node_id".to_string(), node_id.to_string());
        properties.insert("addr0".to_string(), "192.168.1.100:9000".to_string());
        
        let service_info = ServiceInfo::new(
            "_blixard-iroh._tcp",
            &node_id.to_string(),
            "",
            IpAddr::V4("192.168.1.100".parse().unwrap()),
            9000,
            Some(properties),
        ).unwrap();
        
        let parsed = provider.parse_service_info(&service_info).unwrap();
        assert_eq!(parsed.node_id, node_id);
        assert!(!parsed.addresses.is_empty());
    }
}