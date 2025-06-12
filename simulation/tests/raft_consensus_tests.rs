#![cfg(madsim)]

use madsim::time::*;
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// TODO: These tests require the blixard crate to be compatible with madsim
// For now, we'll create placeholder types to demonstrate the test structure

// Global cluster state for mocking purposes
lazy_static::lazy_static! {
    static ref CLUSTER_STATE: Arc<Mutex<ClusterState>> = Arc::new(Mutex::new(ClusterState::default()));
}

#[derive(Default)]
struct ClusterState {
    nodes: Vec<u64>,
    leader_id: u64,
    term: u64,
    vms: Vec<(VmConfig, u64)>, // (config, node_id)
    is_partitioned: bool,
    minority_nodes: Vec<u64>,
}

// Placeholder types until blixard is madsim-compatible
#[derive(Clone, Debug)]
struct NodeConfig {
    id: u64,
    data_dir: String,
    bind_addr: std::net::SocketAddr,
    join_addr: Option<std::net::SocketAddr>,
    use_tailscale: bool,
}

#[derive(Clone, Debug)]
struct VmConfig {
    name: String,
    config_path: String,
    vcpus: u32,
    memory: u64,
}

#[derive(Clone)]
struct TaskSpec {
    command: String,
    args: Vec<String>,
    resources: ResourceRequirements,
    timeout_secs: u64,
}

#[derive(Clone)]
struct ResourceRequirements {
    cpu_cores: u32,
    memory_mb: u64,
    disk_gb: u64,
    required_features: Vec<String>,
}

#[derive(Debug)]
enum VmCommand {
    Create {
        config: VmConfig,
        node_id: u64,
    },
}

// Placeholder Node implementation with basic cluster state tracking
#[derive(Debug)]
struct Node {
    config: NodeConfig,
    is_running: bool,
}

impl Node {
    fn new(config: NodeConfig) -> Self {
        Self {
            config,
            is_running: false,
        }
    }
    
    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_running = true;
        
        // Register node in cluster state
        let mut state = CLUSTER_STATE.lock().unwrap();
        if !state.nodes.contains(&self.config.id) {
            state.nodes.push(self.config.id);
        }
        // First node becomes leader
        if state.leader_id == 0 {
            state.leader_id = self.config.id;
        }
        
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_running = false;
        
        // Remove from cluster state
        let mut state = CLUSTER_STATE.lock().unwrap();
        state.nodes.retain(|&id| id != self.config.id);
        
        // If this was the leader, elect a new one
        if state.leader_id == self.config.id && !state.nodes.is_empty() {
            state.leader_id = state.nodes[0];
            state.term += 1;
        }
        
        Ok(())
    }
    
    async fn is_running(&self) -> bool {
        self.is_running
    }
    
    fn get_id(&self) -> u64 {
        self.config.id
    }
    
    fn get_bind_addr(&self) -> &std::net::SocketAddr {
        &self.config.bind_addr
    }
    
    async fn join_cluster(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    async fn get_cluster_status(&self) -> Result<(u64, Vec<u64>, u64), Box<dyn std::error::Error>> {
        let state = CLUSTER_STATE.lock().unwrap();
        Ok((state.leader_id, state.nodes.clone(), state.term))
    }
    
    async fn submit_task(&self, _task_id: &str, _task: TaskSpec) -> Result<u64, Box<dyn std::error::Error>> {
        let state = CLUSTER_STATE.lock().unwrap();
        
        // Check if we're in a partition
        if state.is_partitioned {
            // Minority partition cannot accept tasks
            if state.minority_nodes.contains(&self.config.id) {
                return Err("Not enough nodes for consensus".into());
            }
        }
        
        Ok(self.config.id)
    }
    
    async fn get_task_status(&self, _task_id: &str) -> Result<Option<(String, Option<()>)>, Box<dyn std::error::Error>> {
        Ok(Some(("pending".to_string(), None)))
    }
    
    async fn send_vm_command(&self, command: VmCommand) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            VmCommand::Create { config, node_id } => {
                let mut state = CLUSTER_STATE.lock().unwrap();
                state.vms.push((config, node_id));
            }
        }
        Ok(())
    }
    
    async fn list_vms(&self) -> Result<Vec<(VmConfig, String)>, Box<dyn std::error::Error>> {
        let state = CLUSTER_STATE.lock().unwrap();
        Ok(state.vms.iter().map(|(config, _)| (config.clone(), "running".to_string())).collect())
    }
}

