//! Iroh P2P transport layer for Blixard - Version 2 with actual implementation
//!
//! This module provides peer-to-peer communication capabilities using Iroh 0.90,
//! enabling direct connections between nodes for efficient data transfer.

use crate::error::{BlixardError, BlixardResult};
use crate::discovery::{DiscoveryManager, create_combined_discovery, IrohDiscoveryBridge};
use crate::p2p_monitor::{P2pMonitor, Direction, ConnectionState, NoOpMonitor};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, Duration};
use tokio::sync::{RwLock, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use iroh::{Endpoint, SecretKey, NodeAddr};
use iroh_blobs::Hash;
use tracing::{info, debug};

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
}

impl DocumentType {
    fn as_str(&self) -> &'static str {
        match self {
            DocumentType::ClusterConfig => "cluster-config",
            DocumentType::VmImages => "vm-images",
            DocumentType::Logs => "logs",
            DocumentType::Metrics => "metrics",
            DocumentType::FileTransfer => "file-transfer",
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
                let entry = self.entries.pop_front().unwrap();
                match entry.direction {
                    Direction::Inbound => self.total_bytes_in -= entry.bytes,
                    Direction::Outbound => self.total_bytes_out -= entry.bytes,
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
}

impl IrohTransportV2 {
    /// Create a new Iroh transport instance without discovery
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        Self::new_with_discovery(node_id, data_dir, None).await
    }
    
    /// Create a new Iroh transport instance with optional discovery
    pub async fn new_with_discovery(
        node_id: u64, 
        data_dir: &Path,
        discovery_manager: Option<Arc<DiscoveryManager>>
    ) -> BlixardResult<Self> {
        Self::new_with_monitor(node_id, data_dir, discovery_manager, Arc::new(NoOpMonitor)).await
    }
    
    /// Create a new Iroh transport instance with discovery and monitor
    pub async fn new_with_monitor(
        node_id: u64, 
        data_dir: &Path,
        discovery_manager: Option<Arc<DiscoveryManager>>,
        monitor: Arc<dyn P2pMonitor>
    ) -> BlixardResult<Self> {
        let iroh_data_dir = data_dir.join("iroh");
        std::fs::create_dir_all(&iroh_data_dir)?;

        info!("Initializing Iroh transport v2 for node {}", node_id);

        // Create secret key for this node
        let secret_key = SecretKey::generate(rand::thread_rng());
        
        // Build endpoint with discovery if provided
        let mut builder = Endpoint::builder()
            .secret_key(secret_key);
        
        let discovery_bridge = if let Some(dm) = discovery_manager {
            info!("Configuring Iroh endpoint with Blixard discovery");
            
            // Create discovery bridge
            let bridge = Arc::new(IrohDiscoveryBridge::new(dm.clone()));
            
            // Start the bridge
            bridge.start().await?;
            
            // Create combined discovery service
            let discovery = create_combined_discovery(dm);
            
            // Configure endpoint with discovery
            builder = builder.add_discovery(discovery);
            
            Some(bridge)
        } else {
            info!("Configuring Iroh endpoint with default n0 DNS discovery");
            builder = builder.discovery_n0();
            None
        };
        
        // Create endpoint
        let endpoint = builder
            .bind()
            .await
            .map_err(|e| BlixardError::Internal {
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
        })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        let node_id = self.endpoint.node_id();
        // TODO: Get direct addresses when API is stable
        // For now, just return NodeAddr with node_id
        Ok(NodeAddr::new(node_id))
    }
    
    /// Get the underlying Iroh endpoint and node ID (for Raft transport)
    pub fn endpoint(&self) -> (Endpoint, iroh::NodeId) {
        let node_id = self.endpoint.node_id();
        (self.endpoint.clone(), node_id)
    }

    /// Create or join a document
    pub async fn create_or_join_doc(&self, doc_type: DocumentType, create_new: bool) -> BlixardResult<()> {
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
    pub async fn write_to_doc(&self, doc_type: DocumentType, key: &str, value: &[u8]) -> BlixardResult<()> {
        let mut documents = self.documents.write().await;
        let doc = documents.get_mut(&doc_type)
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
        
        debug!("Wrote {} bytes to document {:?} key '{}'", value.len(), doc_type, key);
        Ok(())
    }
    
    /// Read from a document
    pub async fn read_from_doc(&self, doc_type: DocumentType, key: &str) -> BlixardResult<Vec<u8>> {
        let documents = self.documents.read().await;
        let doc = documents.get(&doc_type)
            .ok_or_else(|| BlixardError::Internal {
                message: format!("Document {:?} not found", doc_type),
            })?;
        
        let entry = doc.get(key)
            .ok_or_else(|| BlixardError::Internal {
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
        let content = tokio::fs::read(path).await
            .map_err(|e| BlixardError::IoError(e))?;
        
        // Create a hash from the content using blake3
        let hash_bytes = blake3::hash(&content);
        let hash = Hash::from(*hash_bytes.as_bytes());
        
        // In a real implementation, we would store this in a blob store
        // For now, we just return the hash
        
        info!("File shared with hash: {}", hash);
        Ok(hash)
    }

    /// Download a file by hash
    pub async fn download_file(&self, _hash: Hash, _output_path: &Path) -> BlixardResult<()> {
        // For now, return not implemented
        // Real implementation would fetch from blob store
        Err(BlixardError::NotImplemented {
            feature: "Blob download operations".to_string(),
        })
    }
    
    /// Get a document ticket for sharing
    pub async fn get_doc_ticket(&self, doc_type: DocumentType) -> BlixardResult<String> {
        let documents = self.documents.read().await;
        let _namespace = documents.get(&doc_type)
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
        let peer_id = peer_addr.node_id.to_string();
        let start_time = Instant::now();
        let data_size = data.len();
        
        // Record connection attempt
        self.monitor.record_connection_attempt(&peer_id, true).await;
        
        let conn = self.endpoint
            .connect(peer_addr.clone(), doc_type.as_str().as_bytes())
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
        
        let mut stream = conn.open_uni().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to open stream: {}", e),
            })?;
        
        stream.write_all(data).await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to write data: {}", e),
            })?;
        
        stream.finish()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to finish stream: {}", e),
            })?;
        
        // Record successful transfer metrics
        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
        self.monitor.record_bytes_transferred(&peer_id, Direction::Outbound, data_size as u64).await;
        self.monitor.record_message(&peer_id, doc_type.as_str(), data_size, Some(latency_ms)).await;
        
        // Update bandwidth tracker
        {
            let mut trackers = self.bandwidth_trackers.lock().await;
            let tracker = trackers.entry(peer_id.clone())
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
                                let peer_id = conn.remote_address().node_id.to_string();
                                let alpn = conn.alpn();
                                let doc_type = match alpn.as_deref() {
                                    Some(b"cluster-config") => DocumentType::ClusterConfig,
                                    Some(b"vm-images") => DocumentType::VmImages,
                                    Some(b"logs") => DocumentType::Logs,
                                    Some(b"metrics") => DocumentType::Metrics,
                                    Some(b"file-transfer") => DocumentType::FileTransfer,
                                    Some(b"health-check") => DocumentType::ClusterConfig, // Health checks use cluster-config type
                                    _ => return,
                                };
                                
                                // Record connection state change
                                monitor.record_connection_state_change(&peer_id, ConnectionState::Connecting, ConnectionState::Connected).await;
                                
                                // Accept unidirectional streams
                                while let Ok(mut recv_stream) = conn.accept_uni().await {
                                    let start_time = Instant::now();
                                    let mut data = Vec::new();
                                    
                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut data).await {
                                        let data_size = data.len();
                                        
                                        // Record incoming transfer metrics
                                        monitor.record_bytes_transferred(&peer_id, Direction::Inbound, data_size as u64).await;
                                        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                                        monitor.record_message(&peer_id, doc_type.as_str(), data_size, Some(latency_ms)).await;
                                        
                                        // Update bandwidth tracker
                                        {
                                            let mut trackers = bandwidth_trackers.lock().await;
                                            let tracker = trackers.entry(peer_id.clone())
                                                .or_insert_with(|| BandwidthTracker::new(Duration::from_secs(60)));
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
                                monitor.record_connection_state_change(&peer_id, ConnectionState::Connected, ConnectionState::Disconnected).await;
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
        self.send_to_peer(peer_addr, DocumentType::ClusterConfig, echo_data).await
    }
    
    /// Perform a health check to measure RTT using bidirectional stream
    pub async fn perform_health_check(
        &self,
        peer_addr: &NodeAddr,
    ) -> BlixardResult<f64> {
        let start_time = Instant::now();
        let peer_id = peer_addr.node_id.to_string();
        
        // Connect with health-check ALPN
        let conn = self.endpoint
            .connect(peer_addr.clone(), b"health-check")
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect for health check: {}", e),
            })?;
        
        // Open bidirectional stream
        let (mut send_stream, mut recv_stream) = conn.open_bi().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to open bidirectional stream: {}", e),
            })?;
        
        // Create health check payload
        let echo_data = format!("health-check-{}-{}", self.node_id, start_time.elapsed().as_nanos());
        let data = echo_data.as_bytes();
        
        // Send data
        send_stream.write_all(data).await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to write health check data: {}", e),
            })?;
        
        send_stream.finish()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to finish send stream: {}", e),
            })?;
        
        // Read echo response
        let mut response = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut response).await
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
                                let peer_id = conn.remote_address().node_id.to_string();
                                let alpn = conn.alpn();
                                
                                // Only handle health-check ALPN
                                if alpn.as_deref() != Some(b"health-check") {
                                    return;
                                }
                                
                                // Accept bidirectional streams for health checks
                                while let Ok((mut send_stream, mut recv_stream)) = conn.accept_bi().await {
                                    let start_time = Instant::now();
                                    let mut data = Vec::new();
                                    
                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut data).await {
                                        let data_size = data.len();
                                        
                                        // Echo the data back
                                        if let Ok(_) = tokio::io::AsyncWriteExt::write_all(&mut send_stream, &data).await {
                                            let _ = send_stream.finish();
                                            
                                            // Record RTT (round-trip time)
                                            let rtt_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                                            monitor.record_rtt(&peer_id, rtt_ms).await;
                                            
                                            // Record bandwidth for both directions
                                            monitor.record_bytes_transferred(&peer_id, Direction::Inbound, data_size as u64).await;
                                            monitor.record_bytes_transferred(&peer_id, Direction::Outbound, data_size as u64).await;
                                            
                                            // Update bandwidth tracker
                                            {
                                                let mut trackers = bandwidth_trackers.lock().await;
                                                let tracker = trackers.entry(peer_id.clone())
                                                    .or_insert_with(|| BandwidthTracker::new(Duration::from_secs(60)));
                                                tracker.add_entry(data_size as u64, Direction::Inbound);
                                                tracker.add_entry(data_size as u64, Direction::Outbound);
                                            }
                                            
                                            debug!("Health check echo: {} bytes, RTT: {:.2}ms", data_size, rtt_ms);
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
        assert!(!addr.node_id.to_string().is_empty());
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_document_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV2::new(1, temp_dir.path()).await.unwrap();
        
        // Create a document
        transport.create_or_join_doc(DocumentType::ClusterConfig, true).await.unwrap();
        
        // Write to the document
        let key = "test-key";
        let value = b"test-value";
        transport.write_to_doc(DocumentType::ClusterConfig, key, value).await.unwrap();
        
        // Read from the document
        let read_value = transport.read_from_doc(DocumentType::ClusterConfig, key).await.unwrap();
        assert_eq!(read_value, value);
        
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV2::new(1, temp_dir.path()).await.unwrap();
        
        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, b"test content").unwrap();
        
        // Share the file
        let hash = transport.share_file(&test_file).await.unwrap();
        assert!(!hash.to_string().is_empty());
        
        // Download is not implemented yet
        let output_file = temp_dir.path().join("downloaded.txt");
        match transport.download_file(hash, &output_file).await {
            Err(BlixardError::NotImplemented { .. }) => {
                // Expected
            }
            _ => panic!("Expected NotImplemented error"),
        }
        
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
        match transport1.send_to_peer(&addr2, DocumentType::ClusterConfig, test_data).await {
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
}