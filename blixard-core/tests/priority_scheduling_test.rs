use blixard_core::raft_manager::WorkerCapabilities;
use blixard_core::storage::{VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE};
use blixard_core::types::{VmConfig, VmState, VmStatus};
use blixard_core::vm_scheduler::{
    PlacementStrategy, PreemptionConfig, PreemptionPolicy, VmScheduler,
};
use redb::Database;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_priority_based_placement_without_preemption() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db.clone());

    // Register a worker node
    {
        let write_txn = db.begin_write().unwrap();
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();

            let node_id = 1u64;
            let capabilities = WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            };

            let worker_data = bincode::serialize(&("node1".to_string(), capabilities)).unwrap();
            worker_table
                .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
                .unwrap();

            // Mark as online
            status_table
                .insert(node_id.to_le_bytes().as_slice(), [0u8].as_slice())
                .unwrap();
        }
        write_txn.commit().unwrap();
    }

    // Create VM configs with different priorities
    let high_priority_vm = VmConfig {
        name: "high-priority-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        priority: 900,
        preemptible: false,
        ..Default::default()
    };

    let low_priority_vm = VmConfig {
        name: "low-priority-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 2,
        memory: 4096,
        priority: 100,
        preemptible: true,
        ..Default::default()
    };

    // Test placement with priority-based strategy
    let strategy = PlacementStrategy::PriorityBased {
        base_strategy: Box::new(PlacementStrategy::MostAvailable),
        enable_preemption: false,
    };

    // Both should succeed without preemption
    let decision1 = scheduler
        .schedule_vm_placement(&high_priority_vm, strategy.clone())
        .await
        .unwrap();
    assert_eq!(decision1.target_node_id, 1);
    assert!(decision1.preemption_candidates.is_empty());

    let decision2 = scheduler
        .schedule_vm_placement(&low_priority_vm, strategy)
        .await
        .unwrap();
    assert_eq!(decision2.target_node_id, 1);
    assert!(decision2.preemption_candidates.is_empty());
}

#[tokio::test]
async fn test_priority_based_placement_with_preemption() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db.clone());

    // Register a worker node with limited resources
    {
        let write_txn = db.begin_write().unwrap();
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
            let mut vm_table = write_txn.open_table(VM_STATE_TABLE).unwrap();

            let node_id = 1u64;
            let capabilities = WorkerCapabilities {
                cpu_cores: 4,
                memory_mb: 8192,
                disk_gb: 50,
                features: vec!["microvm".to_string()],
            };

            let worker_data = bincode::serialize(&("node1".to_string(), capabilities)).unwrap();
            worker_table
                .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
                .unwrap();
            status_table
                .insert(node_id.to_le_bytes().as_slice(), [0u8].as_slice())
                .unwrap();

            // Add existing low-priority VMs that consume all resources
            let existing_vm1 = VmState {
                name: "existing-low-1".to_string(),
                config: VmConfig {
                    name: "existing-low-1".to_string(),
                    config_path: "/test".to_string(),
                    vcpus: 2,
                    memory: 4096,
                    priority: 200,
                    preemptible: true,
                    ..Default::default()
                },
                status: VmStatus::Running,
                node_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            let existing_vm2 = VmState {
                name: "existing-low-2".to_string(),
                config: VmConfig {
                    name: "existing-low-2".to_string(),
                    config_path: "/test".to_string(),
                    vcpus: 2,
                    memory: 4096,
                    priority: 100,
                    preemptible: true,
                    ..Default::default()
                },
                status: VmStatus::Running,
                node_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            vm_table
                .insert(
                    "existing-low-1",
                    bincode::serialize(&existing_vm1).unwrap().as_slice(),
                )
                .unwrap();
            vm_table
                .insert(
                    "existing-low-2",
                    bincode::serialize(&existing_vm2).unwrap().as_slice(),
                )
                .unwrap();
        }
        write_txn.commit().unwrap();
    }

    // Try to place a high-priority VM that requires preemption
    let high_priority_vm = VmConfig {
        name: "critical-vm".to_string(),
        config_path: "/test".to_string(),
        vcpus: 3,
        memory: 6144,
        priority: 800,
        preemptible: false,
        ..Default::default()
    };

    let strategy = PlacementStrategy::PriorityBased {
        base_strategy: Box::new(PlacementStrategy::MostAvailable),
        enable_preemption: true,
    };

    let decision = scheduler
        .schedule_vm_placement(&high_priority_vm, strategy)
        .await
        .unwrap();

    // Should select node 1 with preemption candidates
    assert_eq!(decision.target_node_id, 1);
    assert!(!decision.preemption_candidates.is_empty());

    // Should suggest preempting the lowest priority VMs first
    assert!(decision
        .preemption_candidates
        .iter()
        .any(|c| c.vm_name == "existing-low-2"));
    assert!(decision.reason.contains("preemption"));
}

