//! Large cluster tests (10+ nodes)
#![cfg(feature = "test-helpers")]

use blixard_core::{
    test_helpers::TestCluster,
    test_helpers_concurrent::{ConcurrentTestCluster, ConcurrentTestClusterBuilder},
    proto::ClusterStatusRequest,
    types::{VmConfig, VmCommand},
};
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[tokio::test]
async fn test_10_node_cluster_formation() {
    let _ = blixard_core::metrics_otel_v2::init_noop();
    
    println!("Creating 10-node cluster...");
    let start = Instant::now();
    
    let cluster = TestCluster::builder()
        .with_nodes(10)
        .with_convergence_timeout(Duration::from_secs(60)) // Longer timeout for large cluster
        .build()
        .await
        .expect("Failed to create 10-node cluster");
    
    let formation_time = start.elapsed();
    println!("10-node cluster formed in {:?}", formation_time);
    
    // Verify all nodes are present
    let leader_client = cluster.leader_client().await
        .expect("Should have a leader");
    
    let status = leader_client.clone()
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .expect("Failed to get status")
        .into_inner();
    
    println!("Cluster state - Leader: {}, Total nodes: {}", 
        status.leader_id, status.nodes.len());
    
    assert_eq!(status.nodes.len(), 10, "Should have 10 nodes");
    
    // Check that all nodes agree on the leader
    let mut leader_agreements = HashMap::new();
    for node in cluster.nodes().values() {
        if let Ok(raft_status) = node.shared_state.get_raft_status().await {
            if let Some(leader_id) = raft_status.leader_id {
                *leader_agreements.entry(leader_id).or_insert(0) += 1;
            }
        }
    }
    
    println!("Leader agreement: {:?}", leader_agreements);
    assert_eq!(leader_agreements.len(), 1, "All nodes should agree on one leader");
    assert_eq!(*leader_agreements.values().next().unwrap(), 10, 
        "All 10 nodes should agree on the leader");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_incremental_growth_to_20_nodes() {
    let _ = blixard_core::metrics_otel_v2::init_noop();
    
    println!("Starting with 5-node cluster...");
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create initial cluster");
    
    // Grow cluster incrementally
    let growth_batches = vec![5, 5, 5]; // Add 5 nodes at a time
    let mut total_nodes = 5;
    
    for (batch_num, batch_size) in growth_batches.iter().enumerate() {
        println!("\nAdding batch {} ({} nodes)...", batch_num + 1, batch_size);
        let batch_start = Instant::now();
        
        let mut join_results = Vec::new();
        for i in 0..*batch_size {
            let join_start = Instant::now();
            match cluster.add_node().await {
                Ok(node_id) => {
                    let join_time = join_start.elapsed();
                    println!("  Node {} joined in {:?}", node_id, join_time);
                    join_results.push((node_id, true, join_time));
                }
                Err(e) => {
                    let join_time = join_start.elapsed();
                    println!("  Failed to add node after {:?}: {:?}", join_time, e);
                    join_results.push((0, false, join_time));
                }
            }
            
            // Small delay between joins to avoid overwhelming the cluster
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        let batch_time = batch_start.elapsed();
        let successful_joins = join_results.iter().filter(|(_, success, _)| *success).count();
        
        println!("Batch {} complete in {:?} - {}/{} nodes joined successfully", 
            batch_num + 1, batch_time, successful_joins, batch_size);
        
        total_nodes += successful_joins;
        
        // Wait for cluster to stabilize after batch
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    
    // Verify final cluster size
    let leader_client = cluster.leader_client().await
        .expect("Should have a leader");
    
    let status = leader_client.clone()
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .expect("Failed to get status")
        .into_inner();
    
    println!("\nFinal cluster state - Leader: {}, Total nodes: {} (expected: {})", 
        status.leader_id, status.nodes.len(), total_nodes);
    
    assert!(status.nodes.len() >= 15, "Should have at least 15 nodes");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_large_cluster_concurrent_operations() {
    let _ = blixard_core::metrics_otel_v2::init_noop();
    
    println!("Creating concurrent 15-node cluster...");
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(15)
        .with_timeout(Duration::from_secs(60))
        .build()
        .await
        .expect("Failed to create large cluster");
    
    println!("Running concurrent operations on 15 nodes...");
    
    // Launch concurrent operations on all nodes
    let node_ids = cluster.node_ids().await;
    let mut handles = Vec::new();
    
    let operations_per_node = 3;
    let start = Instant::now();
    
    for &node_id in &node_ids {
        let cluster_clone = cluster.clone();
        let handle = tokio::spawn(async move {
            let mut results = Vec::new();
            
            for i in 0..operations_per_node {
                let vm_name = format!("large-cluster-vm-{}-{}", node_id, i);
                let vm_config = VmConfig {
                    name: vm_name.clone(),
                    config_path: format!("/tmp/{}.nix", vm_name),
                    vcpus: 1,
                    memory: 128, // Small VMs for testing
                };
                let vm_command = VmCommand::Create {
                    config: vm_config,
                    node_id,
                };
                
                let op_start = Instant::now();
                let result = match cluster_clone.get_node_shared_state(node_id).await {
                    Ok(shared_state) => {
                        shared_state.create_vm_through_raft(vm_command).await
                    }
                    Err(e) => Err(e),
                };
                let op_time = op_start.elapsed();
                
                results.push((vm_name, result.is_ok(), op_time));
            }
            
            (node_id, results)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut total_operations = 0;
    let mut successful_operations = 0;
    let mut operation_times = Vec::new();
    
    for handle in handles {
        match handle.await {
            Ok((node_id, results)) => {
                for (vm_name, success, duration) in results {
                    total_operations += 1;
                    if success {
                        successful_operations += 1;
                        println!("✓ Node {} created {}", node_id, vm_name);
                    } else {
                        println!("✗ Node {} failed to create {}", node_id, vm_name);
                    }
                    operation_times.push(duration);
                }
            }
            Err(e) => {
                println!("Task panicked: {:?}", e);
            }
        }
    }
    
    let total_time = start.elapsed();
    
    // Calculate statistics
    let avg_op_time = operation_times.iter()
        .map(|d| d.as_millis())
        .sum::<u128>() / operation_times.len() as u128;
    
    let max_op_time = operation_times.iter()
        .map(|d| d.as_millis())
        .max()
        .unwrap_or(0);
    
    println!("\nLarge cluster concurrent operations summary:");
    println!("- Total operations: {}", total_operations);
    println!("- Successful: {} ({:.1}%)", 
        successful_operations, 
        (successful_operations as f64 / total_operations as f64) * 100.0);
    println!("- Total time: {:?}", total_time);
    println!("- Average operation time: {}ms", avg_op_time);
    println!("- Max operation time: {}ms", max_op_time);
    println!("- Operations per second: {:.1}", 
        total_operations as f64 / total_time.as_secs_f64());
    
    // Most operations should succeed
    assert!(successful_operations as f64 / total_operations as f64 > 0.8,
        "At least 80% of operations should succeed in large cluster");
    
    cluster.shutdown().await;
}

#[tokio::test]
#[ignore = "Stress test - takes a long time"]
async fn test_50_node_cluster_stress() {
    let _ = blixard_core::metrics_otel_v2::init_noop();
    
    println!("Creating 50-node cluster (this will take a while)...");
    let start = Instant::now();
    
    let cluster = TestCluster::builder()
        .with_nodes(50)
        .with_convergence_timeout(Duration::from_secs(300)) // 5 minutes for 50 nodes
        .build()
        .await
        .expect("Failed to create 50-node cluster");
    
    let formation_time = start.elapsed();
    println!("50-node cluster formed in {:?}", formation_time);
    
    // Basic verification
    let leader_client = cluster.leader_client().await
        .expect("Should have a leader");
    
    let status = leader_client.clone()
        .get_cluster_status(ClusterStatusRequest {})
        .await
        .expect("Failed to get status")
        .into_inner();
    
    println!("50-node cluster - Leader: {}, Total nodes: {}", 
        status.leader_id, status.nodes.len());
    
    assert_eq!(status.nodes.len(), 50, "Should have 50 nodes");
    
    // Measure consensus performance with large cluster
    println!("Testing consensus performance...");
    let consensus_start = Instant::now();
    
    for i in 0..10 {
        let vm_config = VmConfig {
            name: format!("stress-test-vm-{}", i),
            config_path: format!("/tmp/stress-{}.nix", i),
            vcpus: 1,
            memory: 64,
        };
        
        let request = blixard_core::proto::CreateVmRequest {
            name: vm_config.name.clone(),
            config_path: vm_config.config_path,
            vcpus: vm_config.vcpus,
            memory_mb: vm_config.memory,
        };
        
        let op_start = Instant::now();
        match leader_client.clone().create_vm(request).await {
            Ok(_) => {
                let op_time = op_start.elapsed();
                println!("VM {} created in {:?}", i, op_time);
            }
            Err(e) => {
                println!("Failed to create VM {}: {:?}", i, e);
            }
        }
    }
    
    let consensus_time = consensus_start.elapsed();
    println!("10 consensus operations in 50-node cluster took {:?}", consensus_time);
    println!("Average: {:?} per operation", consensus_time / 10);
    
    cluster.shutdown().await;
}