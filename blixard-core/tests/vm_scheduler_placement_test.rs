//! Simple integration test for VM scheduler placement decisions
//!
//! Tests that the scheduler can make placement decisions in a multi-node cluster

#![cfg(feature = "test-helpers")]

use std::time::Duration;

use blixard_core::test_helpers::{TestCluster, timing};
use blixard_core::raft_manager::WorkerCapabilities;
use blixard_core::vm_scheduler::PlacementStrategy;
use blixard_core::types::VmConfig;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_scheduler_placement_decisions() {
    let cluster = TestCluster::new(3).await.expect("Failed to create cluster");
    
    // Wait for leader election
    timing::wait_for_condition_with_backoff(
        || async {
            for i in 1..=3 {
                if let Ok(node) = cluster.get_node(i) {
                    if node.shared_state.is_leader().await {
                        return true;
                    }
                }
            }
            false
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    ).await.expect("Should elect a leader");
    
    // Find the leader
    let mut leader_id = 0;
    for i in 1..=3 {
        if let Ok(node) = cluster.get_node(i) {
            if node.shared_state.is_leader().await {
                leader_id = i;
                break;
            }
        }
    }
    assert!(leader_id > 0, "Should have found a leader");
    
    let leader = cluster.get_node(leader_id).unwrap();
    
    // Check if VM manager is initialized
    let has_vm_manager = leader.shared_state.get_vm_manager().await.is_some();
    println!("Leader has VM manager: {}", has_vm_manager);
    assert!(has_vm_manager, "VM manager should be initialized");
    
    // Register workers with different capacities
    let capabilities1 = WorkerCapabilities {
        cpu_cores: 16,
        memory_mb: 32768,
        disk_gb: 500,
        features: vec!["microvm".to_string()],
    };
    leader.shared_state.register_worker_through_raft(1, "127.0.0.1:7001".to_string(), capabilities1).await
        .expect("Should register worker 1");
    
    let capabilities2 = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 250,
        features: vec!["microvm".to_string()],
    };
    leader.shared_state.register_worker_through_raft(2, "127.0.0.1:7002".to_string(), capabilities2).await
        .expect("Should register worker 2");
    
    let capabilities3 = WorkerCapabilities {
        cpu_cores: 4,
        memory_mb: 8192,
        disk_gb: 100,
        features: vec!["microvm".to_string(), "gpu".to_string()],
    };
    leader.shared_state.register_worker_through_raft(3, "127.0.0.1:7003".to_string(), capabilities3).await
        .expect("Should register worker 3");
    
    // Wait for registrations to propagate
    timing::robust_sleep(Duration::from_millis(1000)).await;
    
    // Test 1: Schedule a VM - should get a placement decision
    let vm1 = VmConfig {
        name: "test-vm-1".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    
    let placement1 = leader.shared_state.schedule_vm_placement(&vm1, PlacementStrategy::MostAvailable).await
        .expect("Should schedule VM");
    
    println!("Placement 1: node {}, reason: {}", placement1.selected_node_id, placement1.reason);
    assert!(placement1.selected_node_id >= 1 && placement1.selected_node_id <= 3);
    assert!(!placement1.reason.is_empty());
    
    // Test 2: Get cluster resource summary
    let summary = leader.shared_state.get_cluster_resource_summary().await
        .expect("Should get resource summary");
    
    println!("Cluster summary: {} nodes, {}vCPU total, {}MB RAM total", 
        summary.total_nodes, summary.total_vcpus, summary.total_memory_mb);
    
    assert_eq!(summary.total_nodes, 3);
    assert_eq!(summary.total_vcpus, 28); // 16 + 8 + 4
    assert_eq!(summary.total_memory_mb, 57344); // 32768 + 16384 + 8192
    
    // Test 3: Manual placement
    let vm2 = VmConfig {
        name: "test-vm-2".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    
    let placement2 = leader.shared_state.schedule_vm_placement(&vm2, PlacementStrategy::Manual { node_id: 2 }).await
        .expect("Should schedule VM manually");
    
    assert_eq!(placement2.selected_node_id, 2);
    assert!(placement2.reason.contains("Manual"));
    
    // Test 4: Try to place a VM that's too large
    let huge_vm = VmConfig {
        name: "huge-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 32,
        memory: 65536,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    
    let result = leader.shared_state.schedule_vm_placement(&huge_vm, PlacementStrategy::MostAvailable).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No nodes can accommodate"));
    
    println!("All placement tests passed!");
}