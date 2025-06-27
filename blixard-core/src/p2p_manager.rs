//! Enhanced P2P Manager for Blixard
//!
//! This module provides comprehensive P2P management with:
//! - Automatic peer discovery
//! - Connection health monitoring
//! - Resource synchronization
//! - Transfer queue management

use crate::error::{BlixardError, BlixardResult};
use crate::iroh_transport::IrohTransport;
use crate::p2p_image_store::P2pImageStore;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{interval, Duration};
use serde::{Serialize, Deserialize};
use tracing::{info, debug};
use chrono::{DateTime, Utc};

/// P2P peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub address: String,
    pub last_seen: DateTime<Utc>,
    pub capabilities: Vec<String>,
    pub shared_resources: HashMap<String, ResourceInfo>,
    pub connection_quality: ConnectionQuality,
}

/// Resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub resource_type: ResourceType,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub hash: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    VmImage,
    Configuration,
    Logs,
    Metrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQuality {
    pub latency_ms: u64,
    pub bandwidth_mbps: f64,
    pub packet_loss: f64,
    pub reliability_score: f64,
}

/// Transfer request
#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub id: String,
    pub resource_type: ResourceType,
    pub name: String,
    pub version: String,
    pub source_peer: Option<String>,
    pub priority: TransferPriority,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransferPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Active transfer tracking
#[derive(Debug, Clone)]
pub struct ActiveTransfer {
    pub id: String,
    pub request: TransferRequest,
    pub progress: TransferProgress,
    pub started_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone)]
pub enum TransferStatus {
    Queued,
    Connecting,
    Downloading,
    Uploading,
    Verifying,
    Completed,
    Failed(String),
}

/// P2P Manager events
#[derive(Debug, Clone)]
pub enum P2pEvent {
    PeerDiscovered(PeerInfo),
    PeerConnected(String),
    PeerDisconnected(String),
    ResourceAvailable(ResourceInfo),
    TransferStarted(String),
    TransferProgress(String, TransferProgress),
    TransferCompleted(String),
    TransferFailed(String, String),
}

/// Enhanced P2P Manager
pub struct P2pManager {
    node_id: u64,
    transport: Arc<IrohTransport>,
    image_store: Arc<P2pImageStore>,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    transfer_queue: Arc<RwLock<VecDeque<TransferRequest>>>,
    active_transfers: Arc<RwLock<HashMap<String, ActiveTransfer>>>,
    event_tx: mpsc::Sender<P2pEvent>,
    event_rx: Arc<RwLock<mpsc::Receiver<P2pEvent>>>,
    config: P2pConfig,
}

/// P2P configuration
#[derive(Debug, Clone)]
pub struct P2pConfig {
    pub max_concurrent_transfers: usize,
    pub transfer_timeout: Duration,
    pub peer_discovery_interval: Duration,
    pub health_check_interval: Duration,
    pub max_retry_attempts: u32,
    pub bandwidth_limit_mbps: Option<f64>,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            max_concurrent_transfers: 3,
            transfer_timeout: Duration::from_secs(300),
            peer_discovery_interval: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            max_retry_attempts: 3,
            bandwidth_limit_mbps: None,
        }
    }
}

impl P2pManager {
    /// Create a new P2P manager
    pub async fn new(
        node_id: u64,
        data_dir: &Path,
        config: P2pConfig,
    ) -> BlixardResult<Self> {
        let transport = Arc::new(IrohTransport::new(node_id, data_dir).await?);
        let image_store = Arc::new(P2pImageStore::new(node_id, data_dir).await?);
        
        let (event_tx, event_rx) = mpsc::channel(1000);
        
        Ok(Self {
            node_id,
            transport,
            image_store,
            peers: Arc::new(RwLock::new(HashMap::new())),
            transfer_queue: Arc::new(RwLock::new(VecDeque::new())),
            active_transfers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
            config,
        })
    }
    
    /// Start the P2P manager
    pub async fn start(&self) -> BlixardResult<()> {
        info!("Starting P2P manager for node {}", self.node_id);
        
        // Start background tasks
        self.start_peer_discovery();
        self.start_health_monitoring();
        self.start_transfer_processor();
        self.start_resource_announcer();
        
        Ok(())
    }
    
    /// Get event receiver
    pub fn event_receiver(&self) -> Arc<RwLock<mpsc::Receiver<P2pEvent>>> {
        self.event_rx.clone()
    }
    
