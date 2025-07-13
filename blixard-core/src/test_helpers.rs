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
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tempfile::TempDir;
use tokio::sync::Mutex;

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
    pub shared_state: Arc<crate::node_shared::SharedNodeState>,
}

impl TestNode {
    /// Create a new test node
    pub async fn new(node_id: u64) -> BlixardResult<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", 50000 + node_id).parse().unwrap();
        let data_dir = TempDir::new()?;
        let config = NodeConfig {
            id: node_id,
            bind_addr: addr,
            data_dir: data_dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let node = Node::new(config);
        let shared_state = node.shared();
        let node = Arc::new(Mutex::new(node));

        Ok(TestNode {
            id: node_id,
            addr,
            node,
            data_dir,
            shared_state,
        })
    }

    /// Get reference to the node
    pub fn node(&self) -> &Arc<Mutex<Node>> {
        &self.node
    }

    /// Get mutable reference to the node
    pub fn node_mut(&mut self) -> &mut Arc<Mutex<Node>> {
        &mut self.node
    }

    /// Start the node
    pub async fn start(&self) -> BlixardResult<()> {
        let mut node = self.node.lock().await;
        node.initialize().await
    }

    /// Stop the node
    pub async fn stop(&self) -> BlixardResult<()> {
        let mut node = self.node.lock().await;
        node.stop().await
    }

    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        let _node = self.node.lock().await;
        // This is a simple check - in reality we'd need to check the node state
        true // TODO: Implement proper running state check
    }

    /// Get the shared state of the node (async access)
    pub async fn shared_state(&self) -> Arc<crate::node_shared::SharedNodeState> {
        let node = self.node.lock().await;
        node.shared()
    }
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

    /// Create a cluster with a specific number of nodes
    pub async fn with_size(node_count: usize) -> BlixardResult<Self> {
        let mut cluster = Self::new();
        for _ in 0..node_count {
            cluster.add_node().await?;
        }
        Ok(cluster)
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
        let shared_state = node.shared();
        let node = Arc::new(Mutex::new(node));

        let test_node = TestNode {
            id: node_id,
            addr,
            node: node.clone(),
            data_dir,
            shared_state,
        };

        self.nodes.insert(node_id, test_node);
        Ok(node_id)
    }

    /// Get nodes map
    pub fn nodes(&self) -> &HashMap<u64, TestNode> {
        &self.nodes
    }

    /// Create cluster with specific number of nodes (alias for with_size)
    pub async fn with_nodes(node_count: usize) -> BlixardResult<Self> {
        Self::with_size(node_count).await
    }

    /// Wait for leader election and return leader node
    pub async fn wait_for_leader(&self, timeout: std::time::Duration) -> BlixardResult<u64> {
        self.wait_for_convergence(timeout).await
    }

    /// Get a specific node by ID
    pub fn get_node(&self, node_id: u64) -> Option<&TestNode> {
        self.nodes.get(&node_id)
    }

    /// Remove a node from the cluster
    pub fn remove_node(&mut self, node_id: u64) -> Option<TestNode> {
        self.nodes.remove(&node_id)
    }

    /// Wait for cluster convergence (leader election)
    pub async fn wait_for_convergence(&self, timeout: std::time::Duration) -> BlixardResult<u64> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(leader_id) = self.get_leader_id().await {
                return Ok(leader_id);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Err(BlixardError::Timeout {
            operation: "wait_for_convergence".to_string(),
            duration: timeout,
        })
    }

    /// Get the current leader ID
    pub async fn get_leader_id(&self) -> BlixardResult<u64> {
        for (node_id, test_node) in &self.nodes {
            if test_node.shared_state.is_leader() {
                return Ok(*node_id);
            }
        }
        Err(BlixardError::NodeNotFound {
            node_id: 0, // Use 0 to indicate "no leader found"
        })
    }

    /// Check if cluster is formed (has a leader)
    pub async fn is_cluster_formed(&self) -> bool {
        self.get_leader_id().await.is_ok()
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

/// Create a test database with temporary directory
/// Returns (Database, TempDir) tuple for tests
pub async fn create_test_database() -> (Arc<redb::Database>, TempDir) {
    use redb::Database;
    
    let temp_dir = TempDir::new().expect("Should be able to create temp dir for tests");
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path).expect("Should be able to create test database"));
    
    (database, temp_dir)
}
