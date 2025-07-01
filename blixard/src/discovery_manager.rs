//! Discovery manager for automatic peer discovery
//!
//! This module provides automatic peer discovery using multiple mechanisms:
//! - Static configuration files
//! - DNS-based discovery
//! - mDNS local network discovery (future)
//! - Cloud provider metadata services (future)

use blixard_core::{
    error::{BlixardError, BlixardResult},
    transport::iroh_peer_connector::IrohPeerConnector,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::Path;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

/// Discovery source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoverySource {
    /// Static configuration file
    StaticFile { path: String },
    /// DNS-based discovery
    Dns { domain: String },
    /// Node registry files (path or URL)
    NodeRegistry { locations: Vec<String> },
}

/// Discovered peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    /// Cluster node ID
    pub node_id: u64,
    /// Iroh node ID (base64)
    pub iroh_node_id: String,
    /// Direct addresses
    pub direct_addresses: Vec<String>,
    /// Relay URL if available
    pub relay_url: Option<String>,
    /// Discovery source
    pub source: String,
    /// When this peer was discovered
    pub discovered_at: std::time::SystemTime,
}

/// Discovery manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Discovery sources to use
    pub sources: Vec<DiscoverySource>,
    /// Refresh interval
    pub refresh_interval: Duration,
    /// Enable automatic connection to discovered peers
    pub auto_connect: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            sources: vec![],
            refresh_interval: Duration::from_secs(60),
            auto_connect: true,
        }
    }
}

/// Discovery manager for automatic peer discovery
pub struct DiscoveryManager {
    /// Configuration
    config: DiscoveryConfig,
    /// Discovered peers
    peers: Arc<RwLock<HashMap<u64, DiscoveredPeer>>>,
    /// Node discovery instance
    node_discovery: Arc<RwLock<crate::node_discovery::NodeDiscovery>>,
    /// Peer connector for establishing connections
    peer_connector: Option<Arc<IrohPeerConnector>>,
}