#[tokio::test]
async fn test_preemption_policy_enforcement() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db.clone());

    // Test Never policy
    let never_policy = PreemptionPolicy::Never;
    assert!(!scheduler.is_preemption_allowed(&never_policy, 1000, 100));

    // Test LowerPriorityOnly policy
    let lower_only_policy = PreemptionPolicy::LowerPriorityOnly;
    assert!(scheduler.is_preemption_allowed(&lower_only_policy, 500, 100));
    assert!(!scheduler.is_preemption_allowed(&lower_only_policy, 100, 500));
    assert!(!scheduler.is_preemption_allowed(&lower_only_policy, 500, 500));

    // Test CostAware policy
    let cost_aware_policy = PreemptionPolicy::CostAware {
        max_cost_increase: 10.0,
    };
    assert!(scheduler.is_preemption_allowed(&cost_aware_policy, 500, 100));
    assert!(!scheduler.is_preemption_allowed(&cost_aware_policy, 100, 500));
}

#[tokio::test]
async fn test_preemption_execution() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(&db_path).unwrap());

    // Initialize tables
    {
        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(WORKER_TABLE).unwrap();
        write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
        write_txn.open_table(VM_STATE_TABLE).unwrap();
        write_txn.commit().unwrap();
    }

    let scheduler = VmScheduler::new(db);

    let candidates = vec![
        blixard_core::vm_scheduler_modules::placement_strategies::PreemptionCandidate {
            vm_name: "vm1".to_string(),
            node_id: 1,
            priority: 100,
            resources_freed: blixard_core::vm_scheduler_modules::resource_analysis::VmResourceRequirements {
                vcpus: 2,
                memory_mb: 4096,
                disk_gb: 0,
                required_features: Vec::new(),
            },
            preemption_cost: 1.0,
        },
        blixard_core::vm_scheduler_modules::placement_strategies::PreemptionCandidate {
            vm_name: "vm2".to_string(),
            node_id: 1,
            priority: 200,
            resources_freed: blixard_core::vm_scheduler_modules::resource_analysis::VmResourceRequirements {
                vcpus: 2,
                memory_mb: 4096,
                disk_gb: 0,
                required_features: Vec::new(),
            },
            preemption_cost: 1.0,
        },
    ];

    let config = PreemptionConfig {
        grace_period_secs: 30,
        try_live_migration: false,
        policy: PreemptionPolicy::LowerPriorityOnly,
    };

    // Test graceful preemption
    let preempted = scheduler
        .execute_preemption(&candidates, &config)
        .await
        .unwrap();
    assert_eq!(preempted.len(), 2);
    assert!(preempted.contains(&"vm1".to_string()));
    assert!(preempted.contains(&"vm2".to_string()));

    // Test forced preemption
    let forced_preempted = scheduler
        .execute_forced_preemption(&candidates)
        .await
        .unwrap();
    assert_eq!(forced_preempted.len(), 2);
}

#[tokio::test]
async fn test_vm_config_validation() {
    let mut vm = VmConfig::default();

    // Valid config
    vm.name = "test-vm".to_string();
    vm.vcpus = 2;
    vm.memory = 1024;
    vm.priority = 500;
    assert!(vm.validate().is_ok());

    // Invalid priority
    vm.priority = 1001;
    assert!(vm.validate().is_err());

    // Reset priority
    vm.priority = 500;

    // Empty name
    vm.name = "".to_string();
    assert!(vm.validate().is_err());

    // Zero vCPUs
    vm.name = "test-vm".to_string();
    vm.vcpus = 0;
    assert!(vm.validate().is_err());

    // Zero memory
    vm.vcpus = 2;
    vm.memory = 0;
    assert!(vm.validate().is_err());
}