    /// Connect to a peer
    pub async fn connect_peer(&self, peer_addr: &str) -> BlixardResult<()> {
        info!("Connecting to peer at {}", peer_addr);
        
        // In a real implementation, this would establish connection via Iroh
        // For now, we'll simulate it
        let peer_info = PeerInfo {
            node_id: format!("peer-{}", uuid::Uuid::new_v4()),
            address: peer_addr.to_string(),
            last_seen: Utc::now(),
            capabilities: vec!["vm-images".to_string(), "logs".to_string()],
            shared_resources: HashMap::new(),
            connection_quality: ConnectionQuality {
                latency_ms: 15,
                bandwidth_mbps: 100.0,
                packet_loss: 0.0,
                reliability_score: 0.99,
            },
        };
        
        self.peers.write().await.insert(peer_info.node_id.clone(), peer_info.clone());
        self.event_tx.send(P2pEvent::PeerConnected(peer_info.node_id.clone())).await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to send P2P event: {}", e),
            })?;
        
        Ok(())
    }
    
    /// Request a resource download
    pub async fn request_download(
        &self,
        resource_type: ResourceType,
        name: &str,
        version: &str,
        priority: TransferPriority,
    ) -> BlixardResult<String> {
        let request = TransferRequest {
            id: uuid::Uuid::new_v4().to_string(),
            resource_type,
            name: name.to_string(),
            version: version.to_string(),
            source_peer: None,
            priority,
            requested_at: Utc::now(),
        };
        
        let request_id = request.id.clone();
        self.transfer_queue.write().await.push_back(request);
        
        Ok(request_id)
    }
    
    /// Upload a resource
    pub async fn upload_resource(
        &self,
        resource_type: ResourceType,
        name: &str,
        version: &str,
        path: &Path,
    ) -> BlixardResult<()> {
        match resource_type {
            ResourceType::VmImage => {
                // Use the image store for VM images
                self.image_store.upload_image(
                    name,
                    version,
                    "Uploaded via P2P manager",
                    path,
                    "unknown",
                    "unknown",
                    vec![],
                ).await?;
            }
            _ => {
                // For other types, use raw file sharing
                let hash = self.transport.share_file(path).await?;
                info!("Shared {} {} v{} with hash {}", 
                    format!("{:?}", resource_type).to_lowercase(), 
                    name, version, hash
                );
            }
        }
        
        Ok(())
    }
    
    /// Get current peers
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }
    
    /// Get active transfers
    pub async fn get_active_transfers(&self) -> Vec<ActiveTransfer> {
        self.active_transfers.read().await.values().cloned().collect()
    }
    
    /// Start peer discovery task
    fn start_peer_discovery(&self) {
        let _peers = self.peers.clone();
        let _event_tx = self.event_tx.clone();
        let interval_duration = self.config.peer_discovery_interval;
        
        tokio::spawn(async move {
            let mut interval = interval(interval_duration);
            loop {
                interval.tick().await;
                
                // In a real implementation, this would use Iroh's discovery
                // For now, we'll simulate finding peers
                debug!("Running peer discovery");
            }
        });
    }
    
    /// Start health monitoring task
    fn start_health_monitoring(&self) {
        let peers = self.peers.clone();
        let event_tx = self.event_tx.clone();
        let interval_duration = self.config.health_check_interval;
        
        tokio::spawn(async move {
            let mut interval = interval(interval_duration);
            loop {
                interval.tick().await;
                
                let mut peers_guard = peers.write().await;
                let now = Utc::now();
                
                // Check peer health
                let mut disconnected = Vec::new();
                for (id, peer) in peers_guard.iter_mut() {
                    let elapsed = now.signed_duration_since(peer.last_seen);
                    if elapsed.num_seconds() > 120 {
                        disconnected.push(id.clone());
                    }
                }
                
                // Remove disconnected peers
                for id in disconnected {
                    peers_guard.remove(&id);
                    let _ = event_tx.send(P2pEvent::PeerDisconnected(id)).await;
                }
            }
        });
    }
    
    /// Start transfer processor task
    fn start_transfer_processor(&self) {
        let transfer_queue = self.transfer_queue.clone();
        let active_transfers = self.active_transfers.clone();
        let event_tx = self.event_tx.clone();
        let max_concurrent = self.config.max_concurrent_transfers;
        let image_store = self.image_store.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                
                let active_count = active_transfers.read().await.len();
                if active_count >= max_concurrent {
                    continue;
                }
                
                // Get next transfer from queue
                let request = {
                    let mut queue = transfer_queue.write().await;
                    queue.pop_front()
                };
                
                if let Some(request) = request {
                    let request_id = request.id.clone();
                    let active_transfer = ActiveTransfer {
                        id: request_id.clone(),
                        request: request.clone(),
                        progress: TransferProgress {
                            bytes_transferred: 0,
                            total_bytes: 0,
                            speed_bps: 0,
                            status: TransferStatus::Connecting,
                        },
                        started_at: Utc::now(),
                        estimated_completion: None,
                    };
                    
                    active_transfers.write().await.insert(request_id.clone(), active_transfer);
                    let _ = event_tx.send(P2pEvent::TransferStarted(request_id.clone())).await;
                    
                    // Process the transfer
                    match request.resource_type {
                        ResourceType::VmImage => {
                            // Download VM image
                            match image_store.download_image(&request.name, &request.version).await {
                                Ok(_) => {
                                    let _ = event_tx.send(P2pEvent::TransferCompleted(request_id.clone())).await;
                                }
                                Err(e) => {
                                    let _ = event_tx.send(P2pEvent::TransferFailed(
                                        request_id.clone(),
                                        e.to_string()
                                    )).await;
                                }
                            }
                        }
                        _ => {
                            // Handle other resource types
                        }
                    }
                    
                    active_transfers.write().await.remove(&request_id);
                }
            }
        });
    }
    
    /// Start resource announcer task
    fn start_resource_announcer(&self) {
        let image_store = self.image_store.clone();
        let event_tx = self.event_tx.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                
                // Announce available resources
                if let Ok(images) = image_store.list_images().await {
                    for image in images {
                        let resource_info = ResourceInfo {
                            resource_type: ResourceType::VmImage,
                            name: image.name.clone(),
                            version: image.version.clone(),
                            size: image.size,
                            hash: image.content_hash.clone(),
                            available: true,
                        };
                        
                        let _ = event_tx.send(P2pEvent::ResourceAvailable(resource_info)).await;
                    }
                }
            }
        });
    }
}

/// Format bytes to human readable string
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}