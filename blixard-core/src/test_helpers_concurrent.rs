//! Concurrent testing utilities for multi-node clusters
//!
//! This module provides a concurrent-safe wrapper around TestCluster
//! to enable testing scenarios that require multiple concurrent operations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::{
    error::BlixardResult,
    test_helpers::{TestCluster, TestNode},
};

/// A cloneable handle to a test cluster that allows concurrent operations
#[derive(Clone)]
pub struct ConcurrentTestCluster {
    inner: Arc<Mutex<TestCluster>>,
    /// Shared view of node information for read operations
    node_info: Arc<RwLock<HashMap<u64, NodeInfo>>>,
}

/// Basic information about a node that can be safely shared
#[derive(Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub addr: SocketAddr,
}

impl ConcurrentTestCluster {
    /// Create a new concurrent test cluster from an existing TestCluster
    pub fn new(cluster: TestCluster) -> Self {
        // Extract node information
        let node_info: HashMap<u64, NodeInfo> = cluster
            .nodes()
            .iter()
            .map(|(id, node)| {
                (
                    *id,
                    NodeInfo {
                        id: node.id,
                        addr: node.addr,
                    },
                )
            })
            .collect();

        Self {
            inner: Arc::new(Mutex::new(cluster)),
            node_info: Arc::new(RwLock::new(node_info)),
        }
    }

    /// Add a new node to the cluster (requires exclusive access)
    pub async fn add_node(&self) -> BlixardResult<u64> {
        let mut cluster = self.inner.lock().await;
        let node_id = cluster.add_node().await?;

        // Update node info
        if let Some(node) = cluster.nodes().get(&node_id) {
            let mut info = self.node_info.write().await;
            info.insert(
                node_id,
                NodeInfo {
                    id: node.id,
                    addr: node.addr,
                },
            );
        }

        Ok(node_id)
    }

    // TODO: Re-implement these methods with Iroh P2P transport
    // /// Get a client for a specific node
    // pub async fn client(&self, node_id: u64) -> BlixardResult<ClusterServiceClient<Channel>> {
    //     let cluster = self.inner.lock().await;
    //     cluster.client(node_id).await
    // }
    //
    // /// Get the leader client
    // pub async fn leader_client(&self) -> BlixardResult<ClusterServiceClient<Channel>> {
    //     let cluster = self.inner.lock().await;
    //     cluster.leader_client().await
    // }

    /// Get node information without locking the cluster
    pub async fn get_node_info(&self, node_id: u64) -> Option<NodeInfo> {
        let info = self.node_info.read().await;
        info.get(&node_id).cloned()
    }

    /// Get all node IDs
    pub async fn node_ids(&self) -> Vec<u64> {
        let info = self.node_info.read().await;
        info.keys().copied().collect()
    }

    /// Shutdown the cluster
    pub async fn shutdown(self) {
        // Only shutdown if this is the last reference
        if let Ok(inner) = Arc::try_unwrap(self.inner) {
            let cluster = inner.into_inner();
            cluster.shutdown().await;
        }
    }

    /// Execute an operation that requires exclusive access to a node
    pub async fn with_node<F, R>(&self, node_id: u64, f: F) -> BlixardResult<R>
    where
        F: FnOnce(&TestNode) -> R,
    {
        let cluster = self.inner.lock().await;
        match cluster.get_node(node_id) {
            Some(node) => Ok(f(node)),
            None => Err(crate::error::BlixardError::NotFound {
                resource: format!("node {}", node_id),
            }),
        }
    }

    /// Get a reference to the shared state of a node
    pub async fn get_node_shared_state(
        &self,
        node_id: u64,
    ) -> BlixardResult<Arc<crate::node_shared::SharedNodeState>> {
        let cluster = self.inner.lock().await;
        match cluster.get_node(node_id) {
            Some(test_node) => {
                let node = test_node.node.lock().await;
                Ok(node.shared())
            }
            None => Err(crate::error::BlixardError::NotFound {
                resource: format!("node {}", node_id),
            }),
        }
    }

    /// Stop a node in the cluster (simulates node failure)
    pub async fn stop_node(&self, node_id: u64) -> BlixardResult<()> {
        let mut cluster = self.inner.lock().await;

        // Use the existing remove_node method which properly handles node removal
        if cluster.remove_node(node_id).is_none() {
            return Err(crate::error::BlixardError::NotFound {
                resource: format!("node {}", node_id),
            });
        }

        // Remove from node info
        let mut info = self.node_info.write().await;
        info.remove(&node_id);

        Ok(())
    }
}

/// Builder for concurrent test clusters
pub struct ConcurrentTestClusterBuilder {
    node_count: usize,
    convergence_timeout: std::time::Duration,
}

impl ConcurrentTestClusterBuilder {
    pub fn new() -> Self {
        Self {
            node_count: 3,
            convergence_timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn with_nodes(mut self, count: usize) -> Self {
        self.node_count = count;
        self
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.convergence_timeout = timeout;
        self
    }

    pub async fn build(self) -> BlixardResult<ConcurrentTestCluster> {
        let mut cluster = TestCluster::new();

        // Add the requested number of nodes
        for _ in 0..self.node_count {
            cluster.add_node().await?;
        }

        Ok(ConcurrentTestCluster::new(cluster))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_cluster_clone() {
        // Initialize metrics
        let _ = crate::metrics_otel::init_noop();

        let cluster = ConcurrentTestClusterBuilder::new()
            .with_nodes(2)
            .build()
            .await
            .expect("Failed to create cluster");

        // Clone the cluster handle
        let cluster_clone = cluster.clone();

        // Both handles should see the same nodes
        let ids1 = cluster.node_ids().await;
        let ids2 = cluster_clone.node_ids().await;

        assert_eq!(ids1, ids2);
        assert_eq!(ids1.len(), 2);

        cluster.shutdown().await;
    }

    #[tokio::test]
    async fn test_concurrent_node_addition() {
        // Initialize metrics
        let _ = crate::metrics_otel::init_noop();

        let cluster = ConcurrentTestClusterBuilder::new()
            .with_nodes(1)
            .build()
            .await
            .expect("Failed to create cluster");

        let cluster_clone = cluster.clone();

        // Add a node from the cloned handle
        let node_id = cluster_clone.add_node().await.expect("Failed to add node");

        // Both handles should see the new node
        let ids = cluster.node_ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&node_id));

        cluster.shutdown().await;
    }
}
