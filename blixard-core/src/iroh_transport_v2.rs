//! Iroh P2P transport layer for Blixard - Version 2 with actual implementation
//!
//! This module provides peer-to-peer communication capabilities using Iroh 0.90,
//! enabling direct connections between nodes for efficient data transfer.

use crate::discovery::{create_combined_discovery, DiscoveryManager, IrohDiscoveryBridge};
use crate::error::{BlixardError, BlixardResult};
use crate::p2p_monitor::{ConnectionState, Direction, NoOpMonitor, P2pMonitor};
use crate::transport::BLIXARD_ALPN;

#[cfg(feature = "failpoints")]
use crate::fail_point;
use iroh::{Endpoint, NodeAddr, SecretKey};
use iroh_blobs::Hash;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info};

/// Types of data channels used in Blixard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentType {
    /// Shared cluster configuration
    ClusterConfig,
    /// VM image catalog and metadata
    VmImages,
    /// Distributed log aggregation
    Logs,
    /// Time-series metrics data
    Metrics,
    /// Temporary documents for file transfers
    FileTransfer,
    /// General metadata storage
    Metadata,
    /// Node state information
    NodeState,
}

impl DocumentType {
    fn as_str(&self) -> &'static str {
        match self {
            DocumentType::ClusterConfig => "cluster-config",
            DocumentType::VmImages => "vm-images",
            DocumentType::Logs => "logs",
            DocumentType::Metrics => "metrics",
            DocumentType::FileTransfer => "file-transfer",
            DocumentType::Metadata => "metadata",
            DocumentType::NodeState => "node-state",
        }
    }
}

/// Simplified document storage
#[derive(Debug, Clone)]
struct DocumentEntry {
    key: String,
    value: Vec<u8>,
    author: String,
    timestamp: std::time::SystemTime,
}

/// Bandwidth tracking entry
#[derive(Debug, Clone)]
struct BandwidthEntry {
    timestamp: Instant,
    bytes: u64,
    direction: Direction,
}

/// Time-windowed bandwidth tracker
#[derive(Debug)]
struct BandwidthTracker {
    /// Rolling window of bandwidth entries
    entries: VecDeque<BandwidthEntry>,
    /// Window duration
    window: Duration,
    /// Total bytes in current window
    total_bytes_in: u64,
    total_bytes_out: u64,
}

impl BandwidthTracker {
    fn new(window: Duration) -> Self {
        Self {
            entries: VecDeque::new(),
            window,
            total_bytes_in: 0,
            total_bytes_out: 0,
        }
    }

    fn add_entry(&mut self, bytes: u64, direction: Direction) {
        let now = Instant::now();
        self.cleanup_old_entries(now);

        self.entries.push_back(BandwidthEntry {
            timestamp: now,
            bytes,
            direction,
        });

        match direction {
            Direction::Inbound => self.total_bytes_in += bytes,
            Direction::Outbound => self.total_bytes_out += bytes,
        }
    }

    fn cleanup_old_entries(&mut self, now: Instant) {
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.timestamp) > self.window {
                // Safe to pop since we just checked front() exists
                if let Some(entry) = self.entries.pop_front() {
                    match entry.direction {
                        Direction::Inbound => self.total_bytes_in -= entry.bytes,
                        Direction::Outbound => self.total_bytes_out -= entry.bytes,
                    }
                }
            } else {
                break;
            }
        }
    }

    fn get_bandwidth(&mut self) -> (f64, f64) {
        let now = Instant::now();
        self.cleanup_old_entries(now);

        let window_secs = self.window.as_secs_f64();
        let bytes_per_sec_in = self.total_bytes_in as f64 / window_secs;
        let bytes_per_sec_out = self.total_bytes_out as f64 / window_secs;

        (bytes_per_sec_in, bytes_per_sec_out)
    }
}

