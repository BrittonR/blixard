//! Integration tests for anti-affinity scheduling

mod common;

use blixard_core::{
    anti_affinity::{AntiAffinityRule, AntiAffinityRules},
    raft_manager::WorkerCapabilities,
    storage::{VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE},
    types::{VmConfig, VmState, VmStatus},
    vm_scheduler::{PlacementStrategy, VmScheduler},
};
use redb::Database;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_scheduler() -> (VmScheduler, Arc<Database>, TempDir) {
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
    let _ = write_txn.open_table(VM_STATE_TABLE).unwrap();
    let _ = write_txn
        .open_table(blixard_core::storage::NODE_TOPOLOGY_TABLE)
        .unwrap();
    write_txn.commit().unwrap();

    let scheduler = VmScheduler::new(database.clone());
    (scheduler, database, temp_dir)
}

async fn add_worker_node(database: &Database, node_id: u64, cpu_cores: u32, memory_mb: u64) {
    let write_txn = database.begin_write().unwrap();

    // Add worker
    {
        let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
        let capabilities = WorkerCapabilities {
            cpu_cores,
            memory_mb,
            disk_gb: 100,
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

async fn add_vm(
    database: &Database,
    vm_name: &str,
    node_id: u64,
    anti_affinity_rules: Option<AntiAffinityRules>,
) {
    let write_txn = database.begin_write().unwrap();
    {
        let mut vm_table = write_txn.open_table(VM_STATE_TABLE).unwrap();

        let vm_state = VmState {
            name: vm_name.to_string(),
            config: {
                let mut config = common::test_vm_config(vm_name);
                config.anti_affinity = anti_affinity_rules;
                config
            },
            status: VmStatus::Running,
            node_id,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let vm_data = bincode::serialize(&vm_state).unwrap();
        vm_table.insert(vm_name, vm_data.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
}

#[tokio::test]
async fn test_hard_anti_affinity_constraint() {
    let (scheduler, database, _temp_dir) = create_test_scheduler().await;

    // Add three worker nodes
    add_worker_node(&database, 1, 8, 16384).await;
    add_worker_node(&database, 2, 8, 16384).await;
    add_worker_node(&database, 3, 8, 16384).await;

    // Add two VMs with "web-app" anti-affinity group on nodes 1 and 2
    let web_rules = AntiAffinityRules::new().add_rule(AntiAffinityRule::hard("web-app"));

    add_vm(&database, "web1", 1, Some(web_rules.clone())).await;
    add_vm(&database, "web2", 2, Some(web_rules.clone())).await;

    // Try to schedule a third VM with the same anti-affinity group
    let new_vm_config = {
        let mut config = common::test_vm_config("web3");
        config.anti_affinity = Some(web_rules);
        config
    };

    // Should place on node 3 (only available node)
    let decision = scheduler
        .schedule_vm_placement(&new_vm_config, PlacementStrategy::MostAvailable)
        .await
        .unwrap();

    assert_eq!(decision.target_node_id, 3);
    assert!(decision.reason.contains("anti-affinity rules applied"));
}

#[tokio::test]
async fn test_soft_anti_affinity_preference() {
    let (scheduler, database, _temp_dir) = create_test_scheduler().await;

    // Add two worker nodes
    add_worker_node(&database, 1, 8, 16384).await;
    add_worker_node(&database, 2, 8, 16384).await;

    // Add one VM with soft anti-affinity on node 1
    let cache_rules = AntiAffinityRules::new().add_rule(AntiAffinityRule::soft("cache-layer", 1.0));

    add_vm(&database, "cache1", 1, Some(cache_rules.clone())).await;

    // Schedule another VM with same soft anti-affinity
    let new_vm_config = {
        let mut config = common::test_vm_config("cache2");
        config.vcpus = 2;
        config.memory = 1024;
        config.anti_affinity = Some(cache_rules);
        config
    };

    // Should prefer node 2 due to soft anti-affinity
    let decision = scheduler
        .schedule_vm_placement(&new_vm_config, PlacementStrategy::MostAvailable)
        .await
        .unwrap();

    assert_eq!(decision.target_node_id, 2);
}

#[tokio::test]
async fn test_anti_affinity_with_max_per_node() {
    let (scheduler, database, _temp_dir) = create_test_scheduler().await;

    // Add two worker nodes
    add_worker_node(&database, 1, 16, 32768).await;
    add_worker_node(&database, 2, 16, 32768).await;

    // Create anti-affinity rule allowing 2 VMs per node
    let db_rules =
        AntiAffinityRules::new().add_rule(AntiAffinityRule::hard("database").with_max_per_node(2));

    // Add first two VMs on node 1
    add_vm(&database, "db1", 1, Some(db_rules.clone())).await;
    add_vm(&database, "db2", 1, Some(db_rules.clone())).await;

    // Third VM should still be allowed on node 2
    let new_vm_config = {
        let mut config = common::test_vm_config("db3");
        config.vcpus = 2;
        config.memory = 1024;
        config.anti_affinity = Some(db_rules.clone());
        config
    };

    let decision = scheduler
        .schedule_vm_placement(&new_vm_config, PlacementStrategy::RoundRobin)
        .await
        .unwrap();

    // Can be placed on either node since max_per_node=2
    assert!(decision.target_node_id == 1 || decision.target_node_id == 2);

    // Add one more VM to reach the limit on node 2
    add_vm(&database, "db3", 2, Some(db_rules.clone())).await;
    add_vm(&database, "db4", 2, Some(db_rules.clone())).await;

    // Now both nodes are at capacity for this anti-affinity group
    let full_vm_config = {
        let mut config = common::test_vm_config("db5");
        config.vcpus = 2;
        config.memory = 1024;
        config.anti_affinity = Some(db_rules);
        config
    };

    // Should fail - both nodes at max capacity
    let result = scheduler
        .schedule_vm_placement(&full_vm_config, PlacementStrategy::MostAvailable)
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("No nodes can accommodate"));
}

#[tokio::test]
async fn test_manual_placement_with_anti_affinity() {
    let (scheduler, database, _temp_dir) = create_test_scheduler().await;

    // Add two worker nodes
    add_worker_node(&database, 1, 8, 16384).await;
    add_worker_node(&database, 2, 8, 16384).await;

    // Add VM with anti-affinity on node 1
    let api_rules = AntiAffinityRules::new().add_rule(AntiAffinityRule::hard("api-server"));

    add_vm(&database, "api1", 1, Some(api_rules.clone())).await;

    // Try manual placement on node 1 (should fail)
    let new_vm_config = {
        let mut config = common::test_vm_config("api2");
        config.vcpus = 2;
        config.memory = 1024;
        config.anti_affinity = Some(api_rules.clone());
        config
    };

    let result = scheduler
        .schedule_vm_placement(&new_vm_config, PlacementStrategy::Manual { node_id: 1 })
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("violates anti-affinity"));

    // Manual placement on node 2 should succeed
    let decision = scheduler
        .schedule_vm_placement(&new_vm_config, PlacementStrategy::Manual { node_id: 2 })
        .await
        .unwrap();

    assert_eq!(decision.target_node_id, 2);
}

#[tokio::test]
async fn test_no_anti_affinity_rules() {
    let (scheduler, database, _temp_dir) = create_test_scheduler().await;

    // Add worker nodes
    add_worker_node(&database, 1, 8, 16384).await;
    add_worker_node(&database, 2, 8, 16384).await;

    // VM without anti-affinity rules
    let vm_config = {
        let mut config = common::test_vm_config("regular-vm");
        config.vcpus = 2;
        config.memory = 1024;
        config.anti_affinity = None; // No anti-affinity rules
        config
    };

    // Should schedule normally without anti-affinity considerations
    let decision = scheduler
        .schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable)
        .await
        .unwrap();

    // Should place on either node
    assert!(decision.target_node_id == 1 || decision.target_node_id == 2);
    assert!(!decision.reason.contains("anti-affinity")); // No mention of anti-affinity
}
