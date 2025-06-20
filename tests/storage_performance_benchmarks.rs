//! Storage Performance Benchmarks
//!
//! This module benchmarks the performance characteristics of the distributed storage layer,
//! including:
//! - Replication latency across different cluster sizes
//! - Throughput under various workloads
//! - Snapshot transfer performance
//! - Operation latency percentiles

#![cfg(feature = "test-helpers")]

use std::time::{Duration, Instant};
use std::collections::HashMap;
use blixard::{
    proto::{
        CreateVmRequest, ListVmsRequest, TaskRequest,
        GetVmStatusRequest, StartVmRequest,
    },
    test_helpers::{TestCluster, timing},
};
use tokio::time::sleep;
use tracing::{info, warn};

/// Benchmark configuration
struct BenchmarkConfig {
    cluster_size: usize,
    operation_count: usize,
    concurrent_ops: usize,
    payload_size: usize,
}

/// Benchmark results
#[derive(Debug)]
struct BenchmarkResult {
    total_duration: Duration,
    operations_per_second: f64,
    average_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    replication_lag: Duration,
}

/// Benchmark replication latency for different cluster sizes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_replication_latency_by_cluster_size() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster_sizes = vec![3, 5, 7];
    let mut results = HashMap::new();
    
    for size in cluster_sizes {
        info!("Benchmarking {}-node cluster", size);
        
        let cluster = TestCluster::builder()
            .with_nodes(size)
            .build()
            .await
            .expect("Failed to create cluster");
        
        let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
        
        // Measure single operation replication latency
        let mut latencies = Vec::new();
        
        for i in 0..10 {
            let vm_name = format!("bench-vm-{}", i);
            let start = Instant::now();
            
            // Create VM
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
            
            // Wait for replication to all nodes
            let replication_complete = timing::wait_for_condition_with_backoff(
                || {
                    let cluster = &cluster;
                    let vm_name = vm_name.clone();
                    async move {
                        for (node_id, _) in cluster.nodes() {
                            let client = match cluster.client(*node_id).await {
                                Ok(c) => c,
                                Err(_) => return false,
                            };
                            
                            let status_request = GetVmStatusRequest {
                                name: vm_name.clone(),
                            };
                            
                            match client.clone().get_vm_status(status_request).await {
                                Ok(resp) => {
                                    if !resp.into_inner().found {
                                        return false;
                                    }
                                },
                                Err(_) => return false,
                            }
                        }
                        true
                    }
                },
                Duration::from_secs(5),
                Duration::from_millis(10),
            ).await;
            
            if replication_complete.is_ok() {
                let latency = start.elapsed();
                latencies.push(latency);
            }
        }
        
        // Calculate statistics
        latencies.sort();
        let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[latencies.len() * 95 / 100];
        let p99 = latencies[latencies.len() * 99 / 100];
        
        info!("Cluster size {}: avg={:?}, p50={:?}, p95={:?}, p99={:?}",
            size, avg_latency, p50, p95, p99);
        
        results.insert(size, (avg_latency, p50, p95, p99));
        
        cluster.shutdown().await;
        
        // Give time between tests
        sleep(Duration::from_millis(500)).await;
    }
    
    // Print summary
    info!("\nReplication Latency Summary:");
    for (size, (avg, p50, p95, p99)) in results {
        info!("  {} nodes: avg={:?}, p50={:?}, p95={:?}, p99={:?}",
            size, avg, p50, p95, p99);
    }
}

