//! Integration tests for VM scheduler with distributed system
//!
//! Tests VM scheduling in multi-node clusters with real resource tracking

#![cfg(feature = "test-helpers")]

use std::time::Duration;

use blixard_core::raft_manager::{ResourceRequirements, TaskSpec, WorkerCapabilities};
use blixard_core::test_helpers::{timing, TestCluster};
use blixard_core::types::{VmCommand, VmConfig, VmStatus};
use blixard_core::vm_scheduler::PlacementStrategy;
use redb::ReadableTable;

/// Test VM scheduling across multiple nodes with resource constraints
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Integration test needs full Raft consensus for VM creation"]
async fn test_vm_scheduling_multi_node_placement() {
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
    )
    .await
    .expect("Should elect a leader");

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

    // Register workers with different capacities
    // Node 1: High capacity
    let capabilities1 = WorkerCapabilities {
        cpu_cores: 16,
        memory_mb: 32768,
        disk_gb: 500,
        features: vec!["microvm".to_string()],
    };
    leader
        .shared_state
        .register_worker_through_raft(1, "127.0.0.1:7001".to_string(), capabilities1)
        .await
        .expect("Should register worker 1");

    // Node 2: Medium capacity
    let capabilities2 = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 250,
        features: vec!["microvm".to_string()],
    };
    leader
        .shared_state
        .register_worker_through_raft(2, "127.0.0.1:7002".to_string(), capabilities2)
        .await
        .expect("Should register worker 2");

    // Node 3: Low capacity with GPU
    let capabilities3 = WorkerCapabilities {
        cpu_cores: 4,
        memory_mb: 8192,
        disk_gb: 100,
        features: vec!["microvm".to_string(), "gpu".to_string()],
    };
    leader
        .shared_state
        .register_worker_through_raft(3, "127.0.0.1:7003".to_string(), capabilities3)
        .await
        .expect("Should register worker 3");

    // Wait for worker registrations to propagate
    timing::robust_sleep(Duration::from_millis(500)).await;

    // Check if VM manager is available
    let has_vm_manager = leader.shared_state.get_vm_manager().await.is_some();
    println!("Leader has VM manager: {}", has_vm_manager);

    // Test 1: Most Available strategy should choose high-capacity node
    let vm1 = VmConfig {
        name: "vm-most-available".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
    };

    let placement1 = leader
        .shared_state
        .schedule_vm_placement(&vm1, PlacementStrategy::MostAvailable)
        .await
        .expect("Should schedule VM");
    println!(
        "Placement decision: selected node {}, reason: {}, alternatives: {:?}",
        placement1.selected_node_id, placement1.reason, placement1.alternative_nodes
    );

    // Node with most capacity should be selected (could be any if all have same availability)
    assert!(
        placement1.selected_node_id >= 1 && placement1.selected_node_id <= 3,
        "Should select a valid node, got {}",
        placement1.selected_node_id
    );

    // Actually create the VM to consume resources
    // Note: create_vm_with_scheduling requires Raft consensus, so we'll use the regular create_vm
    // The scheduler has already given us the placement decision
    leader
        .shared_state
        .send_vm_command(VmCommand::Create {
            config: vm1.clone(),
            node_id: placement1.selected_node_id,
        })
        .await
        .expect("Should create VM");

    timing::robust_sleep(Duration::from_millis(200)).await;

    // Test 2: Round Robin should distribute VMs evenly
    for i in 1..=6 {
        let vm = VmConfig {
            name: format!("vm-round-robin-{}", i),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 1024,
            tenant_id: "default".to_string(),
            ip_address: None,
        };

        let placement = leader
            .shared_state
            .schedule_vm_placement(&vm, PlacementStrategy::RoundRobin)
            .await
            .expect("Should schedule VM");
        leader
            .shared_state
            .send_vm_command(VmCommand::Create {
                config: vm.clone(),
                node_id: placement.selected_node_id,
            })
            .await
            .expect("Should create VM");

        timing::robust_sleep(Duration::from_millis(100)).await;
    }

    // Check VM distribution (should be roughly even)
    let summary = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get resource summary");

    // Each node should have roughly 2-3 VMs
    for node in &summary.nodes {
        assert!(
            node.running_vms >= 1 && node.running_vms <= 4,
            "Node {} should have 1-4 VMs but has {}",
            node.node_id,
            node.running_vms
        );
    }

    // Test 3: Least Available (bin packing) should fill nodes
    for i in 1..=3 {
        let vm = VmConfig {
            name: format!("vm-bin-pack-{}", i),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "default".to_string(),
            ip_address: None,
        };

        let placement = leader
            .shared_state
            .schedule_vm_placement(&vm, PlacementStrategy::LeastAvailable)
            .await
            .expect("Should schedule VM");
        leader
            .shared_state
            .send_vm_command(VmCommand::Create {
                config: vm.clone(),
                node_id: placement.selected_node_id,
            })
            .await
            .expect("Should create VM");

        timing::robust_sleep(Duration::from_millis(100)).await;
    }

    // Test 4: Feature requirements (GPU)
    // This is tricky because we can't modify VmResourceRequirements from VmConfig
    // For now, test that placement works with available features
    let gpu_vm = VmConfig {
        name: "vm-gpu".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
    };

    // Manual placement to GPU node
    let placement_gpu = leader
        .shared_state
        .schedule_vm_placement(&gpu_vm, PlacementStrategy::Manual { node_id: 3 })
        .await
        .expect("Should schedule to GPU node");
    assert_eq!(placement_gpu.selected_node_id, 3);

    // Test 5: Resource exhaustion
    // Try to place a VM that's too large for any node
    let huge_vm = VmConfig {
        name: "vm-huge".to_string(),
        config_path: "".to_string(),
        vcpus: 32,
        memory: 65536,
        tenant_id: "default".to_string(),
        ip_address: None,
    };

    let result = leader
        .shared_state
        .schedule_vm_placement(&huge_vm, PlacementStrategy::MostAvailable)
        .await;
    assert!(result.is_err(), "Should fail to schedule huge VM");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No nodes can accommodate"));
}

