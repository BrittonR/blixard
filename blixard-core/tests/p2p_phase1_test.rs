//! Test for P2P Phase 1 implementation
//!
//! Verifies that the core document operations for state synchronization are working

use blixard_core::{
    error::BlixardResult,
    iroh_transport_v2::{DocumentType, IrohTransportV2},
    p2p_manager::P2pManager,
};
use std::sync::Arc;
use tokio;
use tracing_test::traced_test;

#[tokio::test]
#[traced_test]
async fn test_p2p_metadata_operations() -> BlixardResult<()> {
    // Create temp directory for test data
    let temp_dir = tempfile::tempdir()?;

    // Create P2P manager
    let p2p_manager = P2pManager::new(1, temp_dir.path(), Default::default()).await?;

    // Test metadata storage and retrieval
    let key = "test-key";
    let value = b"test-value";

    // Store metadata
    p2p_manager.store_metadata(key, value).await?;

    // Retrieve metadata
    let retrieved = p2p_manager.get_metadata(key).await?;
    assert_eq!(retrieved, value);

    // Test with different data
    let key2 = "config/cluster";
    let value2 = b"{\"nodes\": [1, 2, 3]}";

    p2p_manager.store_metadata(key2, value2).await?;
    let retrieved2 = p2p_manager.get_metadata(key2).await?;
    assert_eq!(retrieved2, value2);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_p2p_blob_operations() -> BlixardResult<()> {
    // Create temp directory for test data
    let temp_dir = tempfile::tempdir()?;

    // Create P2P manager
    let p2p_manager = P2pManager::new(1, temp_dir.path(), Default::default()).await?;

    // Create a test file
    let test_data = b"This is test blob data for P2P transfer";
    let test_file = temp_dir.path().join("test.dat");
    tokio::fs::write(&test_file, test_data).await?;

    // Share the file
    let hash = p2p_manager.share_data(&test_file, "test-blob").await?;

    // Download using the hash (should get it from local store)
    let downloaded = p2p_manager.download_data(&hash).await?;
    assert_eq!(downloaded, test_data);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_p2p_document_sync() -> BlixardResult<()> {
    // This test verifies that document operations can be used for state synchronization
    let temp_dir = tempfile::tempdir()?;

    // Create transport directly to test document operations
    let transport = Arc::new(IrohTransportV2::new(1).await?);

    // Test various document types
    transport
        .create_or_join_doc(DocumentType::ClusterConfig)
        .await?;
    transport.create_or_join_doc(DocumentType::Metadata).await?;
    transport
        .create_or_join_doc(DocumentType::NodeState)
        .await?;

    // Write state to different documents
    let cluster_config = b"cluster_version: 1.0";
    transport
        .write_to_doc(DocumentType::ClusterConfig, "version", cluster_config)
        .await?;

    let node_state = b"status: healthy";
    transport
        .write_to_doc(DocumentType::NodeState, "node-1", node_state)
        .await?;

    // Read back the state
    let read_config = transport
        .read_from_doc(DocumentType::ClusterConfig, "version")
        .await?;
    assert_eq!(read_config, cluster_config);

    let read_state = transport
        .read_from_doc(DocumentType::NodeState, "node-1")
        .await?;
    assert_eq!(read_state, node_state);

    Ok(())
}