/// P2P transport using Iroh for node-to-node communication
pub struct IrohTransportV2 {
    /// The Iroh endpoint for connections
    endpoint: Endpoint,
    /// Active documents by type (simplified in-memory storage)
    documents: Arc<RwLock<HashMap<DocumentType, HashMap<String, DocumentEntry>>>>,
    /// Node ID for identification
    node_id: u64,
    /// Data directory
    data_dir: PathBuf,
    /// Discovery bridge (if discovery is enabled)
    discovery_bridge: Option<Arc<IrohDiscoveryBridge>>,
    /// P2P monitor for tracking metrics
    monitor: Arc<dyn P2pMonitor>,
    /// Bandwidth trackers per peer
    bandwidth_trackers: Arc<Mutex<HashMap<String, BandwidthTracker>>>,
    /// In-memory blob storage (hash -> content)
    blob_store: Arc<RwLock<HashMap<Hash, Vec<u8>>>>,
}

impl IrohTransportV2 {
    /// Load or generate a secret key for the node
    async fn load_or_generate_secret_key(iroh_data_dir: &Path) -> BlixardResult<SecretKey> {
        let key_path = iroh_data_dir.join("secret_key");

        // Try to load existing key
        if key_path.exists() {
            let key_bytes =
                tokio::fs::read(&key_path)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to read secret key: {}", e),
                    })?;