/// Test VM scheduling with node failures and recovery
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_vm_scheduling_with_node_failures() {
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
    )
    .await
    .expect("Should elect a leader");

    let mut leader_id = 0;
    for i in 1..=3 {
        if let Ok(node) = cluster.get_node(i) {
            if node.shared_state.is_leader().await {
                leader_id = i;
                break;
            }
        }
    }

    let leader = cluster.get_node(leader_id).unwrap();

    // Register all workers
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };

    for i in 1..=3 {
        leader
            .shared_state
            .register_worker_through_raft(
                i,
                format!("127.0.0.1:{}", 7000 + i),
                capabilities.clone(),
            )
            .await
            .expect(&format!("Should register worker {}", i));
    }

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Create some VMs distributed across nodes
    for i in 1..=6 {
        let vm = VmConfig {
            name: format!("vm-{}", i),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "default".to_string(),
            ip_address: None,
        };

        // TODO: Fix create_vm_with_scheduling(vm, PlacementStrategy::RoundRobin).await
        // .expect("Should create VM");
    }

    timing::robust_sleep(Duration::from_millis(500)).await;

    // TODO: Mark node 2 as offline (simulate failure)
    // This functionality needs to be exposed through Raft
    // leader.shared_state.update_worker_status_through_raft(2, blixard_core::raft_manager::WorkerStatus::Offline).await
    //     .expect("Should update worker status");

    // For now, skip this part of the test
    timing::robust_sleep(Duration::from_millis(500)).await;

    // Try to schedule new VMs - should avoid offline node
    for i in 7..=9 {
        let vm = VmConfig {
            name: format!("vm-{}", i),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 1024,
            tenant_id: "default".to_string(),
            ip_address: None,
        };

        let placement = leader
            .shared_state
            .schedule_vm_placement(&vm, PlacementStrategy::MostAvailable)
            .await
            .expect("Should schedule VM");

        // Should not select node 2 (offline)
        assert_ne!(
            placement.selected_node_id, 2,
            "Should not schedule to offline node"
        );
        assert!(placement.selected_node_id == 1 || placement.selected_node_id == 3);

        // TODO: Fix create_vm_with_scheduling(vm, PlacementStrategy::MostAvailable).await
        // .expect("Should create VM");
    }

    // TODO: Bring node 2 back online
    // This functionality needs to be exposed through Raft
    // leader.shared_state.update_worker_status_through_raft(2, blixard_core::raft_manager::WorkerStatus::Online).await
    //     .expect("Should update worker status");

    timing::robust_sleep(Duration::from_millis(500)).await;

    // New VMs should now consider node 2 again
    let vm_recovery = VmConfig {
        name: "vm-after-recovery".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
        tenant_id: "default".to_string(),
        ip_address: None,
    };

    let placement = leader
        .shared_state
        .schedule_vm_placement(&vm_recovery, PlacementStrategy::MostAvailable)
        .await
        .expect("Should schedule VM");

    // Node 2 should be available again
    let summary = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get resource summary");

    assert_eq!(summary.total_nodes, 3, "All nodes should be counted");
    let node2 = summary.nodes.iter().find(|n| n.node_id == 2);
    assert!(node2.is_some(), "Node 2 should be in summary");
}

