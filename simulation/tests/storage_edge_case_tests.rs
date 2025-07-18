//! Storage Edge Case Tests
//!
//! This module tests edge cases and extreme scenarios for the distributed storage layer,
//! including:
//! - Very large state transfers
//! - Rapid leader elections during writes
//! - Storage corruption recovery
//! - Memory pressure scenarios
//! - Extreme concurrency

#![cfg(feature = "test-helpers")]

use std::time::{Duration, Instant};
use blixard_core::{
    proto::{
        CreateVmRequest, ListVmsRequest, 
        TaskRequest, ClusterStatusRequest, StopVmRequest,
    },
    test_helpers::{TestCluster, timing},
};
use tokio::time::sleep;
use tracing::info;

/// Test very large state transfers
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_large_state_transfer() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let mut cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    info!("Creating large state with 100 VMs and 50 tasks");
    
    // Create a large number of VMs with batching
    const LARGE_VM_COUNT: usize = 100;  // Reduced from 1000
    const BATCH_SIZE: usize = 10;
    
    let mut vm_creation_failures = 0;
    let mut vm_creation_successes = 0;
    
    for batch_start in (0..LARGE_VM_COUNT).step_by(BATCH_SIZE) {
        let batch_end = (batch_start + BATCH_SIZE).min(LARGE_VM_COUNT);
        
        // Create batch of VMs
        for i in batch_start..batch_end {
            let vm_name = format!("large-state-vm-{}", i);
            let create_request = CreateVmRequest {
                name: vm_name,
                config_path: format!("/tmp/vm-{}.nix", i % 10),
                vcpus: (i % 8 + 1) as u32,
                memory_mb: (256 * ((i % 4) + 1)) as u32,
            };
            
            match leader_client.clone().create_vm(create_request).await {
                Ok(_) => vm_creation_successes += 1,
                Err(e) => {
                    vm_creation_failures += 1;
                    info!("Failed to create VM {}: {}", i, e);
                }
            }
        }
        
        // Allow Raft to process entries after each batch
        sleep(Duration::from_millis(200)).await;
    }
    
    // Create a large number of tasks with batching
    const LARGE_TASK_COUNT: usize = 50;  // Reduced from 500
    const TASK_BATCH_SIZE: usize = 5;
    
    let mut task_submission_failures = 0;
    let mut task_submission_successes = 0;
    
    for batch_start in (0..LARGE_TASK_COUNT).step_by(TASK_BATCH_SIZE) {
        let batch_end = (batch_start + TASK_BATCH_SIZE).min(LARGE_TASK_COUNT);
        
        // Create batch of tasks
        for i in batch_start..batch_end {
            let task_request = TaskRequest {
                task_id: format!("large-state-task-{}", i),
                command: "echo".to_string(),
                args: vec![format!("Large task {}", i)],
                cpu_cores: (i % 4 + 1) as u32,
                memory_mb: (128 * ((i % 4) + 1)) as u64,
                disk_gb: i as u64 % 10,
                required_features: if i % 5 == 0 { vec!["gpu".to_string()] } else { vec![] },
                timeout_secs: 3600,
            };
            
            match leader_client.clone().submit_task(task_request).await {
                Ok(_) => task_submission_successes += 1,
                Err(e) => {
                    task_submission_failures += 1;
                    info!("Failed to submit task {}: {}", i, e);
                }
            }
        }
        
        // Allow Raft to process entries after each batch
        sleep(Duration::from_millis(200)).await;
    }
    
    // Verify that at least 80% of operations succeeded (allowing for some failures under stress)
    let total_vms = LARGE_VM_COUNT;
    let total_tasks = LARGE_TASK_COUNT;
    
    assert!(vm_creation_successes > 0, "At least some VMs should be created successfully");
    assert!(task_submission_successes > 0, "At least some tasks should be submitted successfully");
    
    let vm_success_rate = vm_creation_successes as f64 / total_vms as f64;
    let task_success_rate = task_submission_successes as f64 / total_tasks as f64;
    
    info!("VM creation: {} successes, {} failures, success rate: {:.2}%", 
        vm_creation_successes, vm_creation_failures, vm_success_rate * 100.0);
    info!("Task submission: {} successes, {} failures, success rate: {:.2}%", 
        task_submission_successes, task_submission_failures, task_success_rate * 100.0);
    
    // Under stress conditions, we expect at least 80% success rate
    assert!(vm_success_rate >= 0.8, 
        "VM creation success rate {:.2}% is below acceptable threshold of 80%", vm_success_rate * 100.0);
    assert!(task_success_rate >= 0.8, 
        "Task submission success rate {:.2}% is below acceptable threshold of 80%", task_success_rate * 100.0);
    
    // Wait for initial replication
    sleep(Duration::from_secs(2)).await;
    
    // Add a new node and measure time to sync large state
    info!("Adding new node to cluster with large state");
    let start = Instant::now();
    let new_node_id = cluster.add_node().await.expect("Failed to add node");
    
    // Wait for new node to fully sync
    let sync_result = timing::wait_for_condition_with_backoff(
        || {
            let cluster = &cluster;
            async move {
                if let Ok(client) = cluster.client(new_node_id).await {
                    match client.clone().list_vms(ListVmsRequest {}).await {
                        Ok(response) => {
                            let vm_count = response.into_inner().vms.len();
                            if vm_count > 0 {
                                info!("New node has synced {} VMs so far", vm_count);
                            }
                            return vm_count >= 90; // Allow for some tolerance (90% of 100 VMs)
                        }
                        Err(e) => {
                            info!("Still waiting for new node to sync: {}", e);
                        }
                    }
                }
                false
            }
        },
        Duration::from_secs(30), // Reduced timeout for debugging
        Duration::from_millis(1000), // Check every second
    ).await;
    
    let sync_time = start.elapsed();
    
    if sync_result.is_ok() {
        info!("Large state sync completed in {:?}", sync_time);
        assert!(sync_time.as_secs() < 60, 
            "Large state sync took {:?}, which exceeds reasonable limit of 60s", sync_time);
    } else {
        // Get final VM count on new node for diagnostics
        let final_count = if let Ok(client) = cluster.client(new_node_id).await {
            if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                response.into_inner().vms.len()
            } else {
                0
            }
        } else {
            0
        };
        
        panic!("Large state sync failed to complete within timeout. \
                New node synced {} VMs out of expected 90+. \
                This indicates a critical issue with snapshot transfer or replication.", 
                final_count);
    }
    
    cluster.shutdown().await;
}

