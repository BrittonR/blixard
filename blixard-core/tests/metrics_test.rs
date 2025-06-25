//! Tests for the metrics and observability system

#![cfg(feature = "test-helpers")]

use std::time::Duration;
use blixard_core::metrics::{self, names as metric_names, Timer, MetricsRegistry};
use blixard_core::test_helpers::{TestCluster, timing};
use blixard_core::{metrics_inc, metrics_gauge, metrics_timer};

/// Test basic counter metrics
#[tokio::test]
async fn test_counter_metrics() {
    let registry = MetricsRegistry::new();
    
    // Initial state
    assert_eq!(registry.get_counter("test.counter"), 0);
    
    // Increment by 1
    registry.increment_counter("test.counter");
    assert_eq!(registry.get_counter("test.counter"), 1);
    
    // Increment by specific amount
    registry.increment_counter_by("test.counter", 5);
    assert_eq!(registry.get_counter("test.counter"), 6);
    
    // Multiple counters
    registry.increment_counter("test.another");
    assert_eq!(registry.get_counter("test.another"), 1);
    assert_eq!(registry.get_counter("test.counter"), 6);
}

/// Test gauge metrics
#[tokio::test]
async fn test_gauge_metrics() {
    let registry = MetricsRegistry::new();
    
    // Initial state
    assert_eq!(registry.get_gauge("test.gauge"), 0.0);
    
    // Set gauge
    registry.set_gauge("test.gauge", 42.5);
    assert_eq!(registry.get_gauge("test.gauge"), 42.5);
    
    // Update gauge
    registry.set_gauge("test.gauge", 10.0);
    assert_eq!(registry.get_gauge("test.gauge"), 10.0);
    
    // Multiple gauges
    registry.set_gauge("cpu.usage", 75.5);
    registry.set_gauge("memory.usage", 60.0);
    assert_eq!(registry.get_gauge("cpu.usage"), 75.5);
    assert_eq!(registry.get_gauge("memory.usage"), 60.0);
}

/// Test histogram metrics
#[tokio::test]
async fn test_histogram_metrics() {
    let registry = MetricsRegistry::new();
    
    // No data initially
    assert!(registry.get_histogram_stats("latency").is_none());
    
    // Record some values
    let durations = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    for ms in durations {
        registry.record_duration("latency", Duration::from_millis(ms));
    }
    
    // Check statistics
    let stats = registry.get_histogram_stats("latency").unwrap();
    assert_eq!(stats.count, 10);
    assert_eq!(stats.min, Duration::from_millis(10));
    assert_eq!(stats.max, Duration::from_millis(100));
    assert_eq!(stats.p50, Duration::from_millis(50));
    assert_eq!(stats.p90, Duration::from_millis(90));
    assert_eq!(stats.p99, Duration::from_millis(90)); // With 10 values, p99 is the 9th value
}

/// Test timer functionality
#[tokio::test]
async fn test_timer() {
    let registry = MetricsRegistry::new();
    
    // Use timer with explicit drop
    {
        let timer = Timer::new("operation.duration".to_string(), registry.clone());
        timing::robust_sleep(Duration::from_millis(50)).await;
        timer.stop();
    }
    
    let stats = registry.get_histogram_stats("operation.duration").unwrap();
    assert_eq!(stats.count, 1);
    assert!(stats.mean >= Duration::from_millis(50));
    
    // Use timer with automatic drop
    {
        let _timer = Timer::new("operation.duration".to_string(), registry.clone());
        timing::robust_sleep(Duration::from_millis(30)).await;
    }
    
    let stats = registry.get_histogram_stats("operation.duration").unwrap();
    assert_eq!(stats.count, 2);
}

/// Test metrics snapshot
#[tokio::test]
async fn test_metrics_snapshot() {
    let registry = MetricsRegistry::new();
    
    // Populate metrics
    registry.increment_counter("requests.total");
    registry.increment_counter_by("bytes.sent", 1024);
    registry.set_gauge("connections.active", 5.0);
    registry.record_duration("request.duration", Duration::from_millis(100));
    registry.record_duration("request.duration", Duration::from_millis(200));
    
    // Take snapshot
    let snapshot = registry.snapshot();
    
    // Verify counters
    assert_eq!(snapshot.counters.get("requests.total"), Some(&1));
    assert_eq!(snapshot.counters.get("bytes.sent"), Some(&1024));
    
    // Verify gauges
    assert_eq!(snapshot.gauges.get("connections.active"), Some(&5.0));
    
    // Verify histograms
    let hist_stats = snapshot.histograms.get("request.duration").unwrap();
    assert_eq!(hist_stats.count, 2);
    assert_eq!(hist_stats.min, Duration::from_millis(100));
    assert_eq!(hist_stats.max, Duration::from_millis(200));
}

