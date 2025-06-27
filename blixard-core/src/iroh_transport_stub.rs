//! Stub implementation of Iroh transport to allow compilation
//!
//! This is a temporary stub while we update to the latest Iroh API

use crate::error::{BlixardError, BlixardResult};
use std::path::Path;
use iroh::NodeAddr;

// Re-export Hash from iroh-blobs
pub use iroh_blobs::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentType {
    ClusterConfig,
    VmImages,
    Logs,
    Metrics,
    FileTransfer,
}

pub struct IrohTransport {
    node_id: u64,
}

impl IrohTransport {
    pub async fn new(node_id: u64, _data_dir: &Path) -> BlixardResult<Self> {
        Ok(Self { node_id })
    }
    
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn create_or_join_doc(&self, _doc_type: DocumentType) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn send_to_peer(&self, _peer_addr: &NodeAddr, _doc_type: DocumentType, _data: &[u8]) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn share_file(&self, _path: &Path) -> BlixardResult<Hash> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn download_file(&self, _hash: Hash, _output_path: &Path) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn accept_connections<F>(&self, _handler: F) -> BlixardResult<()>
    where
        F: FnMut(DocumentType, Vec<u8>) + Send + 'static,
    {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn shutdown(self) -> BlixardResult<()> {
        Ok(())
    }
}