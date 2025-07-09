//! Integration tests for resource admission control with overcommit policies

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardError,
    raft_manager::WorkerCapabilities,
    resource_admission::{AdmissionControlConfig, ResourceAdmissionController},
    resource_management::{ClusterResourceManager, OvercommitPolicy},
    storage::{RESOURCE_POLICY_TABLE, VM_STATE_TABLE, WORKER_TABLE},
    types::{Hypervisor, VmConfig, VmState, VmStatus},
};
use chrono::Utc;
use redb::Database;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test database with required tables
fn create_test_database() -> (TempDir, Arc<Database>) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path).unwrap());
    
    // Initialize tables
    let write_txn = database.begin_write().unwrap();
    let _ = write_txn.open_table(WORKER_TABLE).unwrap();
    let _ = write_txn.open_table(VM_STATE_TABLE).unwrap();
    let _ = write_txn.open_table(RESOURCE_POLICY_TABLE).unwrap();
    write_txn.commit().unwrap();
    
    (temp_dir, database)
}

/// Helper to register a worker node
fn register_worker(database: &Arc<Database>, node_id: u64, cpu: u32, memory: u64, disk: u64) {
    let write_txn = database.begin_write().unwrap();
    let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
    
    let capabilities = WorkerCapabilities {
        cpu_cores: cpu,
        memory_mb: memory,
        disk_gb: disk,
        features: vec!["microvm".to_string()],
    };
    
    let data = bincode::serialize(&("127.0.0.1:7001", capabilities)).unwrap();
    worker_table.insert(&node_id.to_le_bytes(), data.as_slice()).unwrap();
    write_txn.commit().unwrap();
}

/// Helper to create a VM state
fn create_vm(database: &Arc<Database>, name: &str, node_id: u64, vcpus: u32, memory: u32) {
    let write_txn = database.begin_write().unwrap();
    let mut vm_table = write_txn.open_table(VM_STATE_TABLE).unwrap();
    
    let vm_config = VmConfig {
        name: name.to_string(),
        vcpus,
        memory,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    let vm_state = VmState {
        name: name.to_string(),
        config: vm_config,
        status: VmStatus::Running,
        node_id,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    let data = bincode::serialize(&vm_state).unwrap();
    vm_table.insert(name, data.as_slice()).unwrap();
    write_txn.commit().unwrap();
}

#[tokio::test]
async fn test_basic_admission_control() {
    let (_temp_dir, database) = create_test_database();
    
    // Register a node with 8 CPUs, 16GB RAM
    register_worker(&database, 1, 8, 16384, 100);
    
    // Create admission controller with default (moderate) policy
    let config = AdmissionControlConfig::default();
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // Test VM admission
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        vcpus: 4,
        memory: 8192,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    // Should succeed - within physical limits
    assert!(controller.validate_vm_admission(&vm_config, 1).await.is_ok());
}

#[tokio::test]
async fn test_overcommit_policy_enforcement() {
    let (_temp_dir, database) = create_test_database();
    
    // Register a node with 4 CPUs, 8GB RAM
    register_worker(&database, 1, 4, 8192, 100);
    
    // Create admission controller with moderate overcommit
    // Moderate: 2x CPU, 1.2x memory, 15% system reserve
    let config = AdmissionControlConfig {
        strict_mode: false,
        default_overcommit_policy: OvercommitPolicy::moderate(),
        preemptible_discount: 0.75,
        enable_system_reserve: true,
    };
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // Effective capacity with moderate policy:
    // CPU: 4 * 2.0 * 0.85 = 6.8 -> 6 vCPUs
    // Memory: 8192 * 1.2 * 0.85 = 8355 MB
    
    // Test VM that fits within overcommit limits
    let vm_config = VmConfig {
        name: "overcommit-vm".to_string(),
        vcpus: 6,
        memory: 8000,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    // Should succeed with overcommit
    assert!(controller.validate_vm_admission(&vm_config, 1).await.is_ok());
    
    // Test VM that exceeds overcommit limits
    let large_vm = VmConfig {
        name: "large-vm".to_string(),
        vcpus: 8, // Exceeds effective capacity
        memory: 8192,
        preemptible: false,
        ..vm_config
    };
    
    // Should fail
    let result = controller.validate_vm_admission(&large_vm, 1).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), BlixardError::InsufficientResources { .. }));
}

