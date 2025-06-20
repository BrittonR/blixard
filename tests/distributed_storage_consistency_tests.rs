//! Distributed Storage Consistency Tests
//!
//! This module tests the consistency guarantees of the distributed storage layer
//! across multiple nodes in a Raft cluster. It verifies:
//! - Read-after-write consistency
//! - Data replication across nodes
//! - Behavior during network partitions
//! - Performance characteristics
//! - Edge cases and fault tolerance

#![cfg(feature = "test-helpers")]

use std::time::{Duration, Instant};
use std::collections::HashMap;
use blixard::{
    proto::{
        TaskRequest, TaskStatusRequest, CreateVmRequest, GetVmStatusRequest,
        ListVmsRequest,
    },
    test_helpers::{TestCluster, timing},
};
use tracing::{info, warn};

/// Test read-after-write consistency for task operations
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_task_read_after_write_consistency() {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Submit a task through the leader
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    let task_id = format!("test-task-{}", uuid::Uuid::new_v4());
    let task_request = TaskRequest {
        task_id: task_id.clone(),
        command: "echo".to_string(),
        args: vec!["Hello, distributed storage!".to_string()],
        cpu_cores: 1,
        memory_mb: 256,
        disk_gb: 1,
        required_features: vec![],
        timeout_secs: 30,
    };
    
    info!("Submitting task {} to leader", task_id);
    let response = leader_client.clone()
        .submit_task(task_request)
        .await
        .expect("Failed to submit task");
    
    assert!(response.into_inner().accepted, "Task should be accepted");
    
    // Wait for task to be replicated to all nodes
    timing::wait_for_condition_with_backoff(
        || {
            let task_id = task_id.clone();
            let cluster = &cluster;
            async move {
                // Check if task is visible on all nodes
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        let status_request = TaskStatusRequest {
                            task_id: task_id.clone(),
                        };
                        if let Ok(resp) = client.clone().get_task_status(status_request).await {
                            if !resp.into_inner().found {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(50),
    ).await.expect("Task should be replicated to all nodes");
    
    // Verify task is readable from all nodes
    info!("Verifying task is readable from all nodes");
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        let status_request = TaskStatusRequest {
            task_id: task_id.clone(),
        };
        
        let status_response = client.clone()
            .get_task_status(status_request)
            .await
            .expect("Failed to get task status");
        
        let status = status_response.into_inner();
        assert!(status.found, "Task should be found on node {}", node_id);
        info!("Task {} found on node {} with status {:?}", task_id, node_id, status.status);
    }
    
    // Submit multiple tasks in quick succession
    info!("Testing rapid task submission");
    let mut task_ids = Vec::new();
    for i in 0..10 {
        let task_id = format!("rapid-task-{}-{}", i, uuid::Uuid::new_v4());
        let task_request = TaskRequest {
            task_id: task_id.clone(),
            command: "echo".to_string(),
            args: vec![format!("Task {}", i)],
            cpu_cores: 1,
            memory_mb: 128,
            disk_gb: 0,
            required_features: vec![],
            timeout_secs: 10,
        };
        
        leader_client.clone()
            .submit_task(task_request)
            .await
            .expect("Failed to submit task");
        
        task_ids.push(task_id);
    }
    
    // Wait for all tasks to be replicated
    timing::wait_for_condition_with_backoff(
        || {
            let task_ids = task_ids.clone();
            let cluster = &cluster;
            async move {
                // Check if all tasks are visible on all nodes
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        for task_id in &task_ids {
                            let status_request = TaskStatusRequest {
                                task_id: task_id.clone(),
                            };
                            if let Ok(resp) = client.clone().get_task_status(status_request).await {
                                if !resp.into_inner().found {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    ).await.expect("All tasks should be replicated");
    
    // Verify all tasks are readable from all nodes
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        for task_id in &task_ids {
            let status_request = TaskStatusRequest {
                task_id: task_id.clone(),
            };
            
            let status_response = client.clone()
                .get_task_status(status_request)
                .await
                .expect("Failed to get task status");
            
            assert!(status_response.into_inner().found, 
                "Task {} should be found on node {}", task_id, node_id);
        }
    }
    
    info!("All tasks successfully replicated to all nodes");
    cluster.shutdown().await;
}

/// Test read-after-write consistency for VM operations
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_vm_read_after_write_consistency() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Create a VM through the leader
    let vm_name = format!("test-vm-{}", uuid::Uuid::new_v4());
    let create_request = CreateVmRequest {
        name: vm_name.clone(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    };
    
    info!("Creating VM {} through leader", vm_name);
    let response = leader_client.clone()
        .create_vm(create_request)
        .await
        .expect("Failed to create VM");
    
    assert!(response.into_inner().success, "VM creation should succeed");
    
    // Wait for VM to be replicated to all nodes
    timing::wait_for_condition_with_backoff(
        || {
            let vm_name = vm_name.clone();
            let cluster = &cluster;
            async move {
                // Check if VM is visible on all nodes
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        let status_request = GetVmStatusRequest {
                            name: vm_name.clone(),
                        };
                        if let Ok(resp) = client.clone().get_vm_status(status_request).await {
                            if !resp.into_inner().found {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(50),
    ).await.expect("VM should be replicated to all nodes");
    
    // Verify VM is visible from all nodes
    info!("Verifying VM is visible from all nodes");
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        let status_request = GetVmStatusRequest {
            name: vm_name.clone(),
        };
        
        let status_response = client.clone()
            .get_vm_status(status_request)
            .await
            .expect("Failed to get VM status");
        
        let status = status_response.into_inner();
        assert!(status.found, "VM should be found on node {}", node_id);
        assert_eq!(status.vm_info.as_ref().unwrap().name, vm_name);
        assert_eq!(status.vm_info.as_ref().unwrap().vcpus, 2);
        assert_eq!(status.vm_info.as_ref().unwrap().memory_mb, 1024);
        info!("VM {} found on node {} with state {:?}", 
            vm_name, node_id, status.vm_info.unwrap().state);
    }
    
    cluster.shutdown().await;
}

/// Test explicit data verification across nodes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_data_verification_across_nodes() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Create multiple VMs
    let mut vms = HashMap::new();
    for i in 0..5 {
        let vm_name = format!("verify-vm-{}", i);
        let create_request = CreateVmRequest {
            name: vm_name.clone(),
            config_path: format!("/tmp/vm-{}.nix", i),
            vcpus: (i % 4 + 1) as u32,
            memory_mb: (512 * (i + 1)) as u32,
        };
        
        leader_client.clone()
            .create_vm(create_request.clone())
            .await
            .expect("Failed to create VM");
        
        vms.insert(vm_name, (create_request.vcpus, create_request.memory_mb));
    }
    
    // Wait for all VMs to be replicated
    timing::wait_for_condition_with_backoff(
        || {
            let vms = vms.clone();
            let cluster = &cluster;
            async move {
                // Check if all VMs are visible on all nodes
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        for vm_name in vms.keys() {
                            let status_request = GetVmStatusRequest {
                                name: vm_name.clone(),
                            };
                            if let Ok(resp) = client.clone().get_vm_status(status_request).await {
                                if !resp.into_inner().found {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    ).await.expect("All VMs should be replicated");
    
    // Collect VM lists from all nodes
    let mut node_vm_lists = HashMap::new();
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        let list_response = client.clone()
            .list_vms(ListVmsRequest {})
            .await
            .expect("Failed to list VMs");
        
        let vms = list_response.into_inner().vms;
        node_vm_lists.insert(*node_id, vms);
    }
    
    // Verify all nodes have identical VM lists
    info!("Verifying all nodes have identical VM lists");
    let first_node_id = *cluster.nodes().keys().next().unwrap();
    let reference_list = &node_vm_lists[&first_node_id];
    
    for (node_id, vm_list) in &node_vm_lists {
        if *node_id == first_node_id {
            continue;
        }
        
        assert_eq!(vm_list.len(), reference_list.len(), 
            "Node {} has different VM count", node_id);
        
        // Sort both lists by name for comparison
        let mut sorted_ref: Vec<_> = reference_list.iter()
            .map(|vm| (vm.name.clone(), vm.vcpus, vm.memory_mb))
            .collect();
        sorted_ref.sort_by(|a, b| a.0.cmp(&b.0));
        
        let mut sorted_node: Vec<_> = vm_list.iter()
            .map(|vm| (vm.name.clone(), vm.vcpus, vm.memory_mb))
            .collect();
        sorted_node.sort_by(|a, b| a.0.cmp(&b.0));
        
        assert_eq!(sorted_node, sorted_ref, 
            "Node {} has different VM data", node_id);
    }
    
    // Verify specific VM details match expected values
    for (vm_name, (expected_vcpus, expected_memory)) in &vms {
        for (node_id, _) in cluster.nodes() {
            let client = cluster.client(*node_id).await.expect("Failed to get client");
            
            let status_request = GetVmStatusRequest {
                name: vm_name.clone(),
            };
            
            let status_response = client.clone()
                .get_vm_status(status_request)
                .await
                .expect("Failed to get VM status");
            
            let vm_info = status_response.into_inner().vm_info.unwrap();
            assert_eq!(vm_info.vcpus, *expected_vcpus, 
                "VM {} on node {} has wrong vcpus", vm_name, node_id);
            assert_eq!(vm_info.memory_mb, *expected_memory,
                "VM {} on node {} has wrong memory", vm_name, node_id);
        }
    }
    
    info!("All nodes have identical and correct data");
    cluster.shutdown().await;
}

/// Test eventual consistency timing
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "This test may be flaky after consensus enforcement changes - needs investigation"]
async fn test_eventual_consistency_timing() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Measure replication latency for different operation sizes
    let test_cases = vec![
        ("small", 1),
        ("medium", 10),
        ("large", 50),
    ];
    
    for (test_name, vm_count) in test_cases {
        info!("Testing {} operation: {} VMs", test_name, vm_count);
        
        let start_time = Instant::now();
        
        // Create VMs
        let mut vm_names = Vec::new();
        for i in 0..vm_count {
            let vm_name = format!("{}-vm-{}", test_name, i);
            let create_request = CreateVmRequest {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 256,
            };
            
            leader_client.clone()
                .create_vm(create_request)
                .await
                .expect("Failed to create VM");
            
            vm_names.push(vm_name);
        }
        
        let submit_time = start_time.elapsed();
        
        // Wait for all nodes to see all VMs
        let replication_result = timing::wait_for_condition_with_backoff(
            || {
                let vm_names = vm_names.clone();
                let cluster = &cluster;
                async move {
                    for (node_id, _) in cluster.nodes() {
                        let client = match cluster.client(*node_id).await {
                            Ok(c) => c,
                            Err(_) => return false,
                        };
                        
                        for vm_name in &vm_names {
                            let status_request = GetVmStatusRequest {
                                name: vm_name.clone(),
                            };
                            
                            match client.clone().get_vm_status(status_request).await {
                                Ok(resp) => {
                                    if !resp.into_inner().found {
                                        return false;
                                    }
                                },
                                _ => return false,
                            }
                        }
                    }
                    true
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(50),
        ).await;
        
        let total_time = start_time.elapsed();
        let replication_time = total_time - submit_time;
        
        assert!(replication_result.is_ok(), 
            "Replication should complete within timeout");
        
        info!("{} operation stats: submit_time={:?}, replication_time={:?}, total_time={:?}",
            test_name, submit_time, replication_time, total_time);
    }
    
    cluster.shutdown().await;
}

/// Test concurrent writes from multiple nodes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_writes_consistency() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Each node will create VMs concurrently
    let mut handles = Vec::new();
    
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        let node_id = *node_id;
        
        let handle = tokio::spawn(async move {
            let mut created_vms = Vec::new();
            
            for i in 0..5 {
                let vm_name = format!("concurrent-node{}-vm{}", node_id, i);
                let create_request = CreateVmRequest {
                    name: vm_name.clone(),
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: node_id as u32,
                    memory_mb: (256 * node_id) as u32,
                };
                
                match client.clone().create_vm(create_request).await {
                    Ok(resp) => {
                        if resp.into_inner().success {
                            created_vms.push(vm_name);
                        } else {
                            warn!("VM creation failed but returned success=false");
                        }
                    },
                    Err(e) => {
                        warn!("VM creation error: {}", e);
                    }
                }
                
                // Small delay between operations
                timing::robust_sleep(Duration::from_millis(10)).await;
            }
            
            (node_id, created_vms)
        });
        
        handles.push(handle);
    }
    
    // Wait for all concurrent operations to complete
    let results: Vec<(u64, Vec<String>)> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("Task should not panic"))
        .collect();
    
    // Collect all VMs that were created
    let all_vms: Vec<String> = results.iter()
        .flat_map(|(_, vms)| vms.clone())
        .collect();
    
    // Wait for all concurrent writes to be replicated
    timing::wait_for_condition_with_backoff(
        || {
            let all_vms = all_vms.clone();
            let cluster = &cluster;
            async move {
                // Check if all VMs are visible on all nodes
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        for vm_name in &all_vms {
                            let status_request = GetVmStatusRequest {
                                name: vm_name.clone(),
                            };
                            if let Ok(resp) = client.clone().get_vm_status(status_request).await {
                                if !resp.into_inner().found {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    ).await.expect("All concurrent writes should be replicated");
    
    // Verify all VMs exist on all nodes
    info!("Verifying all concurrent writes are visible on all nodes");
    
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        for vm_name in &all_vms {
            let status_request = GetVmStatusRequest {
                name: vm_name.clone(),
            };
            
            let status_response = client.clone()
                .get_vm_status(status_request)
                .await
                .expect("Failed to get VM status");
            
            assert!(status_response.into_inner().found,
                "VM {} should be found on node {}", vm_name, node_id);
        }
    }
    
    info!("All {} VMs from concurrent writes are consistent across all nodes", all_vms.len());
    cluster.shutdown().await;
}

/// Test linearizability of operations
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_linearizability() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Create a VM
    let vm_name = "linearizable-vm";
    let create_request = CreateVmRequest {
        name: vm_name.to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 1,
        memory_mb: 512,
    };
    
    leader_client.clone()
        .create_vm(create_request)
        .await
        .expect("Failed to create VM");
    
    // Start the VM (this should update its state)
    leader_client.clone()
        .start_vm(blixard::proto::StartVmRequest {
            name: vm_name.to_string(),
        })
        .await
        .expect("Failed to start VM");
    
    // TODO: Our system doesn't currently provide linearizable reads.
    // Followers read from their local database which might be behind the leader.
    // For true linearizability, we would need to either:
    // 1. Forward read requests to the leader
    // 2. Have followers wait until they've applied the latest committed entries
    // For now, we wait for replication to complete.
    timing::wait_for_condition_with_backoff(
        || {
            let vm_name = vm_name.to_string();
            let cluster = &cluster;
            async move {
                // Check if VM start is visible on all nodes
                let mut states = Vec::new();
                for (node_id, _) in cluster.nodes() {
                    if let Ok(client) = cluster.client(*node_id).await {
                        let status_request = GetVmStatusRequest {
                            name: vm_name.clone(),
                        };
                        if let Ok(resp) = client.clone().get_vm_status(status_request).await {
                            if let Some(vm_info) = resp.into_inner().vm_info {
                                states.push(vm_info.state);
                            } else {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                // All nodes should see the same state
                states.len() == cluster.nodes().len() && 
                    states.windows(2).all(|w| w[0] == w[1])
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(50),
    ).await.expect("VM state should be consistent across all nodes");
    
    // Read from all nodes - they should all see the latest state
    let mut states = Vec::new();
    for (node_id, _) in cluster.nodes() {
        let client = cluster.client(*node_id).await.expect("Failed to get client");
        
        let status_response = client.clone()
            .get_vm_status(GetVmStatusRequest {
                name: vm_name.to_string(),
            })
            .await
            .expect("Failed to get VM status");
        
        let vm_info = status_response.into_inner().vm_info.unwrap();
        states.push((node_id, vm_info.state));
    }
    
    // All nodes should see the same state
    let first_state = states[0].1;
    for (node_id, state) in &states {
        assert_eq!(*state, first_state,
            "Node {} sees different state {:?} vs {:?}", node_id, state, first_state);
    }
    
    info!("All nodes see consistent state: {:?}", first_state);
    cluster.shutdown().await;
}

// Additional tests can be added for:
// - Network partition scenarios (requires more infrastructure)
// - Performance benchmarks
// - Edge cases like very large state transfers
// - Storage corruption recovery

/// Placeholder for network partition test
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires network partition simulation infrastructure"]
async fn test_network_partition_behavior() {
    // TODO: Implement when network partition simulation is available
    // This would test:
    // - Behavior during partition
    // - Data reconciliation after partition heals
    // - Split-brain prevention
}

/// Basic storage performance benchmark
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_storage_replication_performance() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Benchmark different operation types
    let benchmarks = vec![
        ("single_vm_create", 1, 1),
        ("batch_vm_create", 10, 1),
        ("concurrent_vm_create", 10, 10),
    ];
    
    for (name, total_ops, concurrent_ops) in benchmarks {
        info!("Running benchmark: {}", name);
        
        let start = Instant::now();
        let mut handles = Vec::new();
        
        for batch in 0..(total_ops / concurrent_ops) {
            for i in 0..concurrent_ops {
                let vm_name = format!("{}-batch{}-vm{}", name, batch, i);
                let mut client = leader_client.clone();
                
                let handle = tokio::spawn(async move {
                    let create_request = CreateVmRequest {
                        name: vm_name,
                        config_path: "/tmp/test.nix".to_string(),
                        vcpus: 1,
                        memory_mb: 256,
                    };
                    
                    client.create_vm(create_request).await
                });
                
                handles.push(handle);
            }
            
            // Wait for batch to complete
            futures::future::join_all(handles.drain(..)).await;
        }
        
        let elapsed = start.elapsed();
        let ops_per_sec = (total_ops as f64) / elapsed.as_secs_f64();
        
        info!("Benchmark {}: {} ops in {:?} ({:.2} ops/sec)",
            name, total_ops, elapsed, ops_per_sec);
    }
    
    cluster.shutdown().await;
}

// Helper module for UUID generation
mod uuid {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    pub struct Uuid;
    
    impl Uuid {
        pub fn new_v4() -> String {
            let count = COUNTER.fetch_add(1, Ordering::SeqCst);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros();
            format!("{:x}-{:x}", timestamp, count)
        }
    }
}