/// Benchmark throughput under different workloads
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_throughput_workloads() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let workloads = vec![
        ("sequential_writes", 100, 1),
        ("batch_writes", 100, 10),
        ("concurrent_writes", 100, 25),
        ("mixed_read_write", 200, 10), // 100 reads + 100 writes
    ];
    
    for (workload_name, total_ops, concurrency) in workloads {
        info!("Running workload: {}", workload_name);
        
        let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
        let start = Instant::now();
        let mut handles = Vec::new();
        
        if workload_name == "mixed_read_write" {
            // Create some initial VMs to read
            for i in 0..50 {
                let vm_name = format!("read-vm-{}", i);
                let create_request = CreateVmRequest {
                    name: vm_name,
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 256,
                };
                
                leader_client.clone()
                    .create_vm(create_request)
                    .await
                    .expect("Failed to create VM");
            }
            
            // Mixed read/write operations
            for i in 0..total_ops {
                let mut client = leader_client.clone();
                
                let handle = tokio::spawn(async move {
                    if i % 2 == 0 {
                        // Write operation
                        let vm_name = format!("write-vm-{}", i);
                        let create_request = CreateVmRequest {
                            name: vm_name,
                            config_path: "/tmp/test.nix".to_string(),
                            vcpus: 1,
                            memory_mb: 256,
                        };
                        
                        let _ = client.create_vm(create_request).await;
                    } else {
                        // Read operation
                        let vm_name = format!("read-vm-{}", i % 50);
                        let status_request = GetVmStatusRequest {
                            name: vm_name,
                        };
                        
                        let _ = client.get_vm_status(status_request).await;
                    }
                });
                
                handles.push(handle);
                
                if handles.len() >= concurrency {
                    // Wait for batch to complete
                    futures::future::join_all(handles.drain(..)).await;
                }
            }
        } else {
            // Write-only workloads
            for batch in 0..(total_ops / concurrency) {
                for i in 0..concurrency {
                    let vm_name = format!("{}-vm-{}-{}", workload_name, batch, i);
                    let mut client = leader_client.clone();
                    
                    let handle = tokio::spawn(async move {
                        let create_request = CreateVmRequest {
                            name: vm_name,
                            config_path: "/tmp/test.nix".to_string(),
                            vcpus: 1,
                            memory_mb: 256,
                        };
                        
                        let _ = client.create_vm(create_request).await;
                    });
                    
                    handles.push(handle);
                }
                
                // Wait for batch to complete
                futures::future::join_all(handles.drain(..)).await;
            }
        }
        
        // Wait for any remaining operations
        futures::future::join_all(handles).await;
        
        let elapsed = start.elapsed();
        let ops_per_sec = (total_ops as f64) / elapsed.as_secs_f64();
        
        info!("Workload {} completed: {} ops in {:?} ({:.2} ops/sec)",
            workload_name, total_ops, elapsed, ops_per_sec);
    }
    
    cluster.shutdown().await;
}

