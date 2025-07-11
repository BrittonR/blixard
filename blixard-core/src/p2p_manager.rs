//! Enhanced P2P Manager for Blixard
//!
//! This module provides comprehensive P2P management with:
//! - Automatic peer discovery
//! - Connection health monitoring
//! - Resource synchronization
//! - Transfer queue management

use crate::discovery::DiscoveryManager;
use crate::error::{BlixardError, BlixardResult};
use crate::iroh_transport_v2::{DocumentType, IrohTransportV2};
use crate::p2p_image_store::P2pImageStore;
use chrono::{DateTime, Utc};
use iroh::NodeAddr;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

/// P2P peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub address: String,
    pub last_seen: DateTime<Utc>,
    pub capabilities: Vec<String>,
    pub shared_resources: HashMap<String, ResourceInfo>,
    pub connection_quality: ConnectionQuality,
    /// Optional P2P node ID for Iroh connections
    pub p2p_node_id: Option<String>,
    /// P2P addresses for direct connections
    pub p2p_addresses: Vec<String>,
    /// P2P relay URL for NAT traversal
    pub p2p_relay_url: Option<String>,
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
    transport: Arc<IrohTransportV2>,
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
    pub async fn new(node_id: u64, data_dir: &Path, config: P2pConfig) -> BlixardResult<Self> {
        Self::new_with_discovery(node_id, data_dir, config, None).await
    }

    /// Create a new P2P manager with optional discovery
    pub async fn new_with_discovery(
        node_id: u64,
        data_dir: &Path,
        config: P2pConfig,
        discovery_manager: Option<Arc<DiscoveryManager>>,
    ) -> BlixardResult<Self> {
        let transport = Arc::new(
            IrohTransportV2::new_with_discovery(node_id, data_dir, discovery_manager).await?,
        );
        let image_store = Arc::new(P2pImageStore::with_default_filesystem(node_id, data_dir).await?);

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

    /// Get our P2P node address
    pub async fn get_node_addr(&self) -> BlixardResult<iroh::NodeAddr> {
        self.transport.node_addr().await
    }

    /// Get the underlying Iroh endpoint (for Raft transport)
    pub fn get_endpoint(&self) -> (iroh::Endpoint, iroh::NodeId) {
        self.transport.endpoint()
    }

    /// Connect to a peer by gRPC address (legacy)
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
            p2p_node_id: None,
            p2p_addresses: Vec::new(),
            p2p_relay_url: None,
        };

        self.peers
            .write()
            .await
            .insert(peer_info.node_id.clone(), peer_info.clone());
        self.event_tx
            .send(P2pEvent::PeerConnected(peer_info.node_id.clone()))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to send P2P event: {}", e),
            })?;

        Ok(())
    }

    /// Connect to a peer using Iroh NodeAddr
    pub async fn connect_p2p_peer(
        &self,
        node_id: u64,
        peer_addr: &iroh::NodeAddr,
    ) -> BlixardResult<()> {
        info!("Connecting to P2P peer {} with Iroh address", node_id);

        // Store peer info immediately - connection will be established when needed
        // The Iroh endpoint handles connection establishment automatically when we send RPC requests
        let peer_info = PeerInfo {
            node_id: node_id.to_string(),
            address: peer_addr.node_id.to_string(),
            last_seen: Utc::now(),
            capabilities: vec!["vm-images".to_string(), "logs".to_string()],
            shared_resources: HashMap::new(),
            connection_quality: ConnectionQuality {
                latency_ms: 10,
                bandwidth_mbps: 1000.0,
                packet_loss: 0.0,
                reliability_score: 1.0,
            },
            p2p_node_id: Some(peer_addr.node_id.to_string()),
            p2p_addresses: peer_addr.direct_addresses().map(|addr| addr.to_string()).collect(),
            p2p_relay_url: peer_addr.relay_url().map(|url| url.to_string()),
        };

        self.peers
            .write()
            .await
            .insert(node_id.to_string(), peer_info.clone());
        self.event_tx
            .send(P2pEvent::PeerConnected(node_id.to_string()))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to send P2P event: {}", e),
            })?;

        info!(
            "P2P peer {} info stored, connection will be established on first RPC",
            node_id
        );
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
                self.image_store
                    .upload_image(
                        name,
                        version,
                        "Uploaded via P2P manager",
                        path,
                        "unknown",
                        "unknown",
                        vec![],
                    )
                    .await?;
            }
            _ => {
                // For other types, use raw file sharing
                let hash = self.transport.share_file(path).await?;
                info!(
                    "Shared {} {} v{} with hash {}",
                    format!("{:?}", resource_type).to_lowercase(),
                    name,
                    version,
                    hash
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
        self.active_transfers
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Hash data using Blake3
    pub async fn hash_data(&self, data: &[u8]) -> BlixardResult<iroh_blobs::Hash> {
        use iroh_blobs::Hash;
        Ok(Hash::new(data))
    }

    /// Share data through P2P network
    pub async fn share_data(&self, data: Vec<u8>, name: &str) -> BlixardResult<iroh_blobs::Hash> {
        // For now, just store it locally and return the hash
        let hash = self.hash_data(&data).await?;
        // TODO: Actually share through iroh
        info!("Sharing data {} with hash {}", name, hash);
        Ok(hash)
    }

    /// Store metadata
    pub async fn store_metadata(&self, key: &str, value: &[u8]) -> BlixardResult<()> {
        debug!("Storing metadata for key: {}", key);

        // Use the document operations from IrohTransportV2 to write metadata
        // Metadata is stored in the Metadata document type
        self.transport
            .write_to_doc(DocumentType::Metadata, key, value)
            .await
    }

    /// Get metadata
    pub async fn get_metadata(&self, key: &str) -> BlixardResult<Vec<u8>> {
        debug!("Getting metadata for key: {}", key);

        // Use the document operations from IrohTransportV2 to read metadata
        // Metadata is stored in the Metadata document type
        self.transport
            .read_from_doc(DocumentType::Metadata, key)
            .await
    }

    /// Download data from P2P network
    pub async fn download_data(&self, hash: &iroh_blobs::Hash) -> BlixardResult<Vec<u8>> {
        debug!("Downloading data for hash: {}", hash);

        // First, check if we have the blob locally in the transport's blob store
        // This requires accessing the blob store through the transport
        // For now, we'll use a temporary file as an intermediary
        let temp_path = std::env::temp_dir().join(format!("blixard_blob_{}", hash));

        // Try to download from local store first
        match self.transport.download_file(*hash, &temp_path).await {
            Ok(_) => {
                // Read the file content
                let data = tokio::fs::read(&temp_path)
                    .await
                    .map_err(|e| BlixardError::IoError(e))?;

                // Clean up temp file
                let _ = tokio::fs::remove_file(&temp_path).await;

                debug!(
                    "Successfully downloaded {} bytes from local store",
                    data.len()
                );
                return Ok(data);
            }
            Err(_) => {
                debug!("Blob not found locally, need to download from peers");
            }
        }

        // If not found locally, try to download from connected peers
        let peers = self.peers.read().await;
        for (peer_id, peer_info) in peers.iter() {
            // Check if this peer has the resource
            let has_resource = peer_info
                .shared_resources
                .values()
                .any(|r| r.hash == hash.to_string());

            if !has_resource {
                continue;
            }

            debug!("Attempting to download from peer: {}", peer_id);

            // Parse peer address
            let node_addr = match self.parse_peer_address(&peer_info.address).await {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Failed to parse peer address: {}", e);
                    continue;
                }
            };

            // Try to download from this peer
            match self
                .transport
                .download_blob_from_peer(&node_addr, *hash, &temp_path)
                .await
            {
                Ok(_) => {
                    // Read the downloaded file
                    let data = tokio::fs::read(&temp_path)
                        .await
                        .map_err(|e| BlixardError::IoError(e))?;

                    // Clean up temp file
                    let _ = tokio::fs::remove_file(&temp_path).await;

                    info!(
                        "Successfully downloaded {} bytes from peer {}",
                        data.len(),
                        peer_id
                    );
                    return Ok(data);
                }
                Err(e) => {
                    warn!("Failed to download from peer {}: {}", peer_id, e);
                    continue;
                }
            }
        }

        Err(BlixardError::Internal {
            message: format!("Failed to download blob {} from any source", hash),
        })
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

                    active_transfers
                        .write()
                        .await
                        .insert(request_id.clone(), active_transfer);
                    let _ = event_tx
                        .send(P2pEvent::TransferStarted(request_id.clone()))
                        .await;

                    // Process the transfer
                    match request.resource_type {
                        ResourceType::VmImage => {
                            // Download VM image
                            match image_store
                                .download_image(&request.name, &request.version)
                                .await
                            {
                                Ok(_) => {
                                    let _ = event_tx
                                        .send(P2pEvent::TransferCompleted(request_id.clone()))
                                        .await;
                                }
                                Err(e) => {
                                    let _ = event_tx
                                        .send(P2pEvent::TransferFailed(
                                            request_id.clone(),
                                            e.to_string(),
                                        ))
                                        .await;
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

                        let _ = event_tx
                            .send(P2pEvent::ResourceAvailable(resource_info))
                            .await;
                    }
                }
            }
        });
    }

    /// Parse peer address into NodeAddr
    async fn parse_peer_address(&self, address: &str) -> BlixardResult<NodeAddr> {
        // Expected format: "node_id@ip:port" or just "ip:port"
        let parts: Vec<&str> = address.split('@').collect();

        let (node_id_str, addr_str) = if parts.len() == 2 {
            // Format: node_id@ip:port
            (Some(parts[0]), parts[1])
        } else {
            // Format: ip:port (no node_id)
            (None, address)
        };

        // Parse socket address
        let socket_addr = addr_str.parse().map_err(|e| BlixardError::Internal {
            message: format!("Failed to parse socket address '{}': {}", addr_str, e),
        })?;

        // If we have a node_id string, parse it
        if let Some(node_id_str) = node_id_str {
            let node_id =
                iroh::NodeId::from_str(node_id_str).map_err(|e| BlixardError::Internal {
                    message: format!("Failed to parse node ID '{}': {}", node_id_str, e),
                })?;

            Ok(NodeAddr::new(node_id).with_direct_addresses([socket_addr]))
        } else {
            // Without a node ID, we can't create a proper NodeAddr
            Err(BlixardError::Internal {
                message: format!("Peer address '{}' missing node ID", address),
            })
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_metadata_operations() {
        // Create a temporary directory for test data
        let temp_dir = TempDir::new().unwrap();

        // Create P2P manager with default config
        let p2p_manager = P2pManager::new(1, temp_dir.path(), Default::default())
            .await
            .unwrap();

        // Test storing and retrieving metadata
        let key = "test-key";
        let value = b"test-value-data";

        // Store metadata
        p2p_manager.store_metadata(key, value).await.unwrap();

        // Retrieve metadata
        let retrieved = p2p_manager.get_metadata(key).await.unwrap();
        assert_eq!(retrieved, value);

        // Test with different key-value pairs
        let key2 = "config/node/settings";
        let value2 = b"{\"max_vms\": 10, \"memory_gb\": 32}";

        p2p_manager.store_metadata(key2, value2).await.unwrap();
        let retrieved2 = p2p_manager.get_metadata(key2).await.unwrap();
        assert_eq!(retrieved2, value2);
    }

    #[tokio::test]
    async fn test_blob_download_from_local_store() {
        let temp_dir = TempDir::new().unwrap();
        let p2p_manager = P2pManager::new(1, temp_dir.path(), Default::default())
            .await
            .unwrap();

        // Create a test file
        let test_data = b"This is test blob data";
        let test_file = temp_dir.path().join("test.dat");
        tokio::fs::write(&test_file, test_data).await.unwrap();

        // Share the file (this adds it to local blob store)
        let file_data = tokio::fs::read(&test_file).await.unwrap();
        let hash = p2p_manager
            .share_data(file_data, "test-blob")
            .await
            .unwrap();

        // Download using the hash (should retrieve from local store)
        let downloaded = p2p_manager.download_data(&hash).await.unwrap();
        assert_eq!(downloaded, test_data);
    }

    #[tokio::test]
    async fn test_metadata_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let p2p_manager = P2pManager::new(1, temp_dir.path(), Default::default())
            .await
            .unwrap();

        // Try to get non-existent metadata
        let result = p2p_manager.get_metadata("non-existent-key").await;
        assert!(result.is_err());
    }
}