// Helper to create a test node configuration
fn create_node_config(id: u64, port: u16) -> NodeConfig {
    NodeConfig {
        id,
        data_dir: format!("/tmp/blixard-test-{}", id),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    }
}

// Helper to reset cluster state between tests
fn reset_cluster_state() {
    let mut state = CLUSTER_STATE.lock().unwrap();
    state.nodes.clear();
    state.leader_id = 0;
    state.term = 0;
    state.vms.clear();
    state.is_partitioned = false;
    state.minority_nodes.clear();
}

#[madsim::test]
async fn test_single_node_bootstrap() {
    reset_cluster_state();
    
    let config = create_node_config(1, 7001);
    let mut node = Node::new(config);
    
    // Initialize and start the node
    node.initialize().await.unwrap();
    node.start().await.unwrap();
    
    // Give the node time to elect itself as leader
    sleep(Duration::from_secs(2)).await;
    
    // Verify node is running
    assert!(node.is_running().await);
    
    // Clean up
    node.stop().await.unwrap();
}

#[madsim::test]
async fn test_three_node_leader_election() {
    reset_cluster_state();
    
    // Create three nodes
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let mut nodes = Vec::new();
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Nodes join the cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    // Wait for leader election
    sleep(Duration::from_secs(3)).await;
    
    // Check cluster status from each node
    let mut leaders = HashMap::new();
    
    for (i, node) in nodes.iter().enumerate() {
        let (leader_id, node_ids, _term) = node.get_cluster_status().await.unwrap();
        
        // All nodes should see the same leader
        leaders.insert(i, leader_id);
        
        // Should have 3 nodes in cluster
        assert_eq!(node_ids.len(), 3);
    }
    
    // All nodes should agree on the same leader
    let leader_values: Vec<_> = leaders.values().collect();
    assert!(leader_values.windows(2).all(|w| w[0] == w[1]));
    
    // Clean up
    for mut node in nodes {
        node.stop().await.unwrap();
    }
}

#[madsim::test]
async fn test_task_assignment_and_execution() {
    reset_cluster_state();
    
    // Create a 3-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let mut nodes = Vec::new();
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(Arc::new(node));
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        Arc::get_mut(&mut nodes[i]).unwrap()
            .join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Submit a task
    let task = TaskSpec {
        command: "echo".to_string(),
        args: vec!["Hello from task".to_string()],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 512,
            disk_gb: 1,
            required_features: vec![],
        },
        timeout_secs: 10,
    };
    
    let task_id = "test-task-1";
    let assigned_node = nodes[0].submit_task(task_id, task).await.unwrap();
    
    // Verify task was assigned
    assert!(assigned_node > 0);
    
    // Check task status
    let status = nodes[0].get_task_status(task_id).await.unwrap();
    assert!(status.is_some());
    
    // Clean up
    for node in nodes {
        match Arc::try_unwrap(node) {
            Ok(mut n) => n.stop().await.unwrap(),
            Err(_) => eprintln!("Failed to unwrap Arc for cleanup"),
        }
    }
}

#[madsim::test]
async fn test_leader_failover() {
    reset_cluster_state();
    
    // Create a 5-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
        create_node_config(4, 7004),
        create_node_config(5, 7005),
    ];
    
    let mut nodes = Vec::new();
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(3)).await;
    
    // Find the current leader
    let (initial_leader, _, _) = nodes[0].get_cluster_status().await.unwrap();
    
    // Stop the leader node
    let leader_index = nodes.iter().position(|n| n.get_id() == initial_leader).unwrap();
    nodes[leader_index].stop().await.unwrap();
    
    // Wait for new leader election
    sleep(Duration::from_secs(5)).await;
    
    // Check that a new leader was elected
    let remaining_node_index = if leader_index == 0 { 1 } else { 0 };
    let (new_leader, _, _) = nodes[remaining_node_index].get_cluster_status().await.unwrap();
    
    assert_ne!(new_leader, initial_leader);
    assert_ne!(new_leader, 0);
    
    // Clean up
    for mut node in nodes {
        let _ = node.stop().await;
    }
}

