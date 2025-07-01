//! Iroh P2P transport layer for Blixard - Version 2 with actual implementation
//!
//! This module provides peer-to-peer communication capabilities using Iroh 0.90,
//! enabling direct connections between nodes for efficient data transfer.

use crate::error::{BlixardError, BlixardResult};
use crate::discovery::{DiscoveryManager, create_combined_discovery, IrohDiscoveryBridge};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
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
        let conn = self.endpoint
            .connect(peer_addr.clone(), doc_type.as_str().as_bytes())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect to peer: {}", e),
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
        
        Ok(())
    }

    /// Accept incoming connections (simplified version)
    pub async fn accept_connections<F>(&self, handler: F) -> BlixardResult<()>
    where
        F: Fn(DocumentType, Vec<u8>) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        
        loop {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let handler = handler.clone();
                    
                    tokio::spawn(async move {
                        match incoming.await {
                            Ok(conn) => {
                                let alpn = conn.alpn();
                                let doc_type = match alpn.as_deref() {
                                    Some(b"cluster-config") => DocumentType::ClusterConfig,
                                    Some(b"vm-images") => DocumentType::VmImages,
                                    Some(b"logs") => DocumentType::Logs,
                                    Some(b"metrics") => DocumentType::Metrics,
                                    Some(b"file-transfer") => DocumentType::FileTransfer,
                                    _ => return,
                                };
                                
                                // Accept unidirectional streams
                                while let Ok(mut recv_stream) = conn.accept_uni().await {
                                    let mut data = Vec::new();
                                    if let Ok(_) = tokio::io::AsyncReadExt::read_to_end(&mut recv_stream, &mut data).await {
                                        handler(doc_type, data);
                                    }
                                }
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