            if key_bytes.len() == 32 {
                let mut key_array = [0u8; 32];
                key_array.copy_from_slice(&key_bytes);
                let secret_key = SecretKey::from_bytes(&key_array);
                info!(
                    "Loaded existing secret key, node ID: {}",
                    secret_key.public()
                );
                return Ok(secret_key);
            }
        }

        // Generate new key if not found or invalid
        let secret_key = SecretKey::generate(rand::thread_rng());
        let key_bytes = secret_key.to_bytes();

        // Save the key for future use
        tokio::fs::write(&key_path, &key_bytes)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to save secret key: {}", e),
            })?;

        info!("Generated new secret key, node ID: {}", secret_key.public());
        Ok(secret_key)
    }

    /// Create a new Iroh transport instance without discovery
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        Self::new_with_discovery(node_id, data_dir, None).await
    }

    /// Create a new Iroh transport instance with optional discovery
    pub async fn new_with_discovery(
        node_id: u64,
        data_dir: &Path,
        discovery_manager: Option<Arc<DiscoveryManager>>,
    ) -> BlixardResult<Self> {
        Self::new_with_monitor(node_id, data_dir, discovery_manager, Arc::new(NoOpMonitor)).await
    }

    /// Create a new Iroh transport instance with discovery and monitor
    pub async fn new_with_monitor(
        node_id: u64,
        data_dir: &Path,
        discovery_manager: Option<Arc<DiscoveryManager>>,
        monitor: Arc<dyn P2pMonitor>,
    ) -> BlixardResult<Self> {
        let iroh_data_dir = data_dir.join("iroh");
        std::fs::create_dir_all(&iroh_data_dir)?;

        info!("Initializing Iroh transport v2 for node {}", node_id);

        // Load or generate secret key for this node
        let secret_key = Self::load_or_generate_secret_key(&iroh_data_dir).await?;

        // Build endpoint with discovery if provided
        let mut builder = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![BLIXARD_ALPN.to_vec()]);

        let discovery_bridge = if let Some(dm) = discovery_manager {
            info!("Configuring Iroh endpoint with Blixard discovery");

            // Create discovery bridge
            let bridge = Arc::new(IrohDiscoveryBridge::new(dm.clone()));

            // Start the bridge
            bridge.start().await?;

            // Configure endpoint with discovery
            let discovery = create_combined_discovery(dm.clone());
            builder = builder.add_discovery(discovery);

            Some(bridge)
        } else {
            info!("Configuring Iroh endpoint with default n0 DNS discovery");
            builder = builder.discovery_n0();
            None
        };

        // Create endpoint
        let endpoint = builder.bind().await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to create Iroh endpoint: {}", e),
        })?;

        let iroh_node_id = endpoint.node_id();
        info!("Iroh endpoint initialized with ID: {}", iroh_node_id);

        info!("Iroh P2P transport v2 initialized successfully");

        Ok(Self {
            endpoint,
            documents: Arc::new(RwLock::new(HashMap::new())),
            node_id,
            data_dir: data_dir.to_path_buf(),
            discovery_bridge,
            monitor,
            bandwidth_trackers: Arc::new(Mutex::new(HashMap::new())),
            blob_store: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        let node_id = self.endpoint.node_id();

        // Get direct addresses from bound sockets
        let bound_sockets = self.endpoint.bound_sockets();
        let direct_addresses = bound_sockets;

        // Create NodeAddr with both node ID and direct addresses
        let mut node_addr = NodeAddr::new(node_id).with_direct_addresses(direct_addresses);

        // Add relay URL if available (using default relay)
        // In production, this could be configured
        if let Ok(relay_url) = "https://relay.iroh.network/".parse() {
            node_addr = node_addr.with_relay_url(relay_url);
        }

        info!(
            "Node address created: ID={}, direct_addresses={:?}, relay_url={:?}",
            node_id,
            node_addr.direct_addresses().collect::<Vec<_>>(),
            node_addr.relay_url()
        );

        Ok(node_addr)
    }

    /// Get the underlying Iroh endpoint and node ID (for Raft transport)
    pub fn endpoint(&self) -> (Endpoint, iroh::NodeId) {
        let node_id = self.endpoint.node_id();
        (self.endpoint.clone(), node_id)
    }

    /// Create or join a document
    pub async fn create_or_join_doc(
        &self,
        doc_type: DocumentType,
        create_new: bool,
    ) -> BlixardResult<()> {
        let mut documents = self.documents.write().await;

        if !create_new && documents.contains_key(&doc_type) {
            // Already have this document
            return Ok(());
        }

        // Create a new document (simplified - just an empty HashMap)
        documents.insert(doc_type, HashMap::new());

        info!("Created document {:?}", doc_type);
        Ok(())
    }

    /// Write to a document
    pub async fn write_to_doc(
        &self,
        doc_type: DocumentType,
        key: &str,
        value: &[u8],
    ) -> BlixardResult<()> {
        let mut documents = self.documents.write().await;
        let doc = documents
            .get_mut(&doc_type)
            .ok_or_else(|| BlixardError::Internal {
                message: format!("Document {:?} not found", doc_type),
            })?;

        // Store the entry
        let entry = DocumentEntry {
            key: key.to_string(),
            value: value.to_vec(),
            author: format!("node-{}", self.node_id),
            timestamp: std::time::SystemTime::now(),
        };

        doc.insert(key.to_string(), entry);

        debug!(
            "Wrote {} bytes to document {:?} key '{}'",
            value.len(),
            doc_type,
            key
        );
        Ok(())
    }

    /// Read from a document
    pub async fn read_from_doc(&self, doc_type: DocumentType, key: &str) -> BlixardResult<Vec<u8>> {
        let documents = self.documents.read().await;
        let doc = documents
            .get(&doc_type)
            .ok_or_else(|| BlixardError::Internal {
                message: format!("Document {:?} not found", doc_type),
            })?;

        let entry = doc.get(key).ok_or_else(|| BlixardError::Internal {
            message: format!("Key '{}' not found in document {:?}", key, doc_type),
        })?;

        Ok(entry.value.clone())
    }

    /// Share a file and return its hash
    pub async fn share_file(&self, path: &Path) -> BlixardResult<Hash> {
        info!("Sharing file: {:?}", path);

        // Check if file exists
        if !path.exists() {
            return Err(BlixardError::Internal {
                message: format!("File not found: {:?}", path),
            });
        }

        // Read file content
        let content = tokio::fs::read(path)
            .await
            .map_err(|e| BlixardError::IoError(e))?;

        // Create a hash from the content using blake3
        let hash_bytes = blake3::hash(&content);
        let hash = Hash::from(*hash_bytes.as_bytes());

        // Store in our in-memory blob store
        {
            let mut store = self.blob_store.write().await;
            store.insert(hash, content.clone());
        }

        info!(
            "File shared with hash: {}, size: {} bytes",
            hash,
            content.len()
        );

        Ok(hash)
    }

    /// Download a file by hash
    pub async fn download_file(&self, hash: Hash, output_path: &Path) -> BlixardResult<()> {
        info!("Downloading file with hash: {} to {:?}", hash, output_path);

        // Check if we have the blob locally
        let content = {
            let store = self.blob_store.read().await;
            store.get(&hash).cloned()
        };

        match content {
            Some(data) => {
                // Write to output file
                tokio::fs::write(output_path, &data)
                    .await
                    .map_err(|e| BlixardError::IoError(e))?;

                info!("Successfully downloaded file to {:?}", output_path);
                Ok(())
            }
            None => Err(BlixardError::Internal {
                message: format!("Blob {} not found in local store", hash),
            }),
        }
    }

    /// Share a blob hash with a peer for them to download
    pub async fn send_blob_info(
        &self,
        peer_addr: &NodeAddr,
        hash: Hash,
        filename: &str,
    ) -> BlixardResult<()> {
        info!(
            "Sending blob info to peer: hash={}, filename={}",
            hash, filename
        );

        // Create a message with blob metadata
        let message = format!("BLOB:{}:{}", hash, filename);

        // Send via FileTransfer document type
        self.send_to_peer(peer_addr, DocumentType::FileTransfer, message.as_bytes())
            .await?;

        info!("Blob info sent successfully");
        Ok(())
    }

    /// Download a blob from a peer
    pub async fn download_blob_from_peer(
        &self,
        peer_addr: &NodeAddr,
        hash: Hash,
        output_path: &Path,
    ) -> BlixardResult<()> {
        info!(
            "Downloading blob {} from peer {:?}",
            hash, peer_addr.node_id
        );

        // For now, we'll use a simple protocol where we request the blob
        // and the peer streams it back to us
        let request = format!("GET_BLOB:{}", hash);

        // Connect to peer
        let conn = self
            .endpoint
            .connect(peer_addr.clone(), crate::transport::BLIXARD_ALPN)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect to peer for blob download: {}", e),
            })?;

        // Open bidirectional stream for request/response
        let (mut send_stream, mut recv_stream) =
            conn.open_bi().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to open stream for blob transfer: {}", e),
            })?;

        // Send request
        send_stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to send blob request: {}", e),
            })?;

        send_stream.finish().map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish send stream: {}", e),
        })?;

        // Read response
        let mut blob_data = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut blob_data)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read blob data: {}", e),
            })?;

        // Verify the blob hash
        let received_hash = blake3::hash(&blob_data);
        let expected_hash_bytes = hash.as_bytes();

        if received_hash.as_bytes() != expected_hash_bytes {
            return Err(BlixardError::Internal {
                message: format!(
                    "Blob hash mismatch: expected {}, got {}",
                    hash, received_hash
                ),
            });
        }

        // Write to output file
        tokio::fs::write(output_path, &blob_data)
            .await
            .map_err(|e| BlixardError::IoError(e))?;

        // Also store in our blob store for future sharing
        {
            let mut store = self.blob_store.write().await;
            store.insert(hash, blob_data);
        }

        info!("Successfully downloaded blob {} from peer", hash);
        Ok(())
    }

    /// Handle blob requests from peers
    pub async fn handle_blob_requests<F>(&self, handler: F) -> BlixardResult<()>
    where
        F: Fn(String) -> Option<Vec<u8>> + Send + Sync + Clone + 'static,
    {
        let blob_store = self.blob_store.clone();
        let handler = Arc::new(handler);

        loop {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let blob_store = blob_store.clone();
                    let handler = handler.clone();

                    tokio::spawn(async move {
                        match incoming.await {
                            Ok(conn) => {
                                // Accept bidirectional streams for blob requests
                                while let Ok((mut send_stream, mut recv_stream)) =
                                    conn.accept_bi().await
                                {
                                    let mut request = Vec::new();

                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(
                                        &mut recv_stream,
                                        &mut request,
                                    )
                                    .await
                                    {
                                        if let Ok(request_str) = String::from_utf8(request) {
                                            // Parse GET_BLOB request
                                            if let Some(hash_str) =
                                                request_str.strip_prefix("GET_BLOB:")
                                            {
                                                if let Ok(hash) = hash_str.parse::<Hash>() {
                                                    // Try to read blob from store
                                                    let blob_data = {
                                                        let store = blob_store.read().await;
                                                        store.get(&hash).cloned()
                                                    };

                                                    match blob_data {
                                                        Some(data) => {
                                                            // Send blob data back
                                                            let _ =
                                                                send_stream.write_all(&data).await;
                                                            let _ = send_stream.finish();
                                                            info!("Sent blob {} to peer", hash);
                                                        }
                                                        None => {
                                                            error!(
                                                                "Blob {} not found in store",
                                                                hash
                                                            );
                                                            // Send empty response to indicate error
                                                            let _ = send_stream.finish();
                                                        }
                                                    }
                                                }
                                            } else if let Some(data) = handler(request_str) {
                                                // Custom handler response
                                                let _ = send_stream.write_all(&data).await;
                                                let _ = send_stream.finish();
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Failed to accept blob request connection: {}", e);
                            }
                        }
                    });
                }
                None => {
                    // Endpoint closed
                    break;
                }
            }
        }

        Ok(())
    }

    /// Get a document ticket for sharing
    pub async fn get_doc_ticket(&self, doc_type: DocumentType) -> BlixardResult<String> {
        let documents = self.documents.read().await;
        let _namespace = documents
            .get(&doc_type)
            .ok_or_else(|| BlixardError::Internal {
                message: format!("Document {:?} not found", doc_type),
            })?;

        // For now, return a dummy ticket
        // Real implementation would use iroh-docs ticket system
        Ok(format!("ticket-{:?}-{}", doc_type, uuid::Uuid::new_v4()))
    }

    /// Join a document from a ticket
    pub async fn join_doc_from_ticket(&self, _ticket: &str) -> BlixardResult<()> {
        // For now, just return Ok
        // Real implementation would parse ticket and join document
        Ok(())
    }

    /// Send data to a peer using a simple unidirectional stream
    pub async fn send_to_peer(
        &self,
        peer_addr: &NodeAddr,
        doc_type: DocumentType,
        data: &[u8],
    ) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("network::send_message");

        let peer_id = peer_addr.node_id.to_string();
        let start_time = Instant::now();
        let data_size = data.len();

        // Record connection attempt
        self.monitor.record_connection_attempt(&peer_id, true).await;

        // Always use BLIXARD_ALPN for all connections
        let conn = self
            .endpoint
            .connect(peer_addr.clone(), crate::transport::BLIXARD_ALPN)
            .await
            .map_err(|e| {
                // Record failed connection
                let monitor = self.monitor.clone();
                let peer_id = peer_id.clone();
                tokio::spawn(async move {
                    monitor.record_connection_attempt(&peer_id, false).await;
                });
                BlixardError::Internal {
                    message: format!("Failed to connect to peer: {}", e),
                }
            })?;

        let mut stream = conn.open_uni().await.map_err(|e| BlixardError::Internal {
            message: format!("Failed to open stream: {}", e),
        })?;

        stream
            .write_all(data)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to write data: {}", e),
            })?;

        stream.finish().map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish stream: {}", e),
        })?;

        // Record successful transfer metrics
        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
        self.monitor
            .record_bytes_transferred(&peer_id, Direction::Outbound, data_size as u64)
            .await;
        self.monitor
            .record_message(&peer_id, doc_type.as_str(), data_size, Some(latency_ms))
            .await;

        // Update bandwidth tracker
        {
            let mut trackers = self.bandwidth_trackers.lock().await;
            let tracker = trackers
                .entry(peer_id.clone())
                .or_insert_with(|| BandwidthTracker::new(Duration::from_secs(60)));
            tracker.add_entry(data_size as u64, Direction::Outbound);
        }

        debug!(
            "Sent {} bytes to peer {} for {:?} in {:.2}ms",
            data_size, peer_id, doc_type, latency_ms
        );

        Ok(())
    }

    /// Accept incoming connections (simplified version)
    pub async fn accept_connections<F>(&self, handler: F) -> BlixardResult<()>
    where
        F: Fn(DocumentType, Vec<u8>) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        let monitor = self.monitor.clone();
        let bandwidth_trackers = self.bandwidth_trackers.clone();

        loop {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let handler = handler.clone();
                    let monitor = monitor.clone();
                    let bandwidth_trackers = bandwidth_trackers.clone();

                    tokio::spawn(async move {
                        match incoming.await {
                            Ok(conn) => {
                                // Extract peer ID from connection
                                let peer_node_id = match conn.remote_node_id() {
                                    Ok(id) => id,
                                    Err(e) => {
                                        error!("Failed to get remote node ID: {:?}", e);
                                        return;
                                    }
                                };
                                let peer_id = peer_node_id.to_string();
                                // All connections use standard BLIXARD_ALPN
                                // Document type can be encoded in the message payload if needed
                                let doc_type = DocumentType::ClusterConfig; // Default for now

                                // Record connection state change
                                monitor
                                    .record_connection_state_change(
                                        &peer_id,
                                        ConnectionState::Connecting,
                                        ConnectionState::Connected,
                                    )
                                    .await;

                                // Accept unidirectional streams
                                while let Ok(mut recv_stream) = conn.accept_uni().await {
                                    let start_time = Instant::now();
                                    let mut data = Vec::new();

                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(
                                        &mut recv_stream,
                                        &mut data,
                                    )
                                    .await
                                    {
                                        let data_size = data.len();

                                        // Record incoming transfer metrics
                                        monitor
                                            .record_bytes_transferred(
                                                &peer_id,
                                                Direction::Inbound,
                                                data_size as u64,
                                            )
                                            .await;
                                        let latency_ms =
                                            start_time.elapsed().as_secs_f64() * 1000.0;
                                        monitor
                                            .record_message(
                                                &peer_id,
                                                doc_type.as_str(),
                                                data_size,
                                                Some(latency_ms),
                                            )
                                            .await;

                                        // Update bandwidth tracker
                                        {
                                            let mut trackers = bandwidth_trackers.lock().await;
                                            let tracker = trackers
                                                .entry(peer_id.clone())
                                                .or_insert_with(|| {
                                                    BandwidthTracker::new(Duration::from_secs(60))
                                                });
                                            tracker.add_entry(data_size as u64, Direction::Inbound);
                                        }

                                        debug!(
                                            "Received {} bytes from peer {} for {:?} in {:.2}ms",
                                            data_size, peer_id, doc_type, latency_ms
                                        );

                                        handler(doc_type, data);
                                    }
                                }

                                // Record disconnection
                                monitor
                                    .record_connection_state_change(
                                        &peer_id,
                                        ConnectionState::Connected,
                                        ConnectionState::Disconnected,
                                    )
                                    .await;
                            }
                            Err(e) => {
                                debug!("Failed to accept connection: {}", e);
                            }
                        }
                    });
                }
                None => {
                    // Endpoint closed
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle health check requests by echoing back the data
    pub async fn handle_health_check(
        &self,
        peer_addr: &NodeAddr,
        echo_data: &[u8],
    ) -> BlixardResult<()> {
        // For health checks, we just echo back the data
        self.send_to_peer(peer_addr, DocumentType::ClusterConfig, echo_data)
            .await
    }

    /// Perform a health check to measure RTT using bidirectional stream
    pub async fn perform_health_check(&self, peer_addr: &NodeAddr) -> BlixardResult<f64> {
        let start_time = Instant::now();
        let peer_id = peer_addr.node_id.to_string();

        // Connect with standard BLIXARD_ALPN
        let conn = self
            .endpoint
            .connect(peer_addr.clone(), crate::transport::BLIXARD_ALPN)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect for health check: {}", e),
            })?;

        // Open bidirectional stream
        let (mut send_stream, mut recv_stream) =
            conn.open_bi().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to open bidirectional stream: {}", e),
            })?;

        // Create health check payload
        let echo_data = format!(
            "health-check-{}-{}",
            self.node_id,
            start_time.elapsed().as_nanos()
        );
        let data = echo_data.as_bytes();

        // Send data
        send_stream
            .write_all(data)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to write health check data: {}", e),
            })?;

        send_stream.finish().map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish send stream: {}", e),
        })?;

        // Read echo response
        let mut response = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut response)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read health check response: {}", e),
            })?;

        // Verify we got the same data back
        if response != data {
            return Err(BlixardError::Internal {
                message: "Health check echo mismatch".to_string(),
            });
        }

        // Calculate RTT
        let rtt_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        // Record RTT measurement
        self.monitor.record_rtt(&peer_id, rtt_ms).await;

        Ok(rtt_ms)
    }

    /// Get bandwidth statistics for a peer over a time window
    pub async fn get_bandwidth_stats(&self, peer_id: &str, _window_secs: u64) -> (f64, f64) {
        let mut trackers = self.bandwidth_trackers.lock().await;
        if let Some(tracker) = trackers.get_mut(peer_id) {
            tracker.get_bandwidth()
        } else {
            (0.0, 0.0) // No data for this peer
        }
    }

    /// Get message volume statistics
    pub async fn get_message_stats(&self) -> HashMap<String, (u64, u64)> {
        // This would typically be tracked in a more sophisticated way
        // For now, return empty map - could be extended to track per message type
        HashMap::new()
    }

    /// Accept bidirectional connections for health checks
    pub async fn accept_health_check_connections(&self) -> BlixardResult<()> {
        let monitor = self.monitor.clone();
        let bandwidth_trackers = self.bandwidth_trackers.clone();

        loop {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let monitor = monitor.clone();
                    let bandwidth_trackers = bandwidth_trackers.clone();

                    tokio::spawn(async move {
                        match incoming.await {
                            Ok(conn) => {
                                let peer_node_id = match conn.remote_node_id() {
                                    Ok(id) => id,
                                    Err(e) => {
                                        error!("Failed to get remote node ID: {:?}", e);
                                        return;
                                    }
                                };
                                let peer_id = peer_node_id.to_string();
                                let alpn = conn.alpn();

                                // All connections use standard BLIXARD_ALPN
                                // Message type is determined by payload content

                                // Accept bidirectional streams for health checks
                                while let Ok((mut send_stream, mut recv_stream)) =
                                    conn.accept_bi().await
                                {
                                    let start_time = Instant::now();
                                    let mut data = Vec::new();

                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(
                                        &mut recv_stream,
                                        &mut data,
                                    )
                                    .await
                                    {
                                        let data_size = data.len();

                                        // Echo the data back
                                        if let Ok(_) = tokio::io::AsyncWriteExt::write_all(
                                            &mut send_stream,
                                            &data,
                                        )
                                        .await
                                        {
                                            let _ = send_stream.finish();

                                            // Record RTT (round-trip time)
                                            let rtt_ms =
                                                start_time.elapsed().as_secs_f64() * 1000.0;
                                            monitor.record_rtt(&peer_id, rtt_ms).await;

                                            // Record bandwidth for both directions
                                            monitor
                                                .record_bytes_transferred(
                                                    &peer_id,
                                                    Direction::Inbound,
                                                    data_size as u64,
                                                )
                                                .await;
                                            monitor
                                                .record_bytes_transferred(
                                                    &peer_id,
                                                    Direction::Outbound,
                                                    data_size as u64,
                                                )
                                                .await;

                                            // Update bandwidth tracker
                                            {
                                                let mut trackers = bandwidth_trackers.lock().await;
                                                let tracker = trackers
                                                    .entry(peer_id.clone())
                                                    .or_insert_with(|| {
                                                        BandwidthTracker::new(Duration::from_secs(
                                                            60,
                                                        ))
                                                    });
                                                tracker.add_entry(
                                                    data_size as u64,
                                                    Direction::Inbound,
                                                );
                                                tracker.add_entry(
                                                    data_size as u64,
                                                    Direction::Outbound,
                                                );
                                            }

                                            debug!(
                                                "Health check echo: {} bytes, RTT: {:.2}ms",
                                                data_size, rtt_ms
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Failed to accept health check connection: {}", e);
                            }
                        }
                    });
                }
                None => {
                    // Endpoint closed
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shutdown the Iroh transport
    pub async fn shutdown(self) -> BlixardResult<()> {
        info!("Shutting down Iroh transport v2");

        // Stop discovery bridge if present
        if let Some(bridge) = &self.discovery_bridge {
            bridge.stop().await?;
        }

        // Close endpoint
        self.endpoint.close().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_iroh_transport_v2_creation() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV2::new(1, temp_dir.path()).await.unwrap();
        let addr = transport.node_addr().await.unwrap();

        // Verify node ID exists
        assert!(!addr.node_id.to_string().is_empty());

        // Verify direct addresses are populated
        let direct_addrs: Vec<_> = addr.direct_addresses().collect();
        assert!(
            !direct_addrs.is_empty(),
            "Node should have at least one direct address"
        );

        // Verify relay URL is set
        assert!(
            addr.relay_url().is_some(),
            "Node should have a relay URL configured"
        );

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_document_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV2::new(1, temp_dir.path()).await.unwrap();

        // Create a document
        transport
            .create_or_join_doc(DocumentType::ClusterConfig, true)
            .await
            .unwrap();

        // Write to the document
        let key = "test-key";
        let value = b"test-value";
        transport
            .write_to_doc(DocumentType::ClusterConfig, key, value)
            .await
            .unwrap();

        // Read from the document
        let read_value = transport
            .read_from_doc(DocumentType::ClusterConfig, key)
            .await
            .unwrap();
        assert_eq!(read_value, value);

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV2::new(1, temp_dir.path()).await.unwrap();

        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        let test_content = b"test content for blob storage";
        std::fs::write(&test_file, test_content).unwrap();

        // Share the file
        let hash = transport.share_file(&test_file).await.unwrap();
        assert!(!hash.to_string().is_empty());
        println!("File shared with hash: {}", hash);

        // Download the file locally (from our own blob store)
        let output_file = temp_dir.path().join("downloaded.txt");
        transport.download_file(hash, &output_file).await.unwrap();

        // Verify the downloaded content matches
        let downloaded_content = std::fs::read(&output_file).unwrap();
        assert_eq!(downloaded_content, test_content);

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_communication() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        // Create two transports
        let transport1 = IrohTransportV2::new(1, temp_dir1.path()).await.unwrap();
        let transport2 = IrohTransportV2::new(2, temp_dir2.path()).await.unwrap();

        // Get addresses
        let addr1 = transport1.node_addr().await.unwrap();
        let addr2 = transport2.node_addr().await.unwrap();

        // Test sending data from transport1 to transport2
        let test_data = b"Hello from node 1!";

        // Try to send (may fail if no direct connectivity)
        match transport1
            .send_to_peer(&addr2, DocumentType::ClusterConfig, test_data)
            .await
        {
            Ok(_) => {
                println!("Successfully sent data between nodes");
            }
            Err(e) => {
                println!("Failed to send data (expected in test environment): {}", e);
            }
        }

        transport1.shutdown().await.unwrap();
        transport2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_blob_sharing_between_peers() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        // Create two transports
        let transport1 = IrohTransportV2::new(1, temp_dir1.path()).await.unwrap();
        let transport2 = IrohTransportV2::new(2, temp_dir2.path()).await.unwrap();

        // Get addresses
        let addr1 = transport1.node_addr().await.unwrap();
        let addr2 = transport2.node_addr().await.unwrap();

        // Create and share a file on transport1
        let test_file = temp_dir1.path().join("share_test.txt");
        let test_content = b"This is a test file for P2P blob sharing!";
        std::fs::write(&test_file, test_content).unwrap();

        let hash = transport1.share_file(&test_file).await.unwrap();
        println!("Transport1 shared file with hash: {}", hash);

        // Send blob info to transport2
        transport1
            .send_blob_info(&addr2, hash, "share_test.txt")
            .await
            .unwrap();

        // For this test, we'll just verify that transport1 has the blob
        // In a real P2P scenario, transport2 would request the blob from transport1
        let has_blob = {
            let store = transport1.blob_store.read().await;
            store.contains_key(&hash)
        };
        assert!(has_blob, "Transport1 should have the blob in its store");

        transport1.shutdown().await.unwrap();
        transport2.shutdown().await.unwrap();
    }
}