/// Benchmark snapshot transfer performance
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_snapshot_transfer() {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create different sizes of state
    let state_sizes = vec![
        ("small", 10),
        ("medium", 100),
        ("large", 500),
    ];
    
    for (size_name, vm_count) in state_sizes {
        // Create a fresh cluster for each test
        let mut cluster = TestCluster::builder()
            .with_nodes(3)
            .build()
            .await
            .expect("Failed to create cluster");
        
        let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
        info!("Testing snapshot transfer with {} state ({} VMs)", size_name, vm_count);
        
        // Create VMs to build state
        for i in 0..vm_count {
            let vm_name = format!("snapshot-test-{}-vm-{}", size_name, i);
            let create_request = CreateVmRequest {
                name: vm_name,
                config_path: "/tmp/test.nix".to_string(),
                vcpus: (i % 4 + 1) as u32,
                memory_mb: (256 * ((i % 4) + 1)) as u32,
            };
            
            leader_client.clone()
                .create_vm(create_request)
                .await
                .expect("Failed to create VM");
            
            // Also create some tasks
            if i % 10 == 0 {
                let task_request = TaskRequest {
                    task_id: format!("task-{}-{}", size_name, i),
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
            }
        }
        
        // Wait for state to be replicated and cluster to stabilize
        sleep(Duration::from_millis(500)).await;
        
        // Ensure all VMs are actually created and committed
        let wait_result = timing::wait_for_condition_with_backoff(
            || {
                let mut leader_client = leader_client.clone();
                let expected_count = vm_count;
                async move {
                    if let Ok(response) = leader_client.list_vms(ListVmsRequest {}).await {
                        response.into_inner().vms.len() >= expected_count
                    } else {
                        false
                    }
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(100),
        ).await;
        
        if wait_result.is_err() {
            warn!("State not fully replicated before adding node");
        }
        
        // Add a new node with retry logic
        let start = Instant::now();
        let mut new_node_id = None;
        for attempt in 0..3 {
            match cluster.add_node().await {
                Ok(id) => {
                    new_node_id = Some(id);
                    break;
                }
                Err(e) => {
                    warn!("Failed to add node (attempt {}): {}", attempt + 1, e);
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
        let new_node_id = match new_node_id {
            Some(id) => id,
            None => {
                warn!("Failed to add node after retries for {} state", size_name);
                continue;
            }
        };
        
        // Wait for new node to catch up
        let catch_up_result = timing::wait_for_condition_with_backoff(
            || {
                let cluster = &cluster;
                let vm_count = vm_count;
                async move {
                    if let Ok(client) = cluster.client(new_node_id).await {
                        if let Ok(response) = client.clone().list_vms(ListVmsRequest {}).await {
                            let vms = response.into_inner().vms;
                            return vms.len() >= vm_count;
                        }
                    }
                    false
                }
            },
            Duration::from_secs(30),
            Duration::from_millis(100),
        ).await;
        
        if catch_up_result.is_ok() {
            let catch_up_time = start.elapsed();
            info!("New node caught up to {} state in {:?}", size_name, catch_up_time);
            
            // Calculate approximate state size (rough estimate)
            let approx_state_size_kb = vm_count * 2; // ~2KB per VM entry
            let transfer_rate_kb_per_sec = (approx_state_size_kb as f64) / catch_up_time.as_secs_f64();
            
            info!("Approximate transfer rate: {:.2} KB/s", transfer_rate_kb_per_sec);
        } else {
            warn!("New node failed to catch up within timeout");
        }
        
        // Clean up
        cluster.shutdown().await;
        sleep(Duration::from_millis(500)).await;
    }
}

/// Benchmark operation latency percentiles
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_operation_latencies() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Collect latencies for different operations
    let mut create_vm_latencies = Vec::new();
    let mut start_vm_latencies = Vec::new();
    let mut list_vms_latencies = Vec::new();
    let mut get_status_latencies = Vec::new();
    
    // Create VMs and measure latencies
    for i in 0..100 {
        let vm_name = format!("latency-test-vm-{}", i);
        
        // Measure VM creation latency
        let start = Instant::now();
        let create_request = CreateVmRequest {
            name: vm_name.clone(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory_mb: 512,
        };
        
        leader_client.clone()
            .create_vm(create_request)
            .await
            .expect("Failed to create VM");
        
        create_vm_latencies.push(start.elapsed());
        
        // Measure VM start latency
        let start = Instant::now();
        let start_request = StartVmRequest {
            name: vm_name.clone(),
        };
        
        leader_client.clone()
            .start_vm(start_request)
            .await
            .expect("Failed to start VM");
        
        start_vm_latencies.push(start.elapsed());
        
        // Measure list VMs latency
        let start = Instant::now();
        leader_client.clone()
            .list_vms(ListVmsRequest {})
            .await
            .expect("Failed to list VMs");
        
        list_vms_latencies.push(start.elapsed());
        
        // Measure get status latency
        let start = Instant::now();
        let status_request = GetVmStatusRequest {
            name: vm_name,
        };
        
        leader_client.clone()
            .get_vm_status(status_request)
            .await
            .expect("Failed to get VM status");
        
        get_status_latencies.push(start.elapsed());
    }
    
    // Calculate and display percentiles for each operation
    let operations = vec![
        ("Create VM", &mut create_vm_latencies),
        ("Start VM", &mut start_vm_latencies),
        ("List VMs", &mut list_vms_latencies),
        ("Get VM Status", &mut get_status_latencies),
    ];
    
    info!("\nOperation Latency Percentiles:");
    for (op_name, latencies) in operations {
        latencies.sort();
        
        let p50 = latencies[latencies.len() / 2];
        let p90 = latencies[latencies.len() * 90 / 100];
        let p95 = latencies[latencies.len() * 95 / 100];
        let p99 = latencies[latencies.len() * 99 / 100];
        let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        
        info!("  {}:", op_name);
        info!("    Average: {:?}", avg);
        info!("    P50: {:?}", p50);
        info!("    P90: {:?}", p90);
        info!("    P95: {:?}", p95);
        info!("    P99: {:?}", p99);
    }
    
    cluster.shutdown().await;
}

/// Benchmark performance under sustained load
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_sustained_load() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let leader_client = cluster.leader_client().await.expect("Failed to get leader client");
    
    // Run sustained load for 30 seconds
    let test_duration = Duration::from_secs(30);
    let start_time = Instant::now();
    let mut operation_count = 0;
    let mut error_count = 0;
    
    info!("Running sustained load test for {:?}", test_duration);
    
    while start_time.elapsed() < test_duration {
        let vm_name = format!("sustained-vm-{}", operation_count);
        let create_request = CreateVmRequest {
            name: vm_name,
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        };
        
        match leader_client.clone().create_vm(create_request).await {
            Ok(_) => operation_count += 1,
            Err(_) => error_count += 1,
        }
        
        // Small delay to avoid overwhelming the system
        if operation_count % 10 == 0 {
            sleep(Duration::from_millis(10)).await;
        }
    }
    
    let total_duration = start_time.elapsed();
    let ops_per_sec = (operation_count as f64) / total_duration.as_secs_f64();
    let error_rate = (error_count as f64) / ((operation_count + error_count) as f64) * 100.0;
    
    info!("Sustained load test results:");
    info!("  Duration: {:?}", total_duration);
    info!("  Total operations: {}", operation_count);
    info!("  Operations/sec: {:.2}", ops_per_sec);
    info!("  Errors: {} ({:.2}%)", error_count, error_rate);
    
    cluster.shutdown().await;
}

// Helper function to calculate percentile
fn percentile(sorted_values: &[Duration], p: f64) -> Duration {
    let index = ((sorted_values.len() as f64 - 1.0) * p / 100.0) as usize;
    sorted_values[index]
}