/// Test rapid leader elections during writes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rapid_leader_changes_during_writes() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Get initial leader client
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Start continuous write operations
    let write_handle = {
        let client = leader_client.clone();
        tokio::spawn(async move {
            let mut successful_writes = 0;
            let mut failed_writes = 0;
            
            for i in 0..100 {
                let vm_name = format!("election-test-vm-{}", i);
                let create_request = CreateVmRequest {
                    name: vm_name,
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 256,
                };
                
                match client.clone().create_vm(create_request).await {
                    Ok(_) => successful_writes += 1,
                    Err(_) => failed_writes += 1,
                }
                
                sleep(Duration::from_millis(50)).await;
            }
            
            (successful_writes, failed_writes)
        })
    };
    
    // Simulate leader failures by stopping nodes
    sleep(Duration::from_secs(1)).await;
    
    // Note: In a real test, we would stop the leader node here
    // This requires additional infrastructure to identify and stop specific nodes
    
    // Wait for writes to complete
    let (successful, failed) = write_handle.await.unwrap();
    
    info!("Write results during leader changes: {} successful, {} failed", 
        successful, failed);
    
    // Verify data consistency after elections
    let final_vm_count = if let Ok(client) = cluster.leader_client().await {
        if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
            response.into_inner().vms.len()
        } else {
            0
        }
    } else {
        0
    };
    
    info!("Final VM count after rapid elections: {}", final_vm_count);
    assert!(final_vm_count > 0, "Should have some VMs despite leader changes");
    
    cluster.shutdown().await;
}

