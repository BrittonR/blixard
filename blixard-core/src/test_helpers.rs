//! Enhanced test helpers for integration tests
//!
//! This module provides utilities for setting up full nodes with Raft for testing,
//! including higher-level abstractions for cluster management, automatic port allocation,
//! and better wait conditions.

// Import necessary items
use crate::{
    error::{BlixardError, BlixardResult},
    node::Node,
    types::NodeConfig,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::Mutex;
use tempfile::TempDir;

// Re-export concurrent test utilities
pub use crate::test_helpers_concurrent::*;

// Re-export timing utilities
pub mod timing {
    pub use crate::test_helpers_modules::timing_helpers::*;
}

/// Test-specific node wrapper
pub struct TestNode {
    pub id: u64,
    pub addr: SocketAddr,
    pub node: Arc<Mutex<Node>>,
    pub data_dir: TempDir,
}

/// Test cluster for multi-node testing
pub struct TestCluster {
    nodes: HashMap<u64, TestNode>,
    next_port: u16,
}

impl TestCluster {
    /// Create a new test cluster
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_port: 50000,
        }
    }

    /// Create a builder for test cluster
    pub fn builder() -> Self {
        Self::new()
    }

    /// Add a new node to the cluster
    pub async fn add_node(&mut self) -> BlixardResult<u64> {
        let node_id = self.nodes.len() as u64 + 1;
        let addr: SocketAddr = format!("127.0.0.1:{}", self.next_port).parse().unwrap();
        self.next_port += 1;

        let data_dir = TempDir::new()?;
        let config = NodeConfig {
            id: node_id,
            bind_addr: addr,
            data_dir: data_dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let node = Node::new(config);
        let node = Arc::new(Mutex::new(node));

        let test_node = TestNode {
            id: node_id,
            addr,
            node: node.clone(),
            data_dir,
        };

        self.nodes.insert(node_id, test_node);
        Ok(node_id)
    }

    /// Get nodes map
    pub fn nodes(&self) -> &HashMap<u64, TestNode> {
        &self.nodes
    }

    /// Get a specific node by ID
    pub fn get_node(&self, node_id: u64) -> Option<&TestNode> {
        self.nodes.get(&node_id)
    }

    /// Remove a node from the cluster
    pub fn remove_node(&mut self, node_id: u64) -> Option<TestNode> {
        self.nodes.remove(&node_id)
    }

    /// Shutdown all nodes
    pub async fn shutdown(self) {
        for (_, test_node) in self.nodes {
            let mut node = test_node.node.lock().await;
            let _ = node.stop().await;
        }
    }
}

/// Wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    timeout: std::time::Duration,
) -> BlixardResult<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(BlixardError::Timeout {
        operation: "wait_for_condition".to_string(),
        duration: timeout,
    })
}