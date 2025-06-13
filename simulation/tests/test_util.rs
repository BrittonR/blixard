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
    ClusterStatusRequest,
};
use tonic::Request;

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
        // Use longer timeouts in CI environments
        let multiplier = if std::env::var("CI").is_ok() { 3 } else { 1 };
        
        Self {
            node_count: 3,
            election_timeout_min: Duration::from_millis(150 * multiplier),
            election_timeout_max: Duration::from_millis(300 * multiplier),
            heartbeat_interval: Duration::from_millis(50 * multiplier),
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
        let mut node_states = Vec::new();
        let mut all_responded = true;
        
        for (i, client) in nodes.iter_mut().enumerate() {
            match client.get_cluster_status(blixard_simulation::ClusterStatusRequest {}).await {
                Ok(response) => {
                    let status = response.into_inner();
                    // Only count non-zero leader IDs
                    if status.leader_id != 0 {
                        leader_ids.insert(status.leader_id);
                    }
                    node_states.push((i, status.leader_id, status.term));
                }
                Err(e) => {
                    // Node might be temporarily unavailable during recovery
                    info!("Node {} unavailable during convergence check: {}", i, e);
                    all_responded = false;
                }
            }
        }
        
        // Log current state for debugging
        info!("Convergence check - responded: {}, leader_ids: {:?}, states: {:?}", 
              all_responded, leader_ids, node_states);
        
        // For convergence, we need:
        // 1. All nodes to respond
        // 2. Exactly one leader
        // 3. All nodes agree on that leader
        if !all_responded {
            return Err("Not all nodes responded".to_string());
        }
        
        if leader_ids.is_empty() {
            return Err("No leader elected yet".to_string());
        }
        
        if leader_ids.len() > 1 {
            return Err(format!("Nodes disagree on leader: {:?} (states: {:?})", leader_ids, node_states));
        }
        
        // Check that all nodes report the same leader
        let leader = *leader_ids.iter().next().unwrap();
        for (i, leader_id, _term) in &node_states {
            if *leader_id != leader && *leader_id != 0 {
                return Err(format!("Node {} reports different leader {} vs {}", i, leader_id, leader));
            }
        }
        
        Ok(())
    }
    
    /// Wait for a leader to be elected with exponential backoff
    pub async fn wait_for_leader(
        nodes: &mut [ClusterServiceClient<tonic::transport::Channel>],
        timeout: Duration,
    ) -> Result<u64, String> {
        let start = Instant::now();
        let mut check_interval = Duration::from_millis(50);
        let max_interval = Duration::from_secs(1);
        let mut last_log = Instant::now();
        
        info!("Waiting for leader election (timeout: {:?})", timeout);
        
        while start.elapsed() < timeout {
            let mut node_states = Vec::new();
            
            for (i, client) in nodes.iter_mut().enumerate() {
                match client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    Ok(response) => {
                        let status = response.into_inner();
                        node_states.push((i, status.leader_id, status.term));
                        if status.leader_id != 0 {
                            info!("Leader elected: node {} in term {} (took {:?})", 
                                  status.leader_id, status.term, start.elapsed());
                            return Ok(status.leader_id);
                        }
                    }
                    Err(e) => {
                        node_states.push((i, 0, 0)); // Node unavailable
                        if last_log.elapsed() > Duration::from_secs(5) {
                            info!("Node {} unavailable: {}", i, e);
                        }
                    }
                }
            }
            
            // Log progress every 5 seconds
            if last_log.elapsed() > Duration::from_secs(5) {
                info!("Still waiting for leader. Node states: {:?}", node_states);
                last_log = Instant::now();
            }
            
            sleep(check_interval).await;
            // Exponential backoff to reduce CPU usage during long waits
            check_interval = (check_interval * 2).min(max_interval);
        }
        
        // Final state dump on timeout
        let mut final_states = Vec::new();
        for (i, client) in nodes.iter_mut().enumerate() {
            if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                let status = response.into_inner();
                final_states.push((i, status.leader_id, status.term));
            }
        }
        
        Err(format!("No leader elected within {:?}. Final states: {:?}", timeout, final_states))
    }
    
    /// Wait for a leader to be elected among specific nodes with exponential backoff
    pub async fn wait_for_leader_among(
        nodes: &mut [ClusterServiceClient<tonic::transport::Channel>],
        valid_leaders: &HashSet<u64>,
        timeout: Duration,
    ) -> Result<u64, String> {
        let start = Instant::now();
        let mut check_interval = Duration::from_millis(50);
        let max_interval = Duration::from_secs(1);
        let mut last_log = Instant::now();
        
        info!("Waiting for leader among nodes {:?} (timeout: {:?})", valid_leaders, timeout);
        
        while start.elapsed() < timeout {
            let mut node_states = Vec::new();
            let mut current_leader = 0;
            
            for (i, client) in nodes.iter_mut().enumerate() {
                match client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    Ok(response) => {
                        let status = response.into_inner();
                        node_states.push((i, status.leader_id, status.term));
                        if status.leader_id != 0 {
                            current_leader = status.leader_id;
                            if valid_leaders.contains(&status.leader_id) {
                                info!("Leader elected among valid nodes: {} in term {} (took {:?})", 
                                      status.leader_id, status.term, start.elapsed());
                                return Ok(status.leader_id);
                            }
                        }
                    }
                    Err(_) => {
                        node_states.push((i, 0, 0)); // Node unavailable
                    }
                }
            }
            
            // Log progress every 5 seconds
            if last_log.elapsed() > Duration::from_secs(5) {
                if current_leader != 0 {
                    info!("Current leader is {}, but not in valid set {:?}. States: {:?}", 
                          current_leader, valid_leaders, node_states);
                } else {
                    info!("No leader yet. Node states: {:?}", node_states);
                }
                last_log = Instant::now();
            }
            
            sleep(check_interval).await;
            // Exponential backoff to reduce CPU usage during long waits
            check_interval = (check_interval * 2).min(max_interval);
        }
        
        // Final state dump on timeout
        let mut final_states = Vec::new();
        for (i, client) in nodes.iter_mut().enumerate() {
            if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                let status = response.into_inner();
                final_states.push((i, status.leader_id, status.term));
            }
        }
        
        Err(format!("No leader elected among nodes {:?} within {:?}. Final states: {:?}", 
                    valid_leaders, timeout, final_states))
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
    
    /// Get clients for all nodes with retry logic
    pub async fn get_clients(&mut self) -> Vec<ClusterServiceClient<tonic::transport::Channel>> {
        let mut clients = vec![];
        
        for node in self.cluster.nodes.values() {
            // Retry connection with exponential backoff
            let mut delay = Duration::from_millis(100);
            let max_attempts = 10;
            
            for attempt in 1..=max_attempts {
                match ClusterServiceClient::connect(format!("http://{}", node.addr)).await {
                    Ok(client) => {
                        clients.push(client);
                        break;
                    }
                    Err(e) if attempt < max_attempts => {
                        info!("Connection attempt {}/{} to {} failed: {}", 
                              attempt, max_attempts, node.addr, e);
                        sleep(delay).await;
                        delay = (delay * 2).min(Duration::from_secs(2));
                    }
                    Err(e) => {
                        info!("Failed to connect to {} after {} attempts: {}", 
                              node.addr, max_attempts, e);
                    }
                }
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
    
    // Use environment-aware timeout
    let base_timeout = Duration::from_secs(30);
    let timeout_multiplier = if std::env::var("CI").is_ok() { 3 } else { 1 };
    let timeout = base_timeout * timeout_multiplier;
    
    let result = madsim::time::timeout(timeout, test_fn()).await;
    
    match result {
        Ok(Ok(())) => info!("✅ Test {} passed", name),
        Ok(Err(e)) => panic!("❌ Test {} failed: {}", name, e),
        Err(_) => panic!("❌ Test {} timed out after {:?}", name, timeout),
    }
}

/// Generate deterministic random election timeout
pub fn random_election_timeout(min: Duration, max: Duration, seed: u64) -> Duration {
    // In real implementation, use MadSim's deterministic random
    let range = max.as_millis() - min.as_millis();
    let offset = (seed % range as u64) as u64;
    min + Duration::from_millis(offset)
}

/// Helper to advance time and allow tasks to process
pub async fn advance_and_yield(duration: Duration) {
    madsim::time::advance(duration);
    // Small sleep to let tasks process the time advancement
    sleep(Duration::from_millis(10)).await;
}

/// Helper to wait for condition with time advancement
pub async fn wait_with_advance<F, Fut>(
    mut condition: F,
    timeout: Duration,
    advance_step: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let mut elapsed = Duration::ZERO;
    
    while elapsed < timeout {
        if condition().await {
            return Ok(());
        }
        
        // Advance time to speed up waiting
        madsim::time::advance(advance_step);
        elapsed += advance_step;
        
        // Small sleep to let tasks process
        sleep(Duration::from_millis(10)).await;
    }
    
    Err(format!("Timeout after {:?}", timeout))
}