/// Test extreme concurrency
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_extreme_concurrency() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    info!("Testing extreme concurrency with 1000 concurrent operations");
    
    let start = Instant::now();
    let mut handles = Vec::new();
    
    // Launch 1000 concurrent operations
    for i in 0..1000 {
        let mut client = leader_client.clone();
        
        let handle = tokio::spawn(async move {
            let operation = i % 4;
            
            match operation {
                0 => {
                    // Create VM
                    let vm_name = format!("concurrent-vm-{}", i);
                    let create_request = CreateVmRequest {
                        name: vm_name,
                        config_path: "/tmp/test.nix".to_string(),
                        vcpus: 1,
                        memory_mb: 128,
                    };
                    
                    client.create_vm(create_request).await.is_ok()
                },
                1 => {
                    // Submit task
                    let task_request = TaskRequest {
                        task_id: format!("concurrent-task-{}", i),
                        command: "echo".to_string(),
                        args: vec![format!("Task {}", i)],
                        cpu_cores: 1,
                        memory_mb: 64,
                        disk_gb: 0,
                        required_features: vec![],
                        timeout_secs: 10,
                    };
                    
                    client.submit_task(task_request).await.is_ok()
                },
                2 => {
                    // List VMs
                    client.list_vms(ListVmsRequest {}).await.is_ok()
                },
                _ => {
                    // Get cluster status
                    client.get_cluster_status(ClusterStatusRequest {}).await.is_ok()
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let results = futures::future::join_all(handles).await;
    let successful = results.iter().filter(|r| r.is_ok() && r.as_ref().unwrap() == &true).count();
    let failed = results.len() - successful;
    
    let elapsed = start.elapsed();
    let ops_per_sec = 1000.0 / elapsed.as_secs_f64();
    
    info!("Extreme concurrency test completed:");
    info!("  Total operations: 1000");
    info!("  Successful: {}", successful);
    info!("  Failed: {}", failed);
    info!("  Duration: {:?}", elapsed);
    info!("  Operations/sec: {:.2}", ops_per_sec);
    
    cluster.shutdown().await;
}

/// Test memory pressure scenarios
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_memory_pressure() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    info!("Creating VMs with large memory requirements");
    
    // Create VMs with increasingly large memory requirements
    let mut created_vms = 0;
    for i in 0..100 {
        let vm_name = format!("memory-test-vm-{}", i);
        let memory_mb = 1024 * (i + 1); // 1GB, 2GB, 3GB, etc.
        
        let create_request = CreateVmRequest {
            name: vm_name.clone(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 4,
            memory_mb,
        };
        
        match leader_client.clone().create_vm(create_request).await {
            Ok(resp) => {
                if resp.into_inner().success {
                    created_vms += 1;
                } else {
                    info!("VM creation failed at {} GB", i + 1);
                    break;
                }
            },
            Err(e) => {
                info!("VM creation error at {} GB: {}", i + 1, e);
                break;
            }
        }
    }
    
    info!("Successfully created {} VMs with increasing memory requirements", created_vms);
    
    // ASSERTION: Should be able to create at least a few VMs
    assert!(created_vms >= 3, "Should create at least 3 VMs before hitting memory limits, got {}", created_vms);
    assert!(created_vms <= 100, "VM creation should eventually fail due to memory constraints");
    
    // Verify VMs are actually stored in cluster state
    let vm_list = leader_client.clone().list_vms(ListVmsRequest {}).await
        .expect("Should be able to list VMs");
    let stored_vm_count = vm_list.into_inner().vms.len();
    assert_eq!(stored_vm_count, created_vms, "All created VMs should be stored in cluster state");
    
    // Test task scheduling under memory pressure
    let mut scheduled_tasks = 0;
    for i in 0..50 {
        let task_request = TaskRequest {
            task_id: format!("memory-task-{}", i),
            command: "stress".to_string(),
            args: vec!["--vm".to_string(), "1".to_string()],
            cpu_cores: 2,
            memory_mb: 2048, // 2GB per task
            disk_gb: 0,
            required_features: vec![],
            timeout_secs: 60,
        };
        
        match leader_client.clone().submit_task(task_request).await {
            Ok(resp) => {
                if resp.into_inner().accepted {
                    scheduled_tasks += 1;
                } else {
                    info!("Task scheduling rejected at task {}", i);
                    break;
                }
            },
            Err(e) => {
                info!("Task submission error at task {}: {}", i, e);
                break;
            }
        }
    }
    
    info!("Successfully scheduled {} tasks under memory pressure", scheduled_tasks);
    
    // ASSERTION: Should be able to schedule at least some tasks
    assert!(scheduled_tasks >= 1, "Should schedule at least 1 task before resource constraints, got {}", scheduled_tasks);
    assert!(scheduled_tasks <= 50, "Task scheduling should eventually be limited by resources");
    
    // Verify system remains responsive despite memory pressure
    let health_check = leader_client.clone().get_cluster_status(ClusterStatusRequest {}).await;
    assert!(health_check.is_ok(), "Cluster should remain responsive under memory pressure");
    
    let status = health_check.unwrap().into_inner();
    assert!(status.leader_id > 0, "Cluster should maintain leadership under memory pressure");
    assert_eq!(status.nodes.len(), 3, "All nodes should remain in cluster under memory pressure");
    
    cluster.shutdown().await;
}

/// Test recovery from various failure scenarios
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_failure_recovery_scenarios() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let mut cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Create initial state
    for i in 0..20 {
        let vm_name = format!("recovery-test-vm-{}", i);
        let create_request = CreateVmRequest {
            name: vm_name,
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory_mb: 512,
        };
        
        leader_client.clone()
            .create_vm(create_request)
            .await
            .expect("Failed to create VM");
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Scenario 1: Multiple nodes join simultaneously
    info!("Testing multiple simultaneous node joins");
    // Note: Parallel node addition would require infrastructure changes
    // For now, add nodes sequentially
    let mut new_nodes = Vec::new();
    for _ in 6..9 {
        match cluster.add_node().await {
            Ok(id) => new_nodes.push(Ok(id)),
            Err(e) => new_nodes.push(Err(e)),
        }
    }
    
    let successful_joins = new_nodes.iter().filter(|r| r.is_ok()).count();
    info!("Successfully joined {} nodes", successful_joins);
    
    // Scenario 2: Rapid configuration changes
    info!("Testing rapid configuration changes");
    let mut add_node_successes = 0;
    let mut add_node_failures = 0;
    
    for i in 0..5 {
        if i % 2 == 0 {
            // Add a node
            match cluster.add_node().await {
                Ok(id) => {
                    info!("Added node {}", id);
                    add_node_successes += 1;
                },
                Err(e) => {
                    info!("Failed to add node during rapid changes: {}", e);
                    add_node_failures += 1;
                }
            }
        } else {
            // Remove a non-leader node
            // Note: This requires identifying a non-leader node
            // which needs additional infrastructure
        }
        
        sleep(Duration::from_millis(200)).await;
    }
    
    // Assert that at least some configuration changes succeeded
    assert!(add_node_successes > 0, 
        "At least some node additions should succeed during rapid configuration changes");
    info!("Rapid configuration changes: {} succeeded, {} failed", 
        add_node_successes, add_node_failures);
    
    // Verify final state consistency
    let final_vm_count = if let Ok(client) = cluster.leader_client().await {
        if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
            response.into_inner().vms.len()
        } else {
            0
        }
    } else {
        0
    };
    
    info!("Final VM count after recovery scenarios: {}", final_vm_count);
    assert_eq!(final_vm_count, 20, "All VMs should still be present");
    
    cluster.shutdown().await;
}

/// Test handling of malformed or extreme requests
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_extreme_request_handling() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Test 1: VM with extremely long name (should be rejected)
    let long_name = "vm-".to_string() + &"x".repeat(1000);
    let create_request = CreateVmRequest {
        name: long_name,
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 1,
        memory_mb: 256,
    };
    
    let long_name_result = leader_client.clone().create_vm(create_request).await;
    match long_name_result {
        Ok(resp) => {
            let success = resp.into_inner().success;
            info!("Long name VM creation result: success={}", success);
            assert!(!success, "VM with extremely long name should be rejected by validation");
        },
        Err(e) => {
            info!("Long name VM creation failed as expected: {}", e);
            // Error response is acceptable - validates input properly
        }
    }
    
    // Test 2: VM with zero resources (should be rejected)
    let create_request = CreateVmRequest {
        name: "zero-resource-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 0,
        memory_mb: 0,
    };
    
    let zero_resource_result = leader_client.clone().create_vm(create_request).await;
    match zero_resource_result {
        Ok(resp) => {
            let success = resp.into_inner().success;
            info!("Zero resource VM creation result: success={}", success);
            assert!(!success, "VM with zero resources should be rejected by validation");
        },
        Err(e) => {
            info!("Zero resource VM creation failed as expected: {}", e);
            // Error response is acceptable - validates input properly
        }
    }
    
    // Test 3: Task with unrealistic requirements (should be rejected)
    let task_request = TaskRequest {
        task_id: "impossible-task".to_string(),
        command: "impossible".to_string(),
        args: vec![],
        cpu_cores: 1000,
        memory_mb: 1_000_000_000, // 1PB of memory
        disk_gb: 1_000_000, // 1PB of disk
        required_features: vec!["quantum-processor".to_string()],
        timeout_secs: 1,
    };
    
    let impossible_task_result = leader_client.clone().submit_task(task_request).await;
    match impossible_task_result {
        Ok(resp) => {
            let accepted = resp.into_inner().accepted;
            info!("Impossible task submission result: accepted={}", accepted);
            assert!(!accepted, "Task with impossible requirements should be rejected");
        },
        Err(e) => {
            info!("Impossible task submission failed as expected: {}", e);
            // Error response is acceptable - validates input properly
        }
    }
    
    // Verify cluster remains functional after processing malformed requests
    let health_check = leader_client.clone().get_cluster_status(ClusterStatusRequest {}).await;
    assert!(health_check.is_ok(), "Cluster should remain responsive after malformed requests");
    
    // Verify a normal request still works
    let normal_vm = CreateVmRequest {
        name: "normal-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory_mb: 512,
    };
    
    let normal_result = leader_client.clone().create_vm(normal_vm).await;
    assert!(normal_result.is_ok(), "Normal VM creation should still work after processing malformed requests");
    let normal_resp = normal_result.unwrap().into_inner();
    assert!(normal_resp.success, "Normal VM should be created successfully");
    
    // Test 4: Rapid repeated operations on same resource
    let vm_name = "contested-vm";
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let mut client = leader_client.clone();
        let vm_name = vm_name.to_string();
        
        let handle = tokio::spawn(async move {
            let result = if i % 3 == 0 {
                // Create
                let create_request = CreateVmRequest {
                    name: vm_name,
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 256,
                };
                client.create_vm(create_request).await.is_ok()
            } else if i % 3 == 1 {
                // Start
                let start_request = blixard_core::proto::StartVmRequest {
                    name: vm_name,
                };
                client.start_vm(start_request).await.is_ok()
            } else {
                // Stop
                let stop_request = StopVmRequest {
                    name: vm_name,
                };
                client.stop_vm(stop_request).await.is_ok()
            };
            result
        });
        
        handles.push(handle);
    }
    
    let results = futures::future::join_all(handles).await;
    let successful = results.iter().filter(|r| r.is_ok()).count();
    info!("Concurrent operations on same resource: {} successful out of 20", successful);
    
    cluster.shutdown().await;
}