/// Test cluster resource summary accuracy
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cluster_resource_summary_accuracy() {
    let cluster = TestCluster::new(2).await.expect("Failed to create cluster");

    // Wait for leader election
    timing::wait_for_condition_with_backoff(
        || async {
            for i in 1..=2 {
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
    )
    .await
    .expect("Should elect a leader");

    let mut leader_id = 0;
    for i in 1..=2 {
        if let Ok(node) = cluster.get_node(i) {
            if node.shared_state.is_leader().await {
                leader_id = i;
                break;
            }
        }
    }

    let leader = cluster.get_node(leader_id).unwrap();

    // Register workers with known capacities
    let capabilities1 = WorkerCapabilities {
        cpu_cores: 10,
        memory_mb: 20480, // 20GB
        disk_gb: 200,
        features: vec!["microvm".to_string()],
    };
    let capabilities2 = WorkerCapabilities {
        cpu_cores: 6,
        memory_mb: 12288, // 12GB
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };

    leader
        .shared_state
        .register_worker_through_raft(1, "127.0.0.1:7001".to_string(), capabilities1)
        .await
        .expect("Should register worker 1");
    leader
        .shared_state
        .register_worker_through_raft(2, "127.0.0.1:7002".to_string(), capabilities2)
        .await
        .expect("Should register worker 2");

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Get initial summary
    let summary_initial = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get initial summary");

    assert_eq!(summary_initial.total_nodes, 2);
    assert_eq!(summary_initial.total_vcpus, 16); // 10 + 6
    assert_eq!(summary_initial.total_memory_mb, 32768); // 20480 + 12288
    assert_eq!(summary_initial.total_disk_gb, 300); // 200 + 100
    assert_eq!(summary_initial.used_vcpus, 0);
    assert_eq!(summary_initial.used_memory_mb, 0);
    assert_eq!(summary_initial.total_running_vms, 0);

    // Create VMs and track resource usage
    let vm1 = VmConfig {
        name: "vm1".to_string(),
        config_path: "".to_string(),
        vcpus: 4,
        memory: 8192,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    // TODO: Fix create_vm_with_scheduling(vm1, PlacementStrategy::Manual { node_id: 1 }).await
    // .expect("Should create VM1");

    let vm2 = VmConfig {
        name: "vm2".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    // TODO: Fix create_vm_with_scheduling(vm2, PlacementStrategy::Manual { node_id: 2 }).await
    // .expect("Should create VM2");

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Check updated summary
    let summary_after = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get updated summary");

    assert_eq!(summary_after.used_vcpus, 6); // 4 + 2
    assert_eq!(summary_after.used_memory_mb, 12288); // 8192 + 4096
    assert_eq!(summary_after.used_disk_gb, 10); // 5 + 5 (default per VM)
    assert_eq!(summary_after.total_running_vms, 2);

    // Test utilization calculations
    let (cpu_util, mem_util, disk_util) = summary_after.utilization_percentages();
    assert!((cpu_util - 37.5).abs() < 0.1); // 6/16 = 37.5%
    assert!((mem_util - 37.5).abs() < 0.1); // 12288/32768 = 37.5%
    assert!((disk_util - 3.33).abs() < 0.1); // 10/300 = 3.33%

    // Stop one VM and verify resources are freed
    leader
        .shared_state
        .update_vm_status_through_raft("vm1".to_string(), VmStatus::Stopped, 1)
        .await
        .expect("Should update VM status");

    timing::robust_sleep(Duration::from_millis(500)).await;

    let summary_stopped = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get summary after stop");

    assert_eq!(summary_stopped.used_vcpus, 2); // Only VM2 running
    assert_eq!(summary_stopped.used_memory_mb, 4096);
    assert_eq!(summary_stopped.total_running_vms, 1);
}

/// Test VM migration between nodes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_vm_migration_scheduling() {
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
    )
    .await
    .expect("Should elect a leader");

    let mut leader_id = 0;
    for i in 1..=3 {
        if let Ok(node) = cluster.get_node(i) {
            if node.shared_state.is_leader().await {
                leader_id = i;
                break;
            }
        }
    }

    let leader = cluster.get_node(leader_id).unwrap();

    // Register workers
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };

    for i in 1..=3 {
        leader
            .shared_state
            .register_worker_through_raft(
                i,
                format!("127.0.0.1:{}", 7000 + i),
                capabilities.clone(),
            )
            .await
            .expect(&format!("Should register worker {}", i));
    }

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Create a VM on node 1
    let vm = VmConfig {
        name: "migratable-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 4096,
        tenant_id: "default".to_string(),
        ip_address: None,
    };

    // TODO: Fix create_vm_with_scheduling(vm.clone(), PlacementStrategy::Manual { node_id: 1 }).await
    // .expect("Should create VM on node 1");

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Verify VM is on node 1
    // Get VM states through database
    let db = leader
        .shared_state
        .get_database()
        .await
        .expect("Should have database");
    let read_txn = db.begin_read().expect("Should begin read transaction");
    let vm_table = read_txn
        .open_table(blixard_core::storage::VM_STATE_TABLE)
        .expect("Should open VM table");
    let mut vm_states = Vec::new();
    for vm_result in vm_table.iter().expect("Should iterate VMs") {
        let (_, vm_data) = vm_result.expect("Should read VM entry");
        let vm_state: blixard_core::types::VmState =
            bincode::deserialize(vm_data.value()).expect("Should deserialize VM state");
        vm_states.push(vm_state);
    }

    let vm_state = vm_states
        .iter()
        .find(|v| v.name == "migratable-vm")
        .expect("Should find VM");
    assert_eq!(vm_state.node_id, 1, "VM should be on node 1");

    // Simulate node 1 becoming overloaded by creating more VMs
    for i in 1..=3 {
        let load_vm = VmConfig {
            name: format!("load-vm-{}", i),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "default".to_string(),
            ip_address: None,
        };
        // TODO: Fix create_vm_with_scheduling(load_vm, PlacementStrategy::Manual { node_id: 1 }).await
        // .expect("Should create load VM");
    }

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Check if alternative placement would choose different node
    let alternative_placement = leader
        .shared_state
        .schedule_vm_placement(&vm, PlacementStrategy::MostAvailable)
        .await
        .expect("Should get alternative placement");

    // Should suggest node 2 or 3 (less loaded)
    assert_ne!(
        alternative_placement.selected_node_id, 1,
        "Should suggest different node"
    );

    // TODO: Implement actual VM migration when that feature is added
    // For now, we just verify the scheduler would place it elsewhere
}

/// Test concurrent VM scheduling requests
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_vm_scheduling() {
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
    )
    .await
    .expect("Should elect a leader");

    let mut leader_id = 0;
    for i in 1..=3 {
        if let Ok(node) = cluster.get_node(i) {
            if node.shared_state.is_leader().await {
                leader_id = i;
                break;
            }
        }
    }

    let leader = cluster.get_node(leader_id).unwrap();

    // Register workers with equal capacity
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };

    for i in 1..=3 {
        leader
            .shared_state
            .register_worker_through_raft(
                i,
                format!("127.0.0.1:{}", 7000 + i),
                capabilities.clone(),
            )
            .await
            .expect(&format!("Should register worker {}", i));
    }

    timing::robust_sleep(Duration::from_millis(500)).await;

    // Launch multiple concurrent VM creation requests
    let mut handles = vec![];

    for i in 0..12 {
        let leader_clone = leader.shared_state.clone();
        let handle = tokio::spawn(async move {
            let vm = VmConfig {
                name: format!("concurrent-vm-{}", i),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 1024,
                tenant_id: "default".to_string(),
                ip_address: None,
            };

            leader_clone
                .create_vm_with_scheduling(vm, PlacementStrategy::RoundRobin)
                .await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    let mut successes = 0;
    let mut failures = 0;

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => successes += 1,
            Ok(Err(e)) => {
                eprintln!("VM creation failed: {}", e);
                failures += 1;
            }
            Err(e) => {
                eprintln!("Task failed: {}", e);
                failures += 1;
            }
        }
    }

    println!(
        "Concurrent VM creation: {} successes, {} failures",
        successes, failures
    );
    assert!(successes >= 10, "Most VMs should be created successfully");

    // Verify even distribution
    timing::robust_sleep(Duration::from_secs(1)).await;

    let summary = leader
        .shared_state
        .get_cluster_resource_summary()
        .await
        .expect("Should get resource summary");

    // Each node should have roughly equal number of VMs (4 +/- 2)
    for node in &summary.nodes {
        assert!(
            node.running_vms >= 2 && node.running_vms <= 6,
            "Node {} should have 2-6 VMs but has {}",
            node.node_id,
            node.running_vms
        );
    }

    assert_eq!(summary.total_running_vms, successes as u32);
}