impl DiscoveryManager {
    /// Create a new discovery manager
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            node_discovery: Arc::new(RwLock::new(crate::node_discovery::NodeDiscovery::new())),
            peer_connector: None,
        }
    }

    /// Set the peer connector for automatic connections
    pub fn set_peer_connector(&mut self, peer_connector: Arc<IrohPeerConnector>) {
        self.peer_connector = Some(peer_connector);
    }

    /// Start the discovery process
    pub async fn start(&self) -> BlixardResult<()> {
        info!("Starting discovery manager with {} sources", self.config.sources.len());
        
        // Initial discovery
        self.refresh_peers().await?;
        
        // Start periodic refresh task
        let peers = self.peers.clone();
        let config = self.config.clone();
        let node_discovery = self.node_discovery.clone();
        let peer_connector = self.peer_connector.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(config.refresh_interval);
            loop {
                interval.tick().await;
                
                // Refresh peers from all sources
                for source in &config.sources {
                    if let Err(e) = Self::discover_from_source(
                        source, 
                        &peers, 
                        &node_discovery,
                        peer_connector.as_ref(),
                        config.auto_connect
                    ).await {
                        warn!("Discovery source {:?} failed: {}", source, e);
                    }
                }
            }
        });
        
        Ok(())
    }

    /// Refresh peers from all sources
    pub async fn refresh_peers(&self) -> BlixardResult<()> {
        for source in &self.config.sources {
            if let Err(e) = Self::discover_from_source(
                source, 
                &self.peers, 
                &self.node_discovery,
                self.peer_connector.as_ref(),
                self.config.auto_connect
            ).await {
                warn!("Discovery source {:?} failed: {}", source, e);
            }
        }
        
        Ok(())
    }

    /// Discover peers from a specific source
    async fn discover_from_source(
        source: &DiscoverySource,
        peers: &Arc<RwLock<HashMap<u64, DiscoveredPeer>>>,
        node_discovery: &Arc<RwLock<crate::node_discovery::NodeDiscovery>>,
        peer_connector: Option<&Arc<IrohPeerConnector>>,
        auto_connect: bool,
    ) -> BlixardResult<()> {
        match source {
            DiscoverySource::StaticFile { path } => {
                Self::discover_from_static_file(path, peers, node_discovery, peer_connector, auto_connect).await
            }
            DiscoverySource::NodeRegistry { locations } => {
                for location in locations {
                    if let Err(e) = Self::discover_from_registry(
                        location, 
                        peers, 
                        node_discovery,
                        peer_connector,
                        auto_connect
                    ).await {
                        warn!("Failed to discover from registry {}: {}", location, e);
                    }
                }
                Ok(())
            }
            DiscoverySource::Dns { domain } => {
                Self::discover_from_dns(domain, peers, node_discovery, peer_connector, auto_connect).await
            }
        }
    }

    /// Discover peers from a static configuration file
    async fn discover_from_static_file(
        path: &str,
        peers: &Arc<RwLock<HashMap<u64, DiscoveredPeer>>>,
        _node_discovery: &Arc<RwLock<crate::node_discovery::NodeDiscovery>>,
        peer_connector: Option<&Arc<IrohPeerConnector>>,
        auto_connect: bool,
    ) -> BlixardResult<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read discovery file {}: {}", path, e),
            })?;
        
        let discovered_peers: Vec<DiscoveredPeer> = serde_json::from_str(&content)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to parse discovery file: {}", e),
            })?;
        
        let mut peers_guard = peers.write().await;
        for mut peer in discovered_peers {
            peer.source = format!("static:{}", path);
            peer.discovered_at = std::time::SystemTime::now();
            
            info!("Discovered peer {} from static file", peer.node_id);
            
            // Auto-connect if enabled
            if auto_connect {
                if let Some(connector) = peer_connector {
                    Self::connect_to_peer(&peer, connector).await;
                }
            }
            
            peers_guard.insert(peer.node_id, peer);
        }
        
        Ok(())
    }

    /// Discover from node registry (file or URL)
    async fn discover_from_registry(
        location: &str,
        peers: &Arc<RwLock<HashMap<u64, DiscoveredPeer>>>,
        node_discovery: &Arc<RwLock<crate::node_discovery::NodeDiscovery>>,
        peer_connector: Option<&Arc<IrohPeerConnector>>,
        auto_connect: bool,
    ) -> BlixardResult<()> {
        let mut discovery = node_discovery.write().await;
        let entry = discovery.discover_node(location).await?;
        
        let peer = DiscoveredPeer {
            node_id: entry.cluster_node_id,
            iroh_node_id: entry.iroh_node_id,
            direct_addresses: entry.direct_addresses,
            relay_url: entry.relay_url,
            source: format!("registry:{}", location),
            discovered_at: std::time::SystemTime::now(),
        };
        
        info!("Discovered peer {} from registry", peer.node_id);
        
        // Auto-connect if enabled
        if auto_connect {
            if let Some(connector) = peer_connector {
                Self::connect_to_peer(&peer, connector).await;
            }
        }
        
        let mut peers_guard = peers.write().await;
        peers_guard.insert(peer.node_id, peer);
        
        Ok(())
    }

    /// Discover peers from DNS
    async fn discover_from_dns(
        _domain: &str,
        _peers: &Arc<RwLock<HashMap<u64, DiscoveredPeer>>>,
        _node_discovery: &Arc<RwLock<crate::node_discovery::NodeDiscovery>>,
        _peer_connector: Option<&Arc<IrohPeerConnector>>,
        _auto_connect: bool,
    ) -> BlixardResult<()> {
        // TODO: Implement DNS-based discovery
        // This could use SRV records or TXT records to discover peers
        warn!("DNS discovery not yet implemented");
        Ok(())
    }

    /// Connect to a discovered peer
    async fn connect_to_peer(peer: &DiscoveredPeer, connector: &Arc<IrohPeerConnector>) {
        // Parse Iroh node ID
        let node_id_bytes = match base64::decode(&peer.iroh_node_id) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("Failed to decode Iroh node ID for peer {}: {}", peer.node_id, e);
                return;
            }
        };
        
        let iroh_node_id = match iroh::NodeId::from_bytes(&node_id_bytes) {
            Ok(id) => id,
            Err(e) => {
                error!("Invalid Iroh node ID for peer {}: {}", peer.node_id, e);
                return;
            }
        };
        
        // Create NodeAddr
        let mut builder = iroh::NodeAddr::new(iroh_node_id);
        
        // Add direct addresses
        for addr in &peer.direct_addresses {
            if let Ok(socket_addr) = addr.parse() {
                builder = builder.with_direct_addr(socket_addr);
            }
        }
        
        // Add relay URL
        if let Some(relay_url) = &peer.relay_url {
            if let Ok(url) = relay_url.parse() {
                builder = builder.with_relay_url(url);
            }
        }
        
        let node_addr = builder.build();
        
        // Attempt connection
        info!("Auto-connecting to discovered peer {} at {}", peer.node_id, iroh_node_id);
        if let Err(e) = connector.connect_to_peer(peer.node_id, node_addr).await {
            warn!("Failed to auto-connect to peer {}: {}", peer.node_id, e);
        } else {
            info!("Successfully connected to peer {}", peer.node_id);
        }
    }

    /// Get all discovered peers
    pub async fn get_peers(&self) -> Vec<DiscoveredPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Get a specific peer by node ID
    pub async fn get_peer(&self, node_id: u64) -> Option<DiscoveredPeer> {
        self.peers.read().await.get(&node_id).cloned()
    }

    /// Manually add a discovered peer
    pub async fn add_peer(&self, peer: DiscoveredPeer) -> BlixardResult<()> {
        info!("Manually adding peer {} to discovery", peer.node_id);
        
        // Auto-connect if enabled
        if self.config.auto_connect {
            if let Some(connector) = &self.peer_connector {
                Self::connect_to_peer(&peer, connector).await;
            }
        }
        
        self.peers.write().await.insert(peer.node_id, peer);
        Ok(())
    }
}

/// Create a discovery config from node registry paths
pub fn create_discovery_config(registry_paths: Vec<String>) -> DiscoveryConfig {
    DiscoveryConfig {
        sources: vec![
            DiscoverySource::NodeRegistry {
                locations: registry_paths,
            }
        ],
        refresh_interval: Duration::from_secs(30),
        auto_connect: true,
    }
}