//! P2P integration tests

#![cfg(feature = "test-helpers")]

use blixard_core::{
    iroh_transport::{IrohTransport, DocumentType},
    p2p_manager::{P2pManager, P2pConfig},
    p2p_image_store::P2pImageStore,
};
use std::path::Path;
use tempfile::TempDir;

#[tokio::test]
async fn test_iroh_transport_creation() {
    let temp_dir = TempDir::new().unwrap();
    let transport = IrohTransport::new(1, temp_dir.path()).await;
    assert!(transport.is_ok(), "Should create IrohTransport successfully");
    
    let transport = transport.unwrap();
    let node_addr = transport.node_addr().await;
    assert!(node_addr.is_ok(), "Should get node address");
    
    let addr = node_addr.unwrap();
    assert!(!addr.node_id.to_string().is_empty(), "Should have valid node ID");
}

#[tokio::test]
async fn test_p2p_manager_creation() {
    let temp_dir = TempDir::new().unwrap();
    let config = P2pConfig::default();
    
    let manager = P2pManager::new(1, temp_dir.path(), config).await;
    // P2pManager creation will fail because P2pImageStore tries to create documents
    // which are not implemented in the current Iroh transport
    assert!(manager.is_err(), "P2pManager creation should fail due to unimplemented document operations");
    if let Err(e) = manager {
        println!("P2pManager error: {}", e.to_string());
        assert!(e.to_string().contains("not implemented") || e.to_string().contains("feature"));
    }
}

#[tokio::test]
async fn test_p2p_image_store_creation() {
    let temp_dir = TempDir::new().unwrap();
    
    let store = P2pImageStore::new(1, temp_dir.path()).await;
    // P2pImageStore creation will fail because it tries to create documents
    // which are not implemented in the current Iroh transport
    assert!(store.is_err(), "P2pImageStore creation should fail due to unimplemented document operations");
    if let Err(e) = store {
        println!("P2pImageStore error: {}", e.to_string());
        assert!(e.to_string().contains("not implemented") || e.to_string().contains("feature"));
    }
}

#[tokio::test]
async fn test_document_operations_return_not_implemented() {
    let temp_dir = TempDir::new().unwrap();
    let transport = IrohTransport::new(1, temp_dir.path()).await.unwrap();
    
    // Test that document operations return NotImplemented errors
    let result = transport.create_or_join_doc(DocumentType::ClusterConfig, true).await;
    assert!(result.is_err(), "Document creation should return error");
    if let Err(e) = result {
        println!("Error string: {}", e.to_string());
        assert!(e.to_string().contains("not implemented") || e.to_string().contains("NotImplemented"));
    }
    
    let result = transport.write_to_doc(DocumentType::ClusterConfig, "test", b"data").await;
    assert!(result.is_err(), "Document write should return error");
    if let Err(e) = result {
        assert!(e.to_string().contains("not implemented") || e.to_string().contains("NotImplemented"));
    }
    
    let result = transport.read_from_doc(DocumentType::ClusterConfig, "test").await;
    assert!(result.is_err(), "Document read should return error");
    if let Err(e) = result {
        assert!(e.to_string().contains("not implemented") || e.to_string().contains("NotImplemented"));
    }
}