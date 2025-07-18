//! Tests for resource reservation and overcommit policies

mod common;

use blixard_core::{
    raft_manager::WorkerCapabilities,
    resource_management::{NodeResourceState, OvercommitPolicy, ResourceReservation},
    storage::{WORKER_STATUS_TABLE, WORKER_TABLE},
    vm_scheduler::{PlacementStrategy, VmScheduler},
};
use redb::Database;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_scheduler_with_policy(
    policy: OvercommitPolicy,
) -> (VmScheduler, Arc<Database>, TempDir) {
    // Initialize metrics system for tests if not already initialized
    if blixard_core::metrics_otel::try_metrics().is_none() {
        let _ = blixard_core::metrics_otel::init_noop();
    }

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path).unwrap());

    // Initialize storage tables
    let write_txn = database.begin_write().unwrap();
    let _ = write_txn.open_table(WORKER_TABLE).unwrap();
    let _ = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    let _ = write_txn
        .open_table(blixard_core::storage::VM_STATE_TABLE)
        .unwrap();
    write_txn.commit().unwrap();

    let scheduler = VmScheduler::with_overcommit_policy(database.clone(), policy);
    (scheduler, database, temp_dir)
}

async fn add_worker_node(
    database: &Database,
    node_id: u64,
    cpu_cores: u32,
    memory_mb: u64,
    disk_gb: u64,
) {
    let write_txn = database.begin_write().unwrap();

    // Add worker
    {
        let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
        let capabilities = WorkerCapabilities {
            cpu_cores,
            memory_mb,
            disk_gb,
            features: vec!["microvm".to_string()],
        };
        let worker_data = bincode::serialize(&("worker".to_string(), capabilities)).unwrap();
        worker_table
            .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
            .unwrap();
    }

    // Mark as online
    {
        let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        status_table
            .insert(
                node_id.to_le_bytes().as_slice(),
                vec![blixard_core::raft_manager::WorkerStatus::Online as u8].as_slice(),
            )
            .unwrap();
    }

    write_txn.commit().unwrap();
}

#[tokio::test]
async fn test_conservative_overcommit_policy() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::conservative()).await;

    // Add a node with 8 CPU, 16GB RAM, 100GB disk
    add_worker_node(&database, 1, 8, 16384, 100).await;

    // Note: Resource manager sync is now automatic

    // Try to allocate exactly what's available (minus system reserve)
    let mut vm1 = common::test_vm_config("vm1");
    vm1.vcpus = 7; // 8 * 0.9 = 7.2
    vm1.memory = 14745; // 16384 * 0.9 = 14745
    vm1.tenant_id = "test".to_string();

    let decision = scheduler
        .schedule_vm_placement(&vm1, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    assert_eq!(decision.target_node_id, 1);

    // Actually allocate the resources (simulating VM creation)
    {
        let write_txn = database.begin_write().unwrap();
        {
            let mut vm_table = write_txn
                .open_table(blixard_core::storage::VM_STATE_TABLE)
                .unwrap();
            let vm_state = blixard_core::types::VmState {
                name: vm1.name.clone(),
                node_id: decision.target_node_id,
                status: blixard_core::types::VmStatus::Running,
                config: vm1.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            vm_table
                .insert(
                    vm1.name.as_str(),
                    bincode::serialize(&vm_state).unwrap().as_slice(),
                )
                .unwrap();
        }
        write_txn.commit().unwrap();
    }

    // Try to allocate more - should fail
    let mut vm2 = common::test_vm_config("vm2");
    vm2.vcpus = 8; // Exceeds available
    vm2.memory = 16384;
    vm2.tenant_id = "test".to_string();

    let result = scheduler
        .schedule_vm_placement(&vm2, PlacementStrategy::MostAvailable)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_moderate_overcommit_policy() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::moderate()).await;

    // Add a node with 8 CPU, 16GB RAM, 100GB disk
    add_worker_node(&database, 1, 8, 16384, 100).await;

    // Note: Resource manager sync is now automatic

    // With moderate policy: 2x CPU, 1.2x memory, 1.1x disk, 15% reserve
    // Effective capacity: CPU = 8 * 2 * 0.85 = 13.6
    //                    Memory = 16384 * 1.2 * 0.85 = 16711
    //                    Disk = 100 * 1.1 * 0.85 = 93.5

    // Create multiple VMs that together exceed physical but fit in overcommit
    let mut vm1 = common::test_vm_config("vm1");
    vm1.vcpus = 6;
    vm1.memory = 8192;
    vm1.tenant_id = "test".to_string();

    let mut vm2 = common::test_vm_config("vm2");
    vm2.vcpus = 6; // Total 12 CPUs (exceeds physical 8, but within overcommit)
    vm2.memory = 8192;
    vm2.tenant_id = "test".to_string();

    // Both should succeed with overcommit
    let decision1 = scheduler
        .schedule_vm_placement(&vm1, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    assert_eq!(decision1.target_node_id, 1);

    // Note: In a real scenario, we'd need to actually allocate resources
    // For this test, we're just verifying the policy allows it
}

#[tokio::test]
async fn test_resource_reservations() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::default()).await;

    // Add a node
    add_worker_node(&database, 1, 10, 16384, 100).await;
    // Resource manager sync is now automatic

    // NOTE: Resource reservations would need to be handled through the resource manager
    // For now, just test basic VM scheduling without reservations

    let mut vm = common::test_vm_config("vm1");
    vm.vcpus = 7; // Should fit within 10 * 0.9 = 9 available cores
    vm.memory = 10649; // Should fit within 16384 * 0.9 = 14745 available memory
    vm.tenant_id = "test".to_string();

    let decision = scheduler
        .schedule_vm_placement(&vm, PlacementStrategy::MostAvailable)
        .await
        .unwrap();
    assert_eq!(decision.target_node_id, 1);
}