#[tokio::test]
async fn test_preemptible_vm_discount() {
    let (_temp_dir, database) = create_test_database();
    
    // Register a node with limited resources
    register_worker(&database, 1, 4, 8192, 100);
    
    // Create some existing VMs to consume resources
    create_vm(&database, "vm1", 1, 2, 4096);
    create_vm(&database, "vm2", 1, 1, 2048);
    
    // Create admission controller with preemptible discount
    let config = AdmissionControlConfig {
        strict_mode: false,
        default_overcommit_policy: OvercommitPolicy::default(),
        preemptible_discount: 0.5, // Preemptible VMs use 50% resources
        enable_system_reserve: true,
    };
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // Regular VM that would exceed capacity
    let regular_vm = VmConfig {
        name: "regular-vm".to_string(),
        vcpus: 2,
        memory: 2048,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    // Should fail - not enough resources
    let result = controller.validate_vm_admission(&regular_vm, 1).await;
    assert!(result.is_err());
    
    // Preemptible VM with same specs
    let preemptible_vm = VmConfig {
        name: "preemptible-vm".to_string(),
        preemptible: true,
        ..regular_vm
    };
    
    // Should succeed - uses only 50% of resources
    assert!(controller.validate_vm_admission(&preemptible_vm, 1).await.is_ok());
}

#[tokio::test]
async fn test_strict_mode_no_overcommit() {
    let (_temp_dir, database) = create_test_database();
    
    // Register a node
    register_worker(&database, 1, 4, 8192, 100);
    
    // Create admission controller in strict mode
    let config = AdmissionControlConfig {
        strict_mode: true, // No overcommit allowed
        default_overcommit_policy: OvercommitPolicy::conservative(),
        preemptible_discount: 1.0,
        enable_system_reserve: false,
    };
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // VM within physical limits
    let small_vm = VmConfig {
        name: "small-vm".to_string(),
        vcpus: 4,
        memory: 8192,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    // Should succeed
    assert!(controller.validate_vm_admission(&small_vm, 1).await.is_ok());
    
    // VM exceeding physical limits
    let large_vm = VmConfig {
        name: "large-vm".to_string(),
        vcpus: 5, // More than physical
        ..small_vm
    };
    
    // Should fail in strict mode
    let result = controller.validate_vm_admission(&large_vm, 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_node_specific_overcommit_policy() {
    let (_temp_dir, database) = create_test_database();
    
    // Register multiple nodes
    register_worker(&database, 1, 4, 8192, 100);
    register_worker(&database, 2, 4, 8192, 100);
    
    // Set aggressive policy for node 2
    let write_txn = database.begin_write().unwrap();
    let mut policy_table = write_txn.open_table(RESOURCE_POLICY_TABLE).unwrap();
    let aggressive_policy = OvercommitPolicy::aggressive();
    let policy_data = bincode::serialize(&aggressive_policy).unwrap();
    policy_table.insert(&2u64.to_le_bytes(), policy_data.as_slice()).unwrap();
    write_txn.commit().unwrap();
    
    // Create admission controller
    let config = AdmissionControlConfig::default();
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // Large VM that needs aggressive overcommit
    let large_vm = VmConfig {
        name: "large-vm".to_string(),
        vcpus: 12, // 3x overcommit
        memory: 10000,
        preemptible: false,
        priority: 100,
        anti_affinity: None,
        networks: vec![],
        volumes: vec![],
        nixos_modules: vec![],
        kernel: None,
        init_command: None,
        flake_modules: vec![],
        hypervisor: Hypervisor::CloudHypervisor,
        tenant_id: None,
    };
    
    // Should fail on node 1 (moderate policy)
    let result1 = controller.validate_vm_admission(&large_vm, 1).await;
    assert!(result1.is_err());
    
    // Should succeed on node 2 (aggressive policy)
    let result2 = controller.validate_vm_admission(&large_vm, 2).await;
    assert!(result2.is_ok());
}

#[tokio::test]
async fn test_resource_utilization_summary() {
    let (_temp_dir, database) = create_test_database();
    
    // Register nodes with different capacities
    register_worker(&database, 1, 8, 16384, 200);
    register_worker(&database, 2, 16, 32768, 500);
    
    // Create some VMs
    create_vm(&database, "vm1", 1, 4, 8192);
    create_vm(&database, "vm2", 1, 2, 4096);
    create_vm(&database, "vm3", 2, 8, 16384);
    
    // Create admission controller
    let config = AdmissionControlConfig::default();
    let resource_manager = Arc::new(tokio::sync::RwLock::new(
        ClusterResourceManager::new(config.default_overcommit_policy.clone())
    ));
    let controller = ResourceAdmissionController::new(database.clone(), config, resource_manager);
    
    // Get resource summary
    let summary = controller.get_resource_summary().await.unwrap();
    
    // Verify totals
    assert_eq!(summary.total_cpu, 24); // 8 + 16
    assert_eq!(summary.total_memory_mb, 49152); // 16384 + 32768
    assert_eq!(summary.total_disk_gb, 700); // 200 + 500
    
    // Verify allocations
    assert_eq!(summary.allocated_cpu, 14); // 4 + 2 + 8
    assert_eq!(summary.allocated_memory_mb, 28672); // 8192 + 4096 + 16384
    
    // Verify utilization percentages
    assert!(summary.cpu_utilization_percent > 50.0);
    assert!(summary.memory_utilization_percent > 50.0);
    
    assert_eq!(summary.node_count, 2);
}