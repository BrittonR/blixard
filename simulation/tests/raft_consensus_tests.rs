#![cfg(madsim)]

use madsim::{net::*, runtime::*, time::*};
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use bytes::Bytes;

use blixard::{
    node::Node,
    types::{NodeConfig, VmConfig},
    raft_manager::{TaskSpec, ResourceRequirements},
};

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

#[madsim::test]
async fn test_single_node_bootstrap() {
    let config = create_node_config(1, 7001);
    let mut node = Node::new(config);
    
    // Initialize and start the node
    node.initialize().await.unwrap();
    node.start().await.unwrap();
    
    // Give the node time to elect itself as leader
    sleep(Duration::from_secs(2)).await;
    
    // Verify node is running
    assert!(node.is_running());
    
    // Clean up
    node.stop().await.unwrap();
}

#[madsim::test]
async fn test_three_node_leader_election() {
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
        let join_addr = nodes[0].get_bind_addr().clone();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    // Wait for leader election
    sleep(Duration::from_secs(3)).await;
    
    // Check cluster status from each node
    let mut leader_count = 0;
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
        let join_addr = nodes[0].get_bind_addr().clone();
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
        Arc::try_unwrap(node).unwrap().stop().await.unwrap();
    }
}

#[madsim::test]
async fn test_leader_failover() {
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
        let join_addr = nodes[0].get_bind_addr().clone();
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
    // Create a 5-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
        create_node_config(4, 7004),
        create_node_config(5, 7005),
    ];
    
    let net = NetSim::current();
    
    let mut nodes = Vec::new();
    let mut endpoints = Vec::new();
    
    for config in configs {
        let ep = net.endpoint(config.bind_addr);
        endpoints.push(ep.clone());
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = nodes[0].get_bind_addr().clone();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(3)).await;
    
    // Create network partition: nodes 1,2 vs nodes 3,4,5
    // The majority (3,4,5) should maintain a leader
    net.disconnect_peer(endpoints[0].addr(), endpoints[2].addr());
    net.disconnect_peer(endpoints[0].addr(), endpoints[3].addr());
    net.disconnect_peer(endpoints[0].addr(), endpoints[4].addr());
    net.disconnect_peer(endpoints[1].addr(), endpoints[2].addr());
    net.disconnect_peer(endpoints[1].addr(), endpoints[3].addr());
    net.disconnect_peer(endpoints[1].addr(), endpoints[4].addr());
    
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
    net.connect_peer(endpoints[0].addr(), endpoints[2].addr());
    net.connect_peer(endpoints[0].addr(), endpoints[3].addr());
    net.connect_peer(endpoints[0].addr(), endpoints[4].addr());
    net.connect_peer(endpoints[1].addr(), endpoints[2].addr());
    net.connect_peer(endpoints[1].addr(), endpoints[3].addr());
    net.connect_peer(endpoints[1].addr(), endpoints[4].addr());
    
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
    use tokio::task::JoinSet;
    
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
        let join_addr = nodes[0].get_bind_addr().clone();
        Arc::get_mut(&mut nodes[i]).unwrap()
            .join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Submit multiple tasks concurrently
    let mut tasks = JoinSet::new();
    
    for i in 0..10 {
        let node = nodes[i % nodes.len()].clone();
        tasks.spawn(async move {
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
        });
    }
    
    // Collect results
    let mut success_count = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap().is_ok() {
            success_count += 1;
        }
    }
    
    // All tasks should succeed
    assert_eq!(success_count, 10);
    
    // Clean up
    for node in nodes {
        Arc::try_unwrap(node).unwrap().stop().await.unwrap();
    }
}

#[madsim::test]
async fn test_vm_state_replication() {
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
        let join_addr = nodes[0].get_bind_addr().clone();
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
    
    nodes[0].send_vm_command(blixard::types::VmCommand::Create {
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
    // Create a 3-node cluster
    let configs = vec![
        create_node_config(1, 7001),
        create_node_config(2, 7002),
        create_node_config(3, 7003),
    ];
    
    let net = NetSim::current();
    
    let mut nodes = Vec::new();
    let mut endpoints = Vec::new();
    
    for config in configs {
        let ep = net.endpoint(config.bind_addr);
        endpoints.push(ep.clone());
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        nodes.push(node);
    }
    
    // Form cluster
    for i in 1..nodes.len() {
        let join_addr = nodes[0].get_bind_addr().clone();
        nodes[i].join_cluster(Some(join_addr)).await.unwrap();
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Set 20% packet loss between all nodes
    for i in 0..endpoints.len() {
        for j in 0..endpoints.len() {
            if i != j {
                net.set_packet_loss(endpoints[i].addr(), endpoints[j].addr(), 0.2);
            }
        }
    }
    
    // Try to submit tasks despite packet loss
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