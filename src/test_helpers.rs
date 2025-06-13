//! Test helpers for integration tests
//!
//! This module provides utilities for setting up full nodes with Raft for testing.

use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::{
    node::Node,
    node_shared::SharedNodeState,
    types::NodeConfig,
    grpc_server::start_grpc_server,
    error::BlixardResult,
};

/// Test node handle containing all necessary components
pub struct TestNode {
    pub node: Node,
    pub shared_state: Arc<SharedNodeState>,
    pub addr: SocketAddr,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub server_handle: Option<JoinHandle<()>>,
}

impl TestNode {
    /// Create and start a test node with full Raft setup
    pub async fn start(id: u64, port: u16) -> BlixardResult<Self> {
        Self::start_with_join(id, port, None).await
    }
    
    /// Create and start a test node that will join an existing cluster
    pub async fn start_with_join(id: u64, port: u16, join_addr: Option<SocketAddr>) -> BlixardResult<Self> {
        // Create temp directory for this test node
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_string_lossy().to_string();
        
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        let config = NodeConfig {
            id,
            bind_addr: addr,
            data_dir,
            join_addr,
            use_tailscale: false,
        };
        
        // Create node
        let mut node = Node::new(config);
        let shared_state = node.shared();
        
        // If we have a join address, add it as a peer before initializing
        // This helps the Raft layer know it's part of a cluster
        if let Some(join_addr) = join_addr {
            // Assume node 1 is always the bootstrap node for tests
            let _ = shared_state.add_peer(1, join_addr.to_string()).await;
        }
        
        // Initialize the node (database, raft, etc)
        node.initialize().await?;
        
        // Set running state
        shared_state.set_running(true).await;
        
        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        shared_state.set_shutdown_tx(shutdown_tx).await;
        
        // Start gRPC server
        let state_clone = shared_state.clone();
        let server_handle = tokio::spawn(async move {
            start_grpc_server(state_clone, addr).await.unwrap();
        });
        
        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        // Don't automatically send join request - let the test control this
        // if join_addr.is_some() {
        //     node.send_join_request().await?;
        // }
        
        Ok(TestNode {
            node,
            shared_state,
            addr,
            shutdown_tx: None,  // We already set the shutdown_tx in shared_state
            server_handle: Some(server_handle),
        })
    }
    
    /// Shutdown the test node
    pub async fn shutdown(mut self) {
        // Set running to false
        self.shared_state.set_running(false).await;
        
        // Abort server
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
        
        // Clear database
        self.shared_state.clear_database().await;
    }
}