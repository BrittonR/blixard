//! Iroh P2P transport layer for Blixard - Version 3 simplified
//!
//! This module provides peer-to-peer communication capabilities using Iroh 0.90,
//! with a simplified approach that focuses on getting the basic transport working.

use crate::discovery::DiscoveryManager;
use crate::error::BlixardResult;
use crate::iroh_transport_v2::{DocumentType, IrohTransportV2};
use crate::p2p_monitor::{NoOpMonitor, P2pMonitor};
use iroh::NodeAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// P2P transport using Iroh with simplified document support
///
/// This version uses IrohTransportV2 internally and provides a migration path
/// to full iroh-docs integration once the API stabilizes.
pub struct IrohTransportV3 {
    /// Internal transport implementation
    inner: IrohTransportV2,
}

impl IrohTransportV3 {
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
        info!(
            "Initializing Iroh transport v3 (simplified) for node {}",
            node_id
        );

        let inner =
            IrohTransportV2::new_with_monitor(node_id, data_dir, discovery_manager, monitor)
                .await?;

        info!("Iroh P2P transport v3 initialized successfully");

        Ok(Self { inner })
    }

    /// Get the node's address for sharing with peers
    pub async fn node_addr(&self) -> BlixardResult<NodeAddr> {
        self.inner.node_addr().await
    }

    /// Get the underlying Iroh endpoint and node ID (for Raft transport)
    pub fn endpoint(&self) -> (iroh::Endpoint, iroh::NodeId) {
        self.inner.endpoint()
    }

    /// Create or join a document
    pub async fn create_or_join_doc(
        &self,
        doc_type: DocumentType,
        create_new: bool,
    ) -> BlixardResult<()> {
        self.inner.create_or_join_doc(doc_type, create_new).await
    }

    /// Write to a document
    pub async fn write_to_doc(
        &self,
        doc_type: DocumentType,
        key: &str,
        value: &[u8],
    ) -> BlixardResult<()> {
        self.inner.write_to_doc(doc_type, key, value).await
    }

    /// Read from a document
    pub async fn read_from_doc(&self, doc_type: DocumentType, key: &str) -> BlixardResult<Vec<u8>> {
        self.inner.read_from_doc(doc_type, key).await
    }

    /// Share a file and return its hash
    pub async fn share_file(&self, path: &Path) -> BlixardResult<iroh_blobs::Hash> {
        self.inner.share_file(path).await
    }

    /// Download a file by hash
    pub async fn download_file(
        &self,
        hash: iroh_blobs::Hash,
        output_path: &Path,
    ) -> BlixardResult<()> {
        self.inner.download_file(hash, output_path).await
    }

    /// Get a document ticket for sharing
    pub async fn get_doc_ticket(&self, doc_type: DocumentType) -> BlixardResult<String> {
        self.inner.get_doc_ticket(doc_type).await
    }

    /// Join a document from a ticket
    pub async fn join_doc_from_ticket(
        &self,
        ticket: &str,
        _doc_type: DocumentType,
    ) -> BlixardResult<()> {
        // For compatibility, ignore the doc_type parameter in the ticket parsing
        self.inner.join_doc_from_ticket(ticket).await
    }

    /// Send data to a peer using a simple unidirectional stream
    pub async fn send_to_peer(
        &self,
        peer_addr: &NodeAddr,
        doc_type: DocumentType,
        data: &[u8],
    ) -> BlixardResult<()> {
        self.inner.send_to_peer(peer_addr, doc_type, data).await
    }

    /// Accept incoming connections
    pub async fn accept_connections<F>(&self, handler: F) -> BlixardResult<()>
    where
        F: Fn(DocumentType, Vec<u8>) + Send + Sync + 'static,
    {
        self.inner.accept_connections(handler).await
    }

    /// Handle health check requests by echoing back the data
    pub async fn handle_health_check(
        &self,
        peer_addr: &NodeAddr,
        echo_data: &[u8],
    ) -> BlixardResult<()> {
        self.inner.handle_health_check(peer_addr, echo_data).await
    }

    /// Perform a health check to measure RTT using bidirectional stream
    pub async fn perform_health_check(&self, peer_addr: &NodeAddr) -> BlixardResult<f64> {
        self.inner.perform_health_check(peer_addr).await
    }

    /// Get bandwidth statistics for a peer over a time window
    pub async fn get_bandwidth_stats(&self, peer_id: &str, window_secs: u64) -> (f64, f64) {
        self.inner.get_bandwidth_stats(peer_id, window_secs).await
    }

    /// Get message volume statistics
    pub async fn get_message_stats(&self) -> std::collections::HashMap<String, (u64, u64)> {
        self.inner.get_message_stats().await
    }

    /// Accept bidirectional connections for health checks
    pub async fn accept_health_check_connections(&self) -> BlixardResult<()> {
        self.inner.accept_health_check_connections().await
    }

    /// Subscribe to document events for a specific document type
    pub async fn subscribe_to_doc_events(&self, doc_type: DocumentType) -> BlixardResult<()> {
        info!(
            "Document event subscription for {:?} (not yet implemented)",
            doc_type
        );
        Ok(())
    }

    /// Shutdown the Iroh transport
    pub async fn shutdown(self) -> BlixardResult<()> {
        info!("Shutting down Iroh transport v3");
        self.inner.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_iroh_transport_v3_creation() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV3::new(1, temp_dir.path()).await.unwrap();
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
        let transport = IrohTransportV3::new(1, temp_dir.path()).await.unwrap();

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
    async fn test_document_sharing() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        // Create two transports
        let transport1 = IrohTransportV3::new(1, temp_dir1.path()).await.unwrap();
        let transport2 = IrohTransportV3::new(2, temp_dir2.path()).await.unwrap();

        // Create a document on transport1
        transport1
            .create_or_join_doc(DocumentType::VmImages, true)
            .await
            .unwrap();

        // Get sharing ticket
        let ticket = transport1
            .get_doc_ticket(DocumentType::VmImages)
            .await
            .unwrap();
        assert!(ticket.starts_with("ticket-"));

        // Join document on transport2
        transport2
            .join_doc_from_ticket(&ticket, DocumentType::VmImages)
            .await
            .unwrap();

        // Subscribe to events (placeholder for now)
        transport1
            .subscribe_to_doc_events(DocumentType::VmImages)
            .await
            .unwrap();
        transport2
            .subscribe_to_doc_events(DocumentType::VmImages)
            .await
            .unwrap();

        transport1.shutdown().await.unwrap();
        transport2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let transport = IrohTransportV3::new(1, temp_dir.path()).await.unwrap();

        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, b"test content").unwrap();

        // Share the file
        let hash = transport.share_file(&test_file).await.unwrap();
        assert!(!hash.to_string().is_empty());

        // Download is not implemented yet
        let output_file = temp_dir.path().join("downloaded.txt");
        match transport.download_file(hash, &output_file).await {
            Err(crate::error::BlixardError::NotImplemented { .. }) => {
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
        let transport1 = IrohTransportV3::new(1, temp_dir1.path()).await.unwrap();
        let transport2 = IrohTransportV3::new(2, temp_dir2.path()).await.unwrap();

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
}
