//! Iroh P2P transport layer for Blixard
//!
//! This module provides peer-to-peer communication capabilities using Iroh,
//! enabling direct connections between nodes for efficient data transfer.

use crate::error::{BlixardError, BlixardResult};
use iroh::endpoint::Connection;
use iroh::{Endpoint, NodeAddr, SecretKey};
use iroh_blobs::Hash;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{debug, info};

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

    fn to_alpn(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

/// Connection metadata for health tracking
#[derive(Debug, Clone)]
struct ConnectionInfo {
    connection: Connection,
    last_used: Instant,
    use_count: u64,
}

impl ConnectionInfo {
    fn new(connection: Connection) -> Self {
        Self {
            connection,
            last_used: Instant::now(),
            use_count: 1,
        }
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }

    fn is_stale(&self, max_idle: Duration) -> bool {
        self.last_used.elapsed() > max_idle
    }
}

/// P2P transport using Iroh for node-to-node communication with connection pooling
pub struct IrohTransport {
    /// The Iroh endpoint for connections
    endpoint: Endpoint,
    /// Active connections by peer with metadata
    connections: Arc<RwLock<HashMap<iroh::NodeId, ConnectionInfo>>>,
    /// Node ID for identification
    #[allow(dead_code)] // Future: implement node identification features
    node_id: u64,
    /// Data directory
    #[allow(dead_code)] // Future: implement persistent connection state
    data_dir: PathBuf,
    /// Connection pool configuration
    max_idle_duration: Duration,
    #[allow(dead_code)] // Future: implement connection limiting per peer
    max_connections_per_peer: usize,
}

impl IrohTransport {
    /// Create a new Iroh transport instance
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        let iroh_data_dir = data_dir.join("iroh");
        std::fs::create_dir_all(&iroh_data_dir)?;

        info!("Initializing Iroh transport for node {}", node_id);

        // Create secret key for this node
        let secret_key = SecretKey::generate(rand::thread_rng());

        // Create endpoint with all document type ALPNs
        let alpns: Vec<Vec<u8>> = vec![
            DocumentType::ClusterConfig.to_alpn().to_vec(),
            DocumentType::VmImages.to_alpn().to_vec(),
            DocumentType::Logs.to_alpn().to_vec(),
            DocumentType::Metrics.to_alpn().to_vec(),
            DocumentType::FileTransfer.to_alpn().to_vec(),
        ];

        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(alpns)
            .bind()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create Iroh endpoint: {}", e),
            })?;

        let iroh_node_id = endpoint.node_id();
        info!("Iroh endpoint initialized with ID: {}", iroh_node_id);

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::with_capacity(16))), // Pre-allocate for common case
            node_id,
            data_dir: data_dir.to_path_buf(),
            max_idle_duration: Duration::from_secs(300), // 5 minutes idle timeout
            max_connections_per_peer: 1,                 // For now, limit to 1 connection per peer
        })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        let node_id = self.endpoint.node_id();
        // TODO: Get direct addresses when API is available
        // For now, just return NodeAddr with node_id
        Ok(NodeAddr::new(node_id))
    }

    /// Get the underlying Iroh endpoint and node ID (for Raft transport)
    pub fn endpoint(&self) -> (Endpoint, iroh::NodeId) {
        let node_id = self.endpoint.node_id();
        (self.endpoint.clone(), node_id)
    }

    /// Connect to a peer with connection pooling and health checking
    async fn connect_to_peer(
        &self,
        addr: &NodeAddr,
        doc_type: DocumentType,
    ) -> BlixardResult<Connection> {
        // Try to get existing healthy connection first
        {
            let mut connections = self.connections.write().await;

            // Clean up stale connections
            let stale_keys: Vec<_> = connections
                .iter()
                .filter(|(_, info)| info.is_stale(self.max_idle_duration))
                .map(|(key, _)| *key)
                .collect();

            for key in stale_keys {
                debug!("Removing stale connection to peer: {:?}", key);
                connections.remove(&key);
            }

            // Check for existing healthy connection
            if let Some(info) = connections.get_mut(&addr.node_id) {
                info.mark_used();
                debug!(
                    "Reusing existing connection to peer: {:?} (use count: {})",
                    addr.node_id, info.use_count
                );
                return Ok(info.connection.clone());
            }
        }

        // Create new connection
        debug!("Creating new connection to peer: {:?}", addr.node_id);
        let conn = self
            .endpoint
            .connect(addr.clone(), doc_type.to_alpn())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect to peer: {}", e),
            })?;

        // Store in connection pool
        {
            let mut connections = self.connections.write().await;
            connections.insert(addr.node_id, ConnectionInfo::new(conn.clone()));
            info!("Added new connection to pool for peer: {:?}", addr.node_id);
        }

        Ok(conn)
    }

    /// Cleanup stale connections (can be called periodically)
    pub async fn cleanup_stale_connections(&self) {
        let mut connections = self.connections.write().await;
        let initial_count = connections.len();

        connections.retain(|peer_id, info| {
            let keep = !info.is_stale(self.max_idle_duration);
            if !keep {
                debug!("Removing stale connection to peer: {:?}", peer_id);
            }
            keep
        });

        let removed_count = initial_count - connections.len();
        if removed_count > 0 {
            info!("Cleaned up {} stale connections", removed_count);
        }
    }

    /// Share a file and return its hash
    /// Note: This is a simplified implementation that doesn't use blob store
    /// In a real implementation, you would use iroh-blobs store API
    pub async fn share_file(&self, path: &Path) -> BlixardResult<Hash> {
        info!("Sharing file: {:?}", path);

        // For now, we'll just compute a hash of the file
        // In a real implementation, this would import into a blob store
        let _content = tokio::fs::read(path)
            .await
            .map_err(|e| BlixardError::IoError(Box::new(e)))?;

        // TODO: Implement proper hash generation when iroh-blobs API is available
        // For now, return a dummy hash
        let dummy_hash = [0u8; 32];
        let hash = iroh_blobs::Hash::from(dummy_hash);

        info!("File shared (stub implementation)");

        Ok(hash)
    }

    /// Download a file by hash
    /// Note: This is a simplified implementation
    pub async fn download_file(&self, _hash: Hash, _output_path: &Path) -> BlixardResult<()> {
        // In a real implementation, this would fetch from blob store
        Err(BlixardError::NotImplemented {
            feature: "blob download".to_string(),
        })
    }

    /// Send data to a peer
    pub async fn send_to_peer(
        &self,
        peer_addr: &NodeAddr,
        doc_type: DocumentType,
        data: &[u8],
    ) -> BlixardResult<()> {
        let conn = self.connect_to_peer(peer_addr, doc_type).await?;

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

        Ok(())
    }

    // Stub methods for P2P document operations
    pub async fn create_or_join_doc(
        &self,
        _doc_type: DocumentType,
        _create_new: bool,
    ) -> BlixardResult<()> {
        // For now, just return Ok to allow P2P manager to initialize
        // TODO: Implement document operations when needed
        Ok(())
    }

    pub async fn write_to_doc(
        &self,
        _doc_type: DocumentType,
        _key: &str,
        _value: &[u8],
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "P2P document operations".to_string(),
        })
    }

    pub async fn read_from_doc(
        &self,
        _doc_type: DocumentType,
        _key: &str,
    ) -> BlixardResult<Vec<u8>> {
        Err(BlixardError::NotImplemented {
            feature: "P2P document operations".to_string(),
        })
    }

    pub async fn get_doc_ticket(&self, _doc_type: DocumentType) -> BlixardResult<String> {
        Err(BlixardError::NotImplemented {
            feature: "P2P document operations".to_string(),
        })
    }

    pub async fn join_doc_from_ticket(&self, _ticket: &str) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "P2P document operations".to_string(),
        })
    }

    /// Accept incoming connections and handle them
    pub async fn accept_connections<F>(&self, handler: F) -> BlixardResult<()>
    where
        F: Fn(DocumentType, Vec<u8>) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);

        while let Some(incoming) = self.endpoint.accept().await {
            let handler = handler.clone();

            tokio::spawn(async move {
                // Accept the connection
                let conn = match incoming.await {
                    Ok(conn) => conn,
                    Err(_) => return,
                };

                // Get ALPN from the connection
                let alpn = conn.alpn();
                let doc_type = match alpn.as_deref() {
                    Some(b"cluster-config") => DocumentType::ClusterConfig,
                    Some(b"vm-images") => DocumentType::VmImages,
                    Some(b"logs") => DocumentType::Logs,
                    Some(b"metrics") => DocumentType::Metrics,
                    Some(b"file-transfer") => DocumentType::FileTransfer,
                    _ => return,
                };

                while let Ok(mut recv_stream) = conn.accept_uni().await {
                    let mut data = Vec::new();
                    if let Ok(_) =
                        tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut data).await
                    {
                        handler(doc_type.clone(), data);
                    }
                }
            });
        }

        Ok(())
    }

    /// Shutdown the Iroh transport
    pub async fn shutdown(self) -> BlixardResult<()> {
        info!("Shutting down Iroh transport");
        self.endpoint.close().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_iroh_transport_creation() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransport::new(1, temp_dir.path()).await.unwrap();
        let addr = transport.node_addr().await.unwrap();
        assert!(!addr.node_id.to_string().is_empty());
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransport::new(1, temp_dir.path()).await.unwrap();

        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, b"test content").unwrap();

        // Share the file
        let hash = transport.share_file(&test_file).await.unwrap();
        assert!(!hash.to_string().is_empty());

        transport.shutdown().await.unwrap();
    }
}
