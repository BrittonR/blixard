//! Tests that require concurrent access to the test cluster
#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers_concurrent::{ConcurrentTestCluster, ConcurrentTestClusterBuilder};
use std::time::Duration;

#[tokio::test]
async fn test_leader_change_during_join() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    // Start with 3 nodes to allow leader changes
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create 3-node cluster");
    
    println!("Testing join during leader change...");
    
    // Find current leader
    let initial_leader_id = {
        let leader_client = cluster.leader_client().await
            .expect("Should have a leader");
        
        // Get cluster status to find leader ID
        use blixard_core::iroh_types::ClusterStatusRequest;
        let status = leader_client.clone()
            .get_cluster_status(ClusterStatusRequest {})
            .await
            .expect("Failed to get status")
            .into_inner();
        
        status.leader_id
    };
    
    println!("Initial leader: Node {}", initial_leader_id);
    
    // Start join process in a separate task
    let cluster_clone = cluster.clone();
    let join_handle = tokio::spawn(async move {
        println!("Starting join process...");
        // Add a small delay to ensure we're joining during potential disruption
        tokio::time::sleep(Duration::from_millis(200)).await;
        cluster_clone.add_node().await
    });
    
    // Give join a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Simulate leader disruption by creating heavy load on the leader
    println!("Creating load on leader node {} to potentially trigger election...", initial_leader_id);
    let cluster_for_load = cluster.clone();
    let load_handle = tokio::spawn(async move {
        for i in 0..10 {
            let vm_config = blixard_core::types::VmConfig {
                name: format!("disruption-vm-{}", i),
                config_path: format!("/tmp/disruption-vm-{}.nix", i),
                vcpus: 1,
                memory: 512,
                tenant_id: "default".to_string(),
                ip_address: None,
            };
            let vm_command = blixard_core::types::VmCommand::Create {
                config: vm_config,
                node_id: initial_leader_id,
            };
            
            // Try to create VM through leader
            if let Ok(shared_state) = cluster_for_load.get_node_shared_state(initial_leader_id).await {
                let _ = shared_state.create_vm_through_raft(vm_command).await;
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    
    // Wait for both operations to complete
    let (join_result, _) = tokio::join!(join_handle, load_handle);
    
    match join_result {
        Ok(Ok(node_id)) => {
            println!("✓ Node {} joined successfully despite potential leader change", node_id);
        }
        Ok(Err(e)) => {
            println!("✗ Join failed during leader change: {:?}", e);
            panic!("Join should succeed even during leader changes");
        }
        Err(e) => {
            println!("✗ Join task panicked: {:?}", e);
            panic!("Join task should not panic");
        }
    }
    
    // Verify cluster recovered
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check final state
    let mut final_leader_client = cluster.leader_client().await
        .expect("Should have a leader after recovery");
    
    use blixard_core::iroh_types::ClusterStatusRequest;
    let final_status = final_leader_client
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .expect("Failed to get final status")
        .into_inner();
    
    println!("Final state - Leader: {}, Nodes: {} total", 
        final_status.leader_id, final_status.nodes.len());
    
    // Should have 4 nodes now (3 initial + 1 joined)
    assert_eq!(final_status.nodes.len(), 4, "Should have 4 nodes after join");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_operations_on_different_nodes() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    println!("Testing concurrent operations on different nodes...");
    
    let node_ids = cluster.node_ids().await;
    assert_eq!(node_ids.len(), 3);
    
    // Start concurrent operations on different nodes
    let mut handles = vec![];
    
    for (i, &node_id) in node_ids.iter().enumerate() {
        let cluster_clone = cluster.clone();
        let handle = tokio::spawn(async move {
            let mut results = vec![];
            
            for j in 0..5 {
                let vm_name = format!("node{}-vm{}", node_id, j);
                let vm_config = blixard_core::types::VmConfig {
                    name: vm_name.clone(),
                    config_path: format!("/tmp/{}.nix", vm_name),
                    vcpus: 1,
                    memory: 256,
                    tenant_id: "default".to_string(),
                    ip_address: None,
                };
                let vm_command = blixard_core::types::VmCommand::Create {
                    config: vm_config,
                    node_id,
                };
                
                let result = match cluster_clone.get_node_shared_state(node_id).await {
                    Ok(shared_state) => {
                        shared_state.create_vm_through_raft(vm_command).await.map(|_| ())
                    }
                    Err(e) => Err(e),
                };
                
                results.push((vm_name, result));
                
                // Small delay between operations
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            
            (node_id, results)
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut total_success = 0;
    let mut total_failure = 0;
    
    for handle in handles {
        match handle.await {
            Ok((node_id, results)) => {
                for (vm_name, result) in results {
                    match result {
                        Ok(_) => {
                            println!("✓ Node {} created VM: {}", node_id, vm_name);
                            total_success += 1;
                        }
                        Err(e) => {
                            println!("✗ Node {} failed to create VM {}: {:?}", node_id, vm_name, e);
                            total_failure += 1;
                        }
                    }
                }
            }
            Err(e) => {
                println!("✗ Task panicked: {:?}", e);
                total_failure += 5; // Count all operations as failed
            }
        }
    }
    
    println!("\nConcurrent operations completed: {} success, {} failure", 
        total_success, total_failure);
    
    // Most operations should succeed
    assert!(total_success > 10, "At least 10 operations should succeed");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_cluster_joins() {
    // Initialize metrics for test
    let _ = blixard_core::metrics_otel::init_noop();
    
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create cluster");
    
    println!("Testing truly concurrent node joins...");
    
    // Spawn multiple join operations concurrently
    let mut join_handles = vec![];
    
    for i in 0..3 {
        let cluster_clone = cluster.clone();
        let handle = tokio::spawn(async move {
            println!("Join task {} starting...", i);
            let start = std::time::Instant::now();
            let result = cluster_clone.add_node().await;
            let duration = start.elapsed();
            (i, result, duration)
        });
        join_handles.push(handle);
    }
    
    // Wait for all joins to complete
    let mut successes = 0;
    let mut failures = 0;
    
    for handle in join_handles {
        match handle.await {
            Ok((task_id, result, duration)) => {
                match result {
                    Ok(node_id) => {
                        println!("✓ Task {} successfully added node {} in {:?}", 
                            task_id, node_id, duration);
                        successes += 1;
                    }
                    Err(e) => {
                        println!("✗ Task {} failed after {:?}: {:?}", 
                            task_id, duration, e);
                        failures += 1;
                    }
                }
            }
            Err(e) => {
                println!("✗ Join task panicked: {:?}", e);
                failures += 1;
            }
        }
    }
    
    println!("\nConcurrent join results: {} successes, {} failures", successes, failures);
    
    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify final state
    let node_ids = cluster.node_ids().await;
    println!("Final cluster has {} nodes", node_ids.len());
    
    // We should have successfully added some nodes
    assert!(node_ids.len() >= 2, "At least one concurrent join should succeed");
    assert!(successes >= 1, "At least one concurrent join should succeed");
    
    cluster.shutdown().await;
}