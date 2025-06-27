//! Iroh P2P transport layer for Blixard
//!
//! This module provides peer-to-peer communication capabilities using Iroh,
//! enabling direct connections between nodes for efficient data transfer.

use crate::error::{BlixardError, BlixardResult};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use iroh::node::Node;
use iroh::docs::DocTicket;
use iroh::net::NodeAddr;
use iroh::client::docs::Doc;
use iroh::docs::Author;
use iroh::base::hash::Hash;
use futures::StreamExt;
use tracing::info;

/// Types of documents used in Blixard
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

/// P2P transport using Iroh for node-to-node communication
pub struct IrohTransport {
    /// The Iroh node instance
    node: Node<iroh::blobs::store::fs::Store>,
    /// Iroh client for operations
    client: iroh::client::Iroh,
    /// Author key for signing documents
    author: Author,
    /// Active documents by type
    docs: Arc<RwLock<HashMap<DocumentType, Doc>>>,
    /// Node ID for identification
    node_id: u64,
}

impl IrohTransport {
    /// Create a new Iroh transport instance
    pub async fn new(node_id: u64, data_dir: &Path) -> BlixardResult<Self> {
        let iroh_data_dir = data_dir.join("iroh");
        std::fs::create_dir_all(&iroh_data_dir)?;

        info!("Initializing Iroh transport for node {}", node_id);

        // Create Iroh node
        let node = Node::persistent(&iroh_data_dir)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create Iroh node: {}", e),
            })?
            .spawn()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to spawn Iroh node: {}", e),
            })?;

        let client = node.client().clone();
        
        // Create or load author
        let authors = client.authors().list().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to list authors: {}", e),
            })?;
        
        let authors_list: Vec<_> = authors.collect().await;
        let author = if let Some(Ok(author)) = authors_list.into_iter().next() {
            author
        } else {
            client.authors().create().await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to create author: {}", e),
                })?
        };

        let iroh_node_id = node.node_id();
        info!("Iroh node initialized with ID: {}", iroh_node_id);

        Ok(Self {
            node,
            client,
            author,
            docs: Arc::new(RwLock::new(HashMap::new())),
            node_id,
        })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        let node_id = self.node.node_id();
        let info = self.client.node().status().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to get node status: {}", e),
            })?;
        
        let relay_url = info.listen_addrs
            .iter()
            .find(|addr| addr.to_string().contains("relay"))
            .and_then(|addr| {
                // Try to parse as relay URL
                if let Ok(url) = addr.to_string().parse() {
                    Some(iroh::net::relay::RelayUrl::from(url))
                } else {
                    None
                }
            });
            
        Ok(NodeAddr::new(node_id))
    }

    /// Create or join a document
    pub async fn create_or_join_doc(&self, doc_type: DocumentType) -> BlixardResult<Doc> {
        let mut docs = self.docs.write().await;
        
        if let Some(doc) = docs.get(&doc_type) {
            return Ok(doc.clone());
        }

        info!("Creating new document for {:?}", doc_type);
        
        let doc = self.client.docs().create()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create document: {}", e),
            })?;

        docs.insert(doc_type, doc.clone());
        Ok(doc)
    }

    /// Join a document from a ticket
    pub async fn join_doc_from_ticket(&self, ticket: &DocTicket, doc_type: DocumentType) -> BlixardResult<Doc> {
        let mut docs = self.docs.write().await;
        
        info!("Joining document {:?} from ticket", doc_type);
        
        let doc = self.client.docs().import(ticket.clone())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to import document: {}", e),
            })?;

        docs.insert(doc_type, doc.clone());
        Ok(doc)
    }

    /// Share a file and return its hash
    pub async fn share_file(&self, path: &Path) -> BlixardResult<Hash> {
        use futures::StreamExt;
        
        info!("Sharing file: {:?}", path);
        
        let mut stream = self.client.blobs().add_from_path(
            path.to_path_buf(),
            false, // in_place
            iroh::blobs::util::SetTagOption::Auto,
            iroh::client::blobs::WrapOption::NoWrap,
        )
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to import file: {}", e),
        })?;

        let mut hash = None;
        while let Some(progress) = stream.next().await {
            if let Ok(iroh::client::blobs::AddProgress::Done { hash: h }) = progress {
                hash = Some(h);
                break;
            }
        }
        
        let hash = hash.ok_or_else(|| BlixardError::Internal {
            message: "Failed to get hash from import".to_string(),
        })?;
        
        info!("File shared with hash: {}", hash);
        
        Ok(hash)
    }

    /// Download a file by hash
    pub async fn download_file(&self, hash: Hash, output_path: &Path) -> BlixardResult<()> {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;
        
        info!("Downloading file with hash: {}", hash);
        
        // Get the blob data
        let mut stream = self.client.blobs().read(hash)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to start download: {}", e),
            })?;
            
        // Create output file
        let mut file = tokio::fs::File::create(output_path)
            .await
            .map_err(|e| BlixardError::IoError(e))?;
            
        // Write chunks to file
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| BlixardError::Internal {
                message: format!("Failed to read chunk: {}", e),
            })?;
            
            file.write_all(&chunk).await
                .map_err(|e| BlixardError::IoError(e))?;
        }
        
        file.flush().await
            .map_err(|e| BlixardError::IoError(e))?;

        info!("File downloaded to: {:?}", output_path);
        Ok(())
    }

    /// Write data to a document
    pub async fn write_to_doc(
        &self, 
        doc_type: DocumentType, 
        key: &[u8], 
        value: &[u8]
    ) -> BlixardResult<()> {
        let docs = self.docs.read().await;
        let doc = docs.get(&doc_type)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Document {:?}", doc_type),
            })?;

        doc.set_bytes(self.author, key.to_vec(), value.to_vec())
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to write to document: {}", e),
            })?;

        Ok(())
    }

    /// Read data from a document
    pub async fn read_from_doc(
        &self, 
        doc_type: DocumentType, 
        key: &[u8]
    ) -> BlixardResult<Option<Vec<u8>>> {
        let docs = self.docs.read().await;
        let doc = docs.get(&doc_type)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Document {:?}", doc_type),
            })?;

        let entry = doc.get_exact(self.author, key, false)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to read from document: {}", e),
            })?;

        if let Some(entry) = entry {
            // Need to fetch content through the client
            let content = entry.content_bytes(&self.client).await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to get content: {}", e),
                })?;
            Ok(Some(content.to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Subscribe to document updates
    pub async fn subscribe_to_doc(&self, doc_type: DocumentType) -> BlixardResult<impl futures::Stream<Item = iroh::client::docs::LiveEvent>> {
        let docs = self.docs.read().await;
        let doc = docs.get(&doc_type)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Document {:?}", doc_type),
            })?;

        let stream = doc.subscribe()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to subscribe to document: {}", e),
            })?;

        Ok(stream)
    }

    /// Get a document ticket for sharing
    pub async fn get_doc_ticket(&self, doc_type: DocumentType) -> BlixardResult<DocTicket> {
        let docs = self.docs.read().await;
        let doc = docs.get(&doc_type)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Document {:?}", doc_type),
            })?;

        let ticket = doc.share(iroh::client::docs::ShareMode::Read)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create document ticket: {}", e),
            })?;

        Ok(ticket)
    }

    /// Shutdown the Iroh node
    pub async fn shutdown(self) -> BlixardResult<()> {
        info!("Shutting down Iroh transport");
        self.node.shutdown()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to shutdown Iroh node: {}", e),
            })
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
    async fn test_document_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransport::new(1, temp_dir.path()).await.unwrap();
        
        // Create a document
        let _doc = transport.create_or_join_doc(DocumentType::ClusterConfig).await.unwrap();
        
        // Write and read data
        transport.write_to_doc(DocumentType::ClusterConfig, b"test-key", b"test-value").await.unwrap();
        let value = transport.read_from_doc(DocumentType::ClusterConfig, b"test-key").await.unwrap();
        assert_eq!(value, Some(b"test-value".to_vec()));
        
        transport.shutdown().await.unwrap();
    }
}