/// Test global metrics registry
#[tokio::test]
async fn test_global_metrics() {
    let global = metrics::global();
    
    // Clear any existing state
    global.clear();
    
    // Use global registry
    global.increment_counter("global.counter");
    assert_eq!(global.get_counter("global.counter"), 1);
    
    // Access from different function
    increment_global_counter();
    assert_eq!(global.get_counter("global.counter"), 2);
}

fn increment_global_counter() {
    metrics::global().increment_counter("global.counter");
}

/// Test metrics macros
#[tokio::test]
async fn test_metrics_macros() {
    metrics::global().clear();
    
    // Test increment macro
    metrics_inc!("macro.counter");
    assert_eq!(metrics::global().get_counter("macro.counter"), 1);
    
    metrics_inc!("macro.counter", 5);
    assert_eq!(metrics::global().get_counter("macro.counter"), 6);
    
    // Test gauge macro
    metrics_gauge!("macro.gauge", 42.5);
    assert_eq!(metrics::global().get_gauge("macro.gauge"), 42.5);
    
    // Test timer macro
    {
        let _timer = metrics_timer!("macro.timer");
        timing::robust_sleep(Duration::from_millis(10)).await;
    }
    
    let stats = metrics::global().get_histogram_stats("macro.timer").unwrap();
    assert!(stats.count > 0);
}

/// Test Raft-specific metrics in a cluster
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_raft_metrics_in_cluster() {
    let cluster = TestCluster::new(3).await.expect("Failed to create cluster");
    
    // Wait for convergence
    cluster.wait_for_convergence(Duration::from_secs(10)).await
        .expect("Cluster should converge");
    
    // Get leader
    let leader_id = cluster.get_leader_id().await.unwrap();
    let leader = cluster.get_node(leader_id).unwrap();
    
    // Submit some proposals through the leader
    for i in 0..5 {
        let vm_name = format!("test-vm-{}", i);
        leader.shared_state.send_vm_command(
            blixard_core::types::VmCommand::Create {
                config: blixard_core::types::VmConfig {
                    name: vm_name,
                    config_path: "".to_string(),
                    vcpus: 1,
                    memory: 512,
                },
                node_id: leader_id,
            }
        ).await.ok();
    }
    
    // Give time for proposals to be processed
    timing::robust_sleep(Duration::from_secs(1)).await;
    
    // Check global metrics
    let global = metrics::global();
    
    // Should have recorded proposals
    let proposals = global.get_counter(metric_names::RAFT_PROPOSALS_TOTAL);
    assert!(proposals > 0, "Should have recorded Raft proposals");
    
    // Should have timer data for proposals
    let proposal_stats = global.get_histogram_stats(metric_names::RAFT_PROPOSAL_DURATION);
    assert!(proposal_stats.is_some(), "Should have proposal duration metrics");
    
    if let Some(stats) = proposal_stats {
        assert!(stats.count > 0);
        assert!(stats.mean > Duration::from_nanos(0));
    }
}

/// Test concurrent metric updates
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_metrics() {
    let registry = MetricsRegistry::new();
    let registry_clone = registry.clone();
    
    // Spawn multiple tasks updating metrics
    let mut handles = vec![];
    
    for i in 0..10 {
        let reg = registry_clone.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                reg.increment_counter("concurrent.counter");
                reg.set_gauge("concurrent.gauge", i as f64);
                reg.record_duration("concurrent.timer", Duration::from_micros(i * 10));
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify results
    assert_eq!(registry.get_counter("concurrent.counter"), 1000); // 10 tasks * 100 increments
    
    let hist_stats = registry.get_histogram_stats("concurrent.timer").unwrap();
    assert_eq!(hist_stats.count, 1000);
}

/// Test metric name constants
#[test]
fn test_metric_name_constants() {
    // Ensure metric names follow consistent patterns
    assert!(metric_names::RAFT_PROPOSALS_TOTAL.starts_with("raft."));
    assert!(metric_names::GRPC_REQUESTS_TOTAL.starts_with("grpc."));
    assert!(metric_names::VM_TOTAL.starts_with("vm."));
    assert!(metric_names::PEER_CONNECTIONS_ACTIVE.starts_with("peer."));
    assert!(metric_names::STORAGE_DB_SIZE.starts_with("storage."));
    assert!(metric_names::TASK_SUBMITTED.starts_with("task."));
    assert!(metric_names::RESOURCE_CPU_USED.starts_with("resource."));
    assert!(metric_names::NODE_UPTIME.starts_with("node."));
    assert!(metric_names::CLUSTER_SIZE.starts_with("cluster."));
}