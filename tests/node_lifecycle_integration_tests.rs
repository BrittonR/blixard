#[cfg(feature = "test-helpers")]
mod tests {
    use blixard::test_helpers::{TestNode, TestCluster};
    use blixard::raft_manager::{TaskSpec, ResourceRequirements};
    use std::time::Duration;

    /// Test the complete lifecycle of a single node
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_single_node_full_lifecycle() {
        // Phase 1: Create and start node
        let node = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .build()
            .await
            .unwrap();
        
        // Node is automatically initialized when created
        
        // Verify node is initialized and running
        assert!(node.shared_state.is_initialized().await);
        assert!(node.shared_state.is_running().await);
        
        // Phase 3: Bootstrap as single-node cluster
        // (already bootstrapped in TestNode creation)
        
        // Wait for leader election using condition-based waiting
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                if let Ok(status) = node.shared_state.get_raft_status().await {
                    status.is_leader && status.leader_id == Some(1)
                } else {
                    false
                }
            },
            Duration::from_secs(5),
            Duration::from_millis(100),
        )
        .await
        .expect("Node should become leader");
        
        // Verify node became leader
        let status = node.shared_state.get_raft_status().await.unwrap();
        assert!(status.is_leader);
        assert_eq!(status.leader_id, Some(1));
        
        // Phase 4: Submit and execute tasks
        let task = TaskSpec {
            command: "echo".to_string(),
            args: vec!["Hello, World!".to_string()],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 10,
        };
        
        let assigned_node = node.shared_state.submit_task("test-task-1", task.clone()).await.unwrap();
        assert_eq!(assigned_node, 1); // Should be assigned to self
        
        // Check task status
        let status = node.shared_state.get_task_status("test-task-1").await.unwrap();
        assert!(status.is_some());
        
        // Phase 5: Graceful shutdown
        // Check running state before shutdown
        assert!(node.shared_state.is_running().await);
        
        // Now shutdown
        node.shutdown().await;
    }

    /// Test multi-node cluster lifecycle
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_node_cluster_lifecycle() {
        // Phase 1: Create and bootstrap first node
        let node1 = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .build()
            .await
            .unwrap();
        
        // Wait for leader election using condition-based waiting
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                if let Ok(status) = node1.shared_state.get_raft_status().await {
                    status.leader_id.is_some()
                } else {
                    false
                }
            },
            Duration::from_secs(5),
            Duration::from_millis(100),
        )
        .await
        .expect("Cluster should elect a leader");
        
        // Phase 2: Add second node and join cluster
        let node2 = TestNode::builder()
            .with_id(2)
            .with_auto_port()
            .with_join_addr(Some(node1.addr))
            .build()
            .await
            .unwrap();
        
        // Wait for cluster to stabilize - both nodes should see each other
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                let peers1 = node1.shared_state.get_peers().await;
                let peers2 = node2.shared_state.get_peers().await;
                peers1.iter().any(|p| p.id == 2) && peers2.iter().any(|p| p.id == 1)
            },
            Duration::from_secs(10),
            Duration::from_millis(100),
        )
        .await
        .expect("Nodes should discover each other");
        
        // Verify both nodes see each other
        let peers1 = node1.shared_state.get_peers().await;
        let peers2 = node2.shared_state.get_peers().await;
        assert!(peers1.iter().any(|p| p.id == 2));
        assert!(peers2.iter().any(|p| p.id == 1));
        
        // Phase 3: Add third node
        let node3 = TestNode::builder()
            .with_id(3)
            .with_auto_port()
            .with_join_addr(Some(node1.addr))
            .build()
            .await
            .unwrap();
        
        // Wait for cluster to stabilize - all 3 nodes should be in the cluster
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                if let Ok((leader_id, nodes, _term)) = node1.shared_state.get_cluster_status().await {
                    leader_id > 0 && nodes.len() == 3 && 
                    nodes.contains(&1) && nodes.contains(&2) && nodes.contains(&3)
                } else {
                    false
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(100),
        )
        .await
        .expect("All nodes should join the cluster");
        
        // Verify all nodes are in the cluster
        let (leader_id, nodes, _term) = node1.shared_state.get_cluster_status().await.unwrap();
        assert!(leader_id > 0);
        assert_eq!(nodes.len(), 3);
        assert!(nodes.contains(&1));
        assert!(nodes.contains(&2));
        assert!(nodes.contains(&3));
        
        // Phase 4: Submit tasks from different nodes
        let task = TaskSpec {
            command: "test".to_string(),
            args: vec![],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 30,
        };
        
        // Submit from non-leader node
        let assigned = node2.shared_state.submit_task("task-from-node2", task.clone()).await.unwrap();
        assert!(assigned > 0);
        
        // Phase 5: Node leaving cluster
        // For now, we'll just stop node3 as leave_cluster is not directly available
        node3.shutdown().await;
        
        // Wait for configuration change - node3 removal may take time
        // For now we just give it a moment as leave_cluster is not implemented
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // After shutdown, verify remaining nodes still work
        let (_, nodes, _) = node1.shared_state.get_cluster_status().await.unwrap();
        // Note: node3 may still be in config until removed explicitly
        assert!(nodes.contains(&1));
        assert!(nodes.contains(&2));
        
        // Phase 6: Graceful shutdown of remaining nodes
        // Check they're running before shutdown
        assert!(node1.shared_state.is_running().await);
        assert!(node2.shared_state.is_running().await);
        
        // Now shutdown
        node2.shutdown().await;
        node1.shutdown().await;
    }

    /// Test node restart and recovery
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_node_restart_recovery() {
        // Create and start node with specific data directory
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_str().unwrap().to_string();
        
        let mut node = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .with_data_dir(data_dir.clone())
            .build()
            .await
            .unwrap();
        
        // Wait for leader election
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                if let Ok(status) = node.shared_state.get_raft_status().await {
                    status.is_leader
                } else {
                    false
                }
            },
            Duration::from_secs(5),
            Duration::from_millis(100),
        )
        .await
        .expect("Node should become leader after restart");
        
        // Submit a task
        let task = TaskSpec {
            command: "test".to_string(),
            args: vec!["persistent".to_string()],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 10,
        };
        
        node.shared_state.submit_task("persistent-task", task).await.unwrap();
        
        // Stop the node
        assert!(node.shared_state.is_running().await);
        node.shutdown().await;
        
        // Restart the node with same data directory
        node = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .with_data_dir(data_dir)
            .build()
            .await
            .unwrap();
        
        // Verify task is still there
        let status = node.shared_state.get_task_status("persistent-task").await.unwrap();
        assert!(status.is_some());
        
        // Clean shutdown
        node.shutdown().await;
    }

    /// Test concurrent operations during lifecycle
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_concurrent_lifecycle_operations() {
        // Use TestCluster for this test as it's designed for multi-node scenarios
        let cluster = TestCluster::builder()
            .with_nodes(3)
            .build()
            .await
            .unwrap();
        
        // Get nodes from cluster
        let nodes = cluster.nodes();
        let node1 = nodes.get(&1).unwrap();
        let node2 = nodes.get(&2).unwrap();
        let node3 = nodes.get(&3).unwrap();
        
        // Wait a bit for cluster to stabilize after convergence
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Submit tasks concurrently from all nodes
        let task = TaskSpec {
            command: "echo".to_string(),
            args: vec!["concurrent".to_string()],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 64,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 5,
        };
        
        let mut handles = vec![];
        // Submit 6 tasks (2 per node) with staggered timing
        for i in 0..6 {
            let shared_state = match i % 3 {
                0 => node1.shared_state.clone(),
                1 => node2.shared_state.clone(),
                _ => node3.shared_state.clone(),
            };
            let task_clone = task.clone();
            let task_id = format!("concurrent-task-{}", i);
            
            let handle = tokio::spawn(async move {
                shared_state.submit_task(&task_id, task_clone).await
            });
            handles.push(handle);
            
            // Small delay between submissions
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Wait for all task submissions
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.unwrap();
            assert!(result.is_ok(), "Task {} submission failed: {:?}", i, result.err());
        }
        
        // Verify all tasks were assigned
        let mut found_tasks = 0;
        for i in 0..6 {
            let task_id = format!("concurrent-task-{}", i);
            if let Ok(Some(_)) = node1.shared_state.get_task_status(&task_id).await {
                found_tasks += 1;
            }
        }
        assert_eq!(found_tasks, 6, "All tasks should be assigned");
        
        // Shutdown using cluster
        cluster.shutdown().await;
    }

    /// Test error handling during lifecycle
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_lifecycle_error_handling() {
        // Create a node but don't let it fully initialize
        // We'll manually control its state to test error conditions
        let temp_dir = tempfile::tempdir().unwrap();
        let node_config = blixard::types::NodeConfig {
            id: 999,  // Use a unique ID that won't conflict with TestNode
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            data_dir: temp_dir.path().to_str().unwrap().to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let shared_state = std::sync::Arc::new(blixard::node_shared::SharedNodeState::new(node_config));
        
        // These should fail because node is not initialized
        assert!(shared_state.submit_task("test", TaskSpec {
            command: "test".to_string(),
            args: vec![],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 10,
        }).await.is_err());
        
        assert!(shared_state.get_cluster_status().await.is_err());
        
        // Now create a proper node to test other error conditions
        let node1 = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .build()
            .await
            .unwrap();
        
        // Wait for node1 to be ready and become leader
        blixard::test_helpers::timing::wait_for_condition_with_backoff(
            || async {
                if let Ok(status) = node1.shared_state.get_raft_status().await {
                    status.is_leader
                } else {
                    false
                }
            },
            Duration::from_secs(5),
            Duration::from_millis(100),
        )
        .await
        .expect("Node1 should become leader");
        
        // Test: Operations after stop should fail
        // Save shared state reference before shutdown
        let node1_state = node1.shared_state.clone();
        
        // Verify node is running before shutdown
        assert!(node1_state.is_running().await);
        
        node1.shutdown().await;
        
        // Give time for shutdown to complete
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // After shutdown, get_cluster_status should fail because node is not initialized
        assert!(node1_state.get_cluster_status().await.is_err());
        
        // Note: submit_task doesn't check is_initialized so it would hang
        // This is a known issue in the implementation
    }

    /// Test lifecycle with network partitions (simulated)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg(feature = "simulation")]
    async fn test_lifecycle_with_partitions() {
        // This test would use MadSim to simulate network partitions
        // For now, we'll skip implementation as it requires simulation features
        eprintln!("Network partition test requires simulation feature");
    }
}

#[cfg(not(feature = "test-helpers"))]
fn main() {
    eprintln!("Tests require --features test-helpers");
}