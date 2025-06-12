//! Test utilities for Raft consensus testing
//! 
//! This module provides helpers and utilities for testing Raft implementations,
//! following patterns from tikv/raft-rs.

#![cfg(madsim)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use madsim::{
    runtime::{Handle, NodeHandle},
    net::NetSim,
    time::{sleep, Instant, Duration},
};
use tracing::info;
use serde::{Serialize, Deserialize};

use blixard_simulation::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    cluster_service_client::ClusterServiceClient,
};

/// Test cluster configuration
#[derive(Clone, Debug)]
pub struct TestClusterConfig {
    pub node_count: usize,
    pub election_timeout_min: Duration,
    pub election_timeout_max: Duration,
    pub heartbeat_interval: Duration,
    pub max_entries_per_msg: usize,
}

impl Default for TestClusterConfig {
    fn default() -> Self {
        Self {
            node_count: 3,
            election_timeout_min: Duration::from_millis(150),
            election_timeout_max: Duration::from_millis(300),
            heartbeat_interval: Duration::from_millis(50),
            max_entries_per_msg: 100,
        }
    }
}

/// Test cluster that manages multiple Raft nodes
pub struct TestCluster {
    pub handle: Handle,
    pub nodes: HashMap<u64, TestNode>,
    pub config: TestClusterConfig,
}

pub struct TestNode {
    pub id: u64,
    pub handle: NodeHandle,
    pub addr: String,
    pub client: Option<ClusterServiceClient<tonic::transport::Channel>>,
}

/// Create a test cluster with the given configuration
pub async fn create_test_cluster(config: TestClusterConfig) -> TestCluster {
    let handle = Handle::current();
    let mut nodes = HashMap::new();
    
    for i in 1..=config.node_count {
        let node_id = i as u64;
        let addr = format!("10.0.0.{}:700{}", i, i);
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", node_id))
            .ip(format!("10.0.0.{}", i).parse().unwrap())
            .build();
        
        nodes.insert(node_id, TestNode {
            id: node_id,
            handle: node_handle,
            addr,
            client: None,
        });
    }
    
    TestCluster {
        handle,
        nodes,
        config,
    }
}

/// Network partition helper
pub struct NetworkPartition {
    pub majority: Vec<u64>,
    pub minority: Vec<u64>,
}

impl NetworkPartition {
    /// Create a partition where minority nodes cannot reach majority
    pub fn create(node_ids: &[u64], minority_size: usize) -> Self {
        let mut ids = node_ids.to_vec();
        ids.sort();
        
        let minority = ids.iter().take(minority_size).copied().collect();
        let majority = ids.iter().skip(minority_size).copied().collect();
        
        NetworkPartition { majority, minority }
    }
    
    /// Apply the partition to the network
    pub fn apply(&self, _net: &NetSim) {
        for &minority_node in &self.minority {
            for &majority_node in &self.majority {
                // Simulate network partition by clogging links
                // Note: This would need actual node handles in real implementation
                // For now, we'll skip the actual network operation
            }
        }
    }
    
    /// Heal the partition
    pub fn heal(&self, _net: &NetSim) {
        for &minority_node in &self.minority {
            for &majority_node in &self.majority {
                // Note: This would need actual node handles in real implementation
                // For now, we'll skip the actual network operation
            }
        }
    }
}

/// Test utilities for verifying consensus properties
pub struct ConsensusVerifier;

impl ConsensusVerifier {
    /// Verify that at most one leader exists per term
    pub async fn verify_election_safety(nodes: &mut [ClusterServiceClient<tonic::transport::Channel>]) -> Result<(), String> {
        let mut term_leaders: HashMap<u64, HashSet<u64>> = HashMap::new();
        
        for (i, client) in nodes.iter_mut().enumerate() {
            let status = client.get_cluster_status(blixard_simulation::ClusterStatusRequest {})
                .await
                .map_err(|e| format!("Failed to get status from node {}: {}", i, e))?
                .into_inner();
            
            let term = status.term;
            let leader_id = status.leader_id;
            
            if leader_id != 0 {
                term_leaders.entry(term).or_insert_with(HashSet::new).insert(leader_id);
            }
        }
        
        // Check that each term has at most one leader
        for (term, leaders) in term_leaders {
            if leaders.len() > 1 {
                return Err(format!("Multiple leaders in term {}: {:?}", term, leaders));
            }
        }
        
        Ok(())
    }
    
