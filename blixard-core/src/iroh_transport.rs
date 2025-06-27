//! Iroh P2P transport layer for Blixard
//!
//! This module provides peer-to-peer communication capabilities using Iroh,
//! enabling direct connections between nodes for efficient data transfer.

use crate::error::{BlixardError, BlixardResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use iroh::{Endpoint, SecretKey, NodeAddr};
use iroh::endpoint::Connection;
use iroh_blobs::Hash;
use tracing::info;

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

/// P2P transport using Iroh for node-to-node communication
pub struct IrohTransport {
    /// The Iroh endpoint for connections
    endpoint: Endpoint,
    /// Active connections by peer
    connections: Arc<RwLock<HashMap<iroh::NodeId, Connection>>>,
    /// Node ID for identification
    node_id: u64,
    /// Data directory
    data_dir: PathBuf,
}

impl IrohTransport {
    /// Create a new Iroh transport instance
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        let iroh_data_dir = data_dir.join("iroh");
        std::fs::create_dir_all(&iroh_data_dir)?;

        info!("Initializing Iroh transport for node {}", node_id);

        // Create secret key for this node
        let secret_key = SecretKey::generate();
        
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
            connections: Arc::new(RwLock::new(HashMap::new())),
            node_id,
            data_dir: data_dir.to_path_buf(),
        })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        let node_id = self.endpoint.node_id();
        let addrs = self.endpoint.direct_addresses();
        
        Ok(NodeAddr::new(node_id).with_direct_addresses(addrs))
    }

    /// Connect to a peer
    async fn connect_to_peer(&self, addr: &NodeAddr, doc_type: DocumentType) -> BlixardResult<Connection> {
        let mut connections = self.connections.write().await;
        
        if let Some(conn) = connections.get(&addr.node_id) {
            // Check if connection is still alive by checking if it's closed
            let closed = conn.closed();
            if !closed {
                return Ok(conn.clone());
            }
        }
        
        let conn = self.endpoint
            .connect(addr.clone(), doc_type.to_alpn())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect to peer: {}", e),
            })?;
            
        connections.insert(addr.node_id, conn.clone());
        Ok(conn)
    }

    /// Share a file and return its hash
    /// Note: This is a simplified implementation that doesn't use blob store
    /// In a real implementation, you would use iroh-blobs store API
    pub async fn share_file(&self, path: &Path) -> BlixardResult<Hash> {
        info!("Sharing file: {:?}", path);
        
        // For now, we'll just compute a hash of the file
        // In a real implementation, this would import into a blob store
        let content = tokio::fs::read(path).await
            .map_err(|e| BlixardError::IoError(e))?;
            
        let hash = iroh_blobs::Hash::from_bytes(&content);
        
        info!("File shared with hash: {}", hash);
        
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
            
        Ok(())
    }

    /// Accept incoming connections and handle them
    pub async fn accept_connections<F>(&self, mut handler: F) -> BlixardResult<()>
    where
        F: FnMut(DocumentType, Vec<u8>) + Send + 'static,
    {
        while let Ok(incoming) = self.endpoint.accept().await {
            let alpn = incoming.alpn();
            let doc_type = match alpn {
                b"cluster-config" => DocumentType::ClusterConfig,
                b"vm-images" => DocumentType::VmImages,
                b"logs" => DocumentType::Logs,
                b"metrics" => DocumentType::Metrics,
                b"file-transfer" => DocumentType::FileTransfer,
                _ => continue,
            };
            
            let conn = match incoming.accept() {
                Ok(conn) => conn,
                Err(_) => continue,
            };
            
            tokio::spawn(async move {
                while let Ok(mut recv_stream) = conn.accept_uni().await {
                    let mut data = Vec::new();
                    if recv_stream.read_to_end(&mut data).await.is_ok() {
                        handler(doc_type, data);
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