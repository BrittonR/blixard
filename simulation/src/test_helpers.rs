//! Test helpers for simulation tests
//! 
//! This module provides self-contained test utilities that don't depend on blixard_core.
//! All test helpers are reimplemented here to work with MadSim's deterministic runtime.

// Madsim is automatically available when built with cfg(madsim)

use crate::proto::*;
use crate::NodeConfig;
use uuid::Uuid;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tonic::transport::Channel;

/// Port allocator for tests
pub struct PortAllocator {
    next_port: Arc<Mutex<u16>>,
}

impl PortAllocator {
    pub fn new(start_port: u16) -> Self {
        Self {
            next_port: Arc::new(Mutex::new(start_port)),
        }
    }

    pub async fn allocate_port(&self) -> u16 {
        let mut port = self.next_port.lock().unwrap();
        let allocated = *port;
        *port += 1;
        allocated
    }
}

/// Test node wrapper for simulation tests
pub struct TestNode {
    pub id: u64,
    pub bind_addr: SocketAddr,
    pub data_dir: String,
    pub client: Option<cluster_service_client::ClusterServiceClient<Channel>>,
}

impl TestNode {
    /// Create a new test node
    pub async fn new(id: u64, port_allocator: &PortAllocator) -> Self {
        let port = port_allocator.allocate_port().await;
        let bind_addr = format!("127.0.0.1:{}", port).parse().unwrap();
        let data_dir = format!("/tmp/blixard-sim-test-{}-{}", id, Uuid::new_v4());
        
        Self {
            id,
            bind_addr,
            data_dir,
            client: None,
        }
    }

    /// Start the node (simulated)
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // In simulation, we create a mock client
        // Real implementation would start a server
        // Note: In real tests, the server must be running before creating a client
        // For now, we don't create a real client in simulation
        // Tests should either mock the behavior or start a real server first
        Ok(())
    }

    /// Get a gRPC client for this node
    pub fn client(&self) -> Option<&cluster_service_client::ClusterServiceClient<Channel>> {
        self.client.as_ref()
    }

    /// Stop the node
    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client = None;
        Ok(())
    }

    /// Get node configuration
    pub fn config(&self) -> NodeConfig {
        NodeConfig {
            id: self.id,
            bind_addr: self.bind_addr,
            data_dir: self.data_dir.clone(),
            join_addr: None,
            use_tailscale: false,
        }
    }
}

/// Test cluster for multi-node tests
pub struct TestCluster {
    nodes: Vec<TestNode>,
    port_allocator: PortAllocator,
}

impl TestCluster {
    /// Create a new test cluster
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            port_allocator: PortAllocator::new(30000),
        }
    }

    /// Add a node to the cluster
    pub async fn add_node(&mut self) -> Result<&mut TestNode, Box<dyn std::error::Error>> {
        let id = self.nodes.len() as u64 + 1;
        let mut node = TestNode::new(id, &self.port_allocator).await;
        node.start().await?;
        self.nodes.push(node);
        Ok(self.nodes.last_mut().unwrap())
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[TestNode] {
        &self.nodes
    }

    /// Get a specific node by ID
    pub fn get_node(&self, id: u64) -> Option<&TestNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Stop all nodes
    pub async fn stop_all(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for node in &mut self.nodes {
            node.stop().await?;
        }
        Ok(())
    }
}

/// Timing utilities for deterministic tests
pub mod timing {
    use std::time::Duration;
    use madsim::time::{sleep, Instant};

    /// Environment-aware sleep that scales with test environment
    pub async fn robust_sleep(duration: Duration) {
        let multiplier = if std::env::var("CI").is_ok() { 3 } else { 1 };
        let scaled_duration = duration * multiplier as u32;
        sleep(scaled_duration).await;
    }

    /// Wait for a condition with exponential backoff
    pub async fn wait_for_condition_with_backoff<F, Fut>(
        mut condition: F,
        max_wait: Duration,
        initial_interval: Duration,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<bool, Box<dyn std::error::Error>>>,
    {
        let start = Instant::now();
        let mut interval = initial_interval;
        
        while start.elapsed() < max_wait {
            if condition().await? {
                return Ok(());
            }
            sleep(interval).await;
            interval = std::cmp::min(interval * 2, Duration::from_secs(1));
        }
        
        Err("Condition not met within timeout".into())
    }

    /// Get a scaled timeout for the current environment
    pub fn scaled_timeout(base: Duration) -> Duration {
        let multiplier = if std::env::var("CI").is_ok() { 3 } else { 1 };
        base * multiplier as u32
    }
}

/// Wait for a condition (simple version)
pub async fn wait_for_condition<F>(
    condition: F,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn() -> bool,
{
    let start = madsim::time::Instant::now();
    while !condition() {
        if start.elapsed() > timeout {
            return Err("Timeout waiting for condition".into());
        }
        madsim::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}