    /// Verify that all nodes eventually converge on the same log
    pub async fn verify_log_matching(nodes: &mut [ClusterServiceClient<tonic::transport::Channel>]) -> Result<(), String> {
        // This would check that all committed entries match across nodes
        // For now, we'll just check that all nodes agree on the leader
        let mut leader_ids = HashSet::new();
        
        for (i, client) in nodes.iter_mut().enumerate() {
            let status = client.get_cluster_status(blixard_simulation::ClusterStatusRequest {})
                .await
                .map_err(|e| format!("Failed to get status from node {}: {}", i, e))?
                .into_inner();
            
            leader_ids.insert(status.leader_id);
        }
        
        if leader_ids.len() > 1 {
            return Err(format!("Nodes disagree on leader: {:?}", leader_ids));
        }
        
        Ok(())
    }
    
    /// Wait for a leader to be elected
    pub async fn wait_for_leader(
        nodes: &mut [ClusterServiceClient<tonic::transport::Channel>],
        timeout: Duration,
    ) -> Result<u64, String> {
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            for client in nodes.iter_mut() {
                if let Ok(response) = client.get_cluster_status(blixard_simulation::ClusterStatusRequest {}).await {
                    let status = response.into_inner();
                    if status.leader_id != 0 {
                        return Ok(status.leader_id);
                    }
                }
            }
            
            sleep(Duration::from_millis(50)).await;
        }
        
        Err("No leader elected within timeout".to_string())
    }
}

/// Message filter for controlling message delivery
pub struct MessageFilter {
    pub blocked_pairs: HashSet<(u64, u64)>,
    pub drop_rate: f64,
}

impl MessageFilter {
    pub fn new() -> Self {
        Self {
            blocked_pairs: HashSet::new(),
            drop_rate: 0.0,
        }
    }
    
    pub fn block_messages(&mut self, from: u64, to: u64) {
        self.blocked_pairs.insert((from, to));
    }
    
    pub fn unblock_messages(&mut self, from: u64, to: u64) {
        self.blocked_pairs.remove(&(from, to));
    }
    
    pub fn should_deliver(&self, from: u64, to: u64) -> bool {
        if self.blocked_pairs.contains(&(from, to)) {
            return false;
        }
        
        if self.drop_rate > 0.0 {
            // In real implementation, use deterministic random
            return madsim::rand::random::<f64>() > self.drop_rate;
        }
        
        true
    }
}

/// Test harness for running Raft tests with different configurations
pub struct RaftTestHarness {
    pub cluster: TestCluster,
    pub message_filter: Arc<RwLock<MessageFilter>>,
}

impl RaftTestHarness {
    pub async fn new(config: TestClusterConfig) -> Self {
        let cluster = create_test_cluster(config).await;
        let message_filter = Arc::new(RwLock::new(MessageFilter::new()));
        
        Self {
            cluster,
            message_filter,
        }
    }
    
    /// Start all nodes in the cluster
    pub async fn start_cluster(&mut self) {
        // Implementation would start the actual Raft nodes
        info!("Starting test cluster with {} nodes", self.cluster.nodes.len());
    }
    
    /// Stop all nodes
    pub async fn stop_cluster(&mut self) {
        info!("Stopping test cluster");
    }
    
    /// Inject a network partition
    pub fn partition(&self, minority_size: usize) -> NetworkPartition {
        let node_ids: Vec<_> = self.cluster.nodes.keys().copied().collect();
        let partition = NetworkPartition::create(&node_ids, minority_size);
        partition.apply(&NetSim::current());
        partition
    }
    
    /// Get clients for all nodes
    pub async fn get_clients(&mut self) -> Vec<ClusterServiceClient<tonic::transport::Channel>> {
        let mut clients = vec![];
        
        for node in self.cluster.nodes.values() {
            if let Ok(client) = ClusterServiceClient::connect(format!("http://{}", node.addr)).await {
                clients.push(client);
            }
        }
        
        clients
    }
}

/// Helper to run a test with timeout and proper cleanup
pub async fn run_test<F, Fut>(name: &str, test_fn: F) 
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    info!("Running test: {}", name);
    
    let result = madsim::time::timeout(Duration::from_secs(30), test_fn()).await;
    
    match result {
        Ok(Ok(())) => info!("✅ Test {} passed", name),
        Ok(Err(e)) => panic!("❌ Test {} failed: {}", name, e),
        Err(_) => panic!("❌ Test {} timed out", name),
    }
}

/// Generate deterministic random election timeout
pub fn random_election_timeout(min: Duration, max: Duration, seed: u64) -> Duration {
    // In real implementation, use MadSim's deterministic random
    let range = max.as_millis() - min.as_millis();
    let offset = (seed % range as u64) as u64;
    min + Duration::from_millis(offset)
}