#[madsim::test]
async fn test_network_partition_recovery() {
    reset_cluster_state();
    
    // Create a 5-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
        create_node_config(4, 7004),
        create_node_config(5, 7005),
    ];
    
    let mut nodes = Vec::new();
    
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(3)).await;
    
    // Create network partition: nodes 1,2 vs nodes 3,4,5
    {
        let mut state = CLUSTER_STATE.lock().unwrap();
        state.is_partitioned = true;
        state.minority_nodes = vec![1, 2]; // Nodes 1 and 2 are in minority
    }
    
    // Wait for partition to take effect
    sleep(Duration::from_secs(5)).await;
    
    // Check that majority partition still works
    let task = TaskSpec {
        command: "test".to_string(),
        args: vec![],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 256,
            disk_gb: 1,
            required_features: vec![],
        },
        timeout_secs: 10,
    };
    
    // This should succeed in the majority partition
    let result = nodes[2].submit_task("partition-test", task.clone()).await;
    assert!(result.is_ok());
    
    // This should fail in the minority partition
    let result = nodes[0].submit_task("partition-test-2", task).await;
    assert!(result.is_err());
    
    // Heal the partition
    {
        let mut state = CLUSTER_STATE.lock().unwrap();
        state.is_partitioned = false;
        state.minority_nodes.clear();
    }
    
    // Wait for recovery
    sleep(Duration::from_secs(5)).await;
    
    // Verify cluster is healed
    let (_, node_ids, _) = nodes[0].get_cluster_status().await.unwrap();
    assert_eq!(node_ids.len(), 5);
    
    // Clean up
    for mut node in nodes {
        let _ = node.stop().await;
    }
}

#[madsim::test]
async fn test_concurrent_task_submission() {
    use futures::future::join_all;
    
    reset_cluster_state();
    
    // Create a 3-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let mut nodes = Vec::new();
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(Arc::new(node));
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        Arc::get_mut(&mut nodes[i]).unwrap()
            .join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Submit multiple tasks concurrently
    let mut tasks = Vec::new();
    
    for i in 0..10 {
        let node = nodes[i % nodes.len()].clone();
        let task_future = async move {
            let task = TaskSpec {
                command: format!("task-{}", i),
                args: vec![],
                resources: ResourceRequirements {
                    cpu_cores: 1,
                    memory_mb: 256,
                    disk_gb: 1,
                    required_features: vec![],
                },
                timeout_secs: 10,
            };
            
            node.submit_task(&format!("concurrent-task-{}", i), task).await
        };
        tasks.push(task_future);
    }
    
    // Collect results
    let results = join_all(tasks).await;
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    
    // All tasks should succeed
    assert_eq!(success_count, 10);
    
    // Clean up
    for node in nodes {
        match Arc::try_unwrap(node) {
            Ok(mut n) => n.stop().await.unwrap(),
            Err(_) => eprintln!("Failed to unwrap Arc for cleanup"),
        }
    }
}

#[madsim::test]
async fn test_vm_state_replication() {
    reset_cluster_state();
    
    // Create a 3-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let mut nodes = Vec::new();
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Create a VM on node 1
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/path/to/config".to_string(),
        vcpus: 2,
        memory: 1024,
    };
    
    nodes[0].send_vm_command(VmCommand::Create {
        config: vm_config.clone(),
        node_id: nodes[0].get_id(),
    }).await.unwrap();
    
    // Wait for replication
    sleep(Duration::from_millis(500)).await;
    
    // Verify VM is visible from all nodes
    for node in &nodes {
        let vms = node.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].0.name, "test-vm");
    }
    
    // Clean up
    for mut node in nodes {
        node.stop().await.unwrap();
    }
}

#[madsim::test]
async fn test_packet_loss_resilience() {
    reset_cluster_state();
    
    // Create a 3-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let mut nodes = Vec::new();
    
    for config in configs {
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = *nodes[0].get_bind_addr();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Note: packet loss simulation would need proper MadSim setup
    // This is a placeholder for now
    
    // Try to submit tasks despite simulated packet loss
    let mut successful = 0;
    for i in 0..5 {
        let task = TaskSpec {
            command: format!("lossy-task-{}", i),
            args: vec![],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 10,
        };
        
        if nodes[0].submit_task(&format!("lossy-{}", i), task).await.is_ok() {
            successful += 1;
        }
    }
    
    // Despite packet loss, most tasks should succeed
    assert!(successful >= 3);
    
    // Clean up
    for mut node in nodes {
        let _ = node.stop().await;
    }
}