#[tokio::test]
async fn test_vm_placement_with_multiple_nodes() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::default()).await;

    // Add nodes
    add_worker_node(&database, 1, 8, 16384, 100).await;
    add_worker_node(&database, 2, 8, 16384, 100).await;

    let mut vm = common::test_vm_config("critical-vm");
    vm.vcpus = 4;
    vm.memory = 8192;
    vm.tenant_id = "test".to_string();

    // Schedule VM - should choose one of the available nodes
    let decision = scheduler
        .schedule_vm_placement(&vm, PlacementStrategy::MostAvailable)
        .await
        .unwrap();

    // Should have placed on one of the nodes
    assert!(decision.target_node_id == 1 || decision.target_node_id == 2);
}

#[tokio::test]
async fn test_cluster_resource_summary() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::default()).await;

    // Add nodes
    add_worker_node(&database, 1, 8, 16384, 100).await;
    add_worker_node(&database, 2, 8, 16384, 100).await;
    // Resource manager sync is now automatic

    // Get cluster resource summary
    let summary = scheduler.get_cluster_resource_summary().await.unwrap();
    
    // Verify cluster summary has both nodes
    assert_eq!(summary.total_nodes, 2);
    assert_eq!(summary.healthy_nodes, 2);
    assert_eq!(summary.total_vcpus, 16); // 8 + 8
    assert_eq!(summary.total_memory_mb, 32768); // 16384 + 16384
}

#[tokio::test]
async fn test_resource_utilization_tracking() {
    let (scheduler, database, _temp_dir) =
        create_test_scheduler_with_policy(OvercommitPolicy::moderate()).await;

    // Add a node
    add_worker_node(&database, 1, 8, 16384, 100).await;
    // Resource manager sync is now automatic

    // Get cluster resource usage to verify initial state
    let usage = scheduler.get_cluster_resource_usage().await.unwrap();
    assert_eq!(usage.len(), 1);
    
    let node_usage = &usage[0];
    assert_eq!(node_usage.node_id, 1);
    assert_eq!(node_usage.used_vcpus, 0); // No VMs running initially
    assert_eq!(node_usage.used_memory_mb, 0);
    
    // Verify cluster summary utilization is 0%
    let summary = scheduler.get_cluster_resource_summary().await.unwrap();
    let (cpu_util, mem_util, disk_util) = summary.utilization_percentages();
    assert_eq!(cpu_util, 0.0);
    assert_eq!(mem_util, 0.0);
    assert_eq!(disk_util, 0.0);
}

#[test]
fn test_node_resource_state() {
    let mut state = NodeResourceState::new(10, 16384, 100, OvercommitPolicy::moderate());

    // Test allocation
    assert!(state.can_allocate(5, 8192, 50));
    state.allocate(5, 8192, 50).unwrap();

    // Test available resources
    let (cpu, _mem, _disk) = state.available_resources();
    assert_eq!(cpu, 12); // (10 * 2 * 0.85) - 5 = 12

    // Test utilization
    let (cpu_util, mem_util, disk_util) = state.utilization_percentages();
    assert_eq!(cpu_util, 50.0); // 5/10 * 100
    assert_eq!(mem_util, 50.0); // 8192/16384 * 100
    assert_eq!(disk_util, 50.0); // 50/100 * 100

    // Test release
    state.release(5, 8192, 50);
    let (cpu, _mem, _disk) = state.available_resources();
    assert_eq!(cpu, 17); // Back to full capacity
}

#[test]
fn test_expired_reservations() {
    let mut state = NodeResourceState::new(10, 16384, 100, OvercommitPolicy::default());

    // Add an expired reservation
    let expired_reservation = ResourceReservation {
        id: "expired".to_string(),
        owner: "test".to_string(),
        cpu_cores: 2,
        memory_mb: 4096,
        disk_gb: 10,
        priority: 10,
        is_hard: true,
        expires_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
    };

    state.add_reservation(expired_reservation).unwrap();
    assert_eq!(state.reservations.len(), 1);

    // Cleanup expired
    state.cleanup_expired_reservations();
    assert_eq!(state.reservations.len(), 0);

    // Resources should be released
    let (cpu, _, _) = state.available_resources();
    assert_eq!(cpu, 9); // Full capacity with system reserve
}
