//! Stub implementation of Iroh transport to allow compilation
//!
//! This is a temporary stub while we update to the latest Iroh API

use crate::error::{BlixardError, BlixardResult};
use std::path::Path;
use iroh::docs::DocTicket;
use iroh::net::NodeAddr;
// Stub Hash type
#[derive(Debug, Clone)]
pub struct Hash(String);

impl Hash {
    pub fn from_hex(_hex: &str) -> Result<Self, std::io::Error> {
        Ok(Hash("stub_hash".to_string()))
    }
    
    pub fn to_hex(&self) -> String {
        self.0.clone()
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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
    
    pub async fn join_doc_from_ticket(&self, _ticket: &DocTicket, _doc_type: DocumentType) -> BlixardResult<()> {
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
    
    pub async fn write_to_doc(&self, _doc_type: DocumentType, _key: &[u8], _value: &[u8]) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn read_from_doc(&self, _doc_type: DocumentType, _key: &[u8]) -> BlixardResult<Option<Vec<u8>>> {
        Ok(None)
    }
    
    pub async fn subscribe_to_doc(&self, _doc_type: DocumentType) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn get_doc_ticket(&self, _doc_type: DocumentType) -> BlixardResult<DocTicket> {
        Err(BlixardError::NotImplemented {
            feature: "Iroh transport".to_string(),
        })
    }
    
    pub async fn shutdown(self) -> BlixardResult<()> {
        Ok(())
    }
}