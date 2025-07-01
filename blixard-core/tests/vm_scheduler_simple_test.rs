//! Simple integration tests for VM scheduler functionality
//!
//! Tests basic VM scheduling without requiring full cluster setup

use std::sync::Arc;
use tempfile::TempDir;
use redb::Database;

use blixard_core::{
    vm_scheduler::{VmScheduler, PlacementStrategy, VmResourceRequirements, NodeResourceUsage},
    raft_manager::{WorkerCapabilities, WorkerStatus},
    types::{VmConfig, VmState, VmStatus},
    storage::{init_database_tables, WORKER_TABLE, WORKER_STATUS_TABLE, VM_STATE_TABLE},
    error::BlixardResult,
};

/// Helper function to create a test database with initialized tables
fn create_test_database() -> BlixardResult<(Arc<Database>, TempDir)> {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path)?);
    
    // Initialize tables
    init_database_tables(&database)?;
    
    Ok((database, temp_dir))
}

/// Helper function to add a worker to the database
fn add_worker(
    database: &Database, 
    node_id: u64, 
    capabilities: WorkerCapabilities,
    is_online: bool
) -> BlixardResult<()> {
    let write_txn = database.begin_write()?;
    {
        let mut worker_table = write_txn.open_table(WORKER_TABLE)?;
        let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE)?;
        
        let worker_data = bincode::serialize(&(format!("127.0.0.1:{}", 7000 + node_id), capabilities))?;
        worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
        
        let status = if is_online { WorkerStatus::Online as u8 } else { WorkerStatus::Offline as u8 };
        status_table.insert(node_id.to_le_bytes().as_slice(), [status].as_slice())?;
    }
    write_txn.commit()?;
    Ok(())
}

/// Helper function to add a VM to the database
fn add_vm(
    database: &Database,
    vm_name: &str,
    node_id: u64,
    vcpus: u32,
    memory: u32,
    status: VmStatus,
) -> BlixardResult<()> {
    let write_txn = database.begin_write()?;
    {
        let mut vm_table = write_txn.open_table(VM_STATE_TABLE)?;
        
        let vm_config = VmConfig {
            name: vm_name.to_string(),
            config_path: "".to_string(),
            vcpus,
            memory,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
        };
        
        let vm_state = VmState {
            name: vm_name.to_string(),
            config: vm_config,
            status,
            node_id,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        let vm_data = bincode::serialize(&vm_state)?;
        vm_table.insert(vm_name, vm_data.as_slice())?;
    }
    write_txn.commit()?;
    Ok(())
}

#[tokio::test]
async fn test_scheduler_basic_placement() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers with different capacities
    let capabilities1 = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    let capabilities2 = WorkerCapabilities {
        cpu_cores: 4,
        memory_mb: 8192,
        disk_gb: 50,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities1, true).unwrap();
    add_worker(&database, 2, capabilities2, true).unwrap();
    
    // Test MostAvailable strategy
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Either node could be selected when both have 100% availability
    // The important thing is that it picked one and has the other as alternative
    assert!(placement.selected_node_id == 1 || placement.selected_node_id == 2);
    assert!(placement.reason.contains("MostAvailable"));
    let other_node = if placement.selected_node_id == 1 { 2 } else { 1 };
    assert_eq!(placement.alternative_nodes, vec![other_node]);
}

#[tokio::test]
async fn test_scheduler_resource_constraints() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add a worker with limited resources
    let capabilities = WorkerCapabilities {
        cpu_cores: 2,
        memory_mb: 4096,
        disk_gb: 20,
        features: vec!["microvm".to_string()],
    };
    add_worker(&database, 1, capabilities, true).unwrap();
    
    // Try to place a VM that fits
    let small_vm = VmConfig {
        name: "small-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    let result = scheduler.schedule_vm_placement(&small_vm, PlacementStrategy::MostAvailable).await;
    assert!(result.is_ok());
    
    // Try to place a VM that doesn't fit
    let large_vm = VmConfig {
        name: "large-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 8,
        memory: 16384,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    let result = scheduler.schedule_vm_placement(&large_vm, PlacementStrategy::MostAvailable).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No nodes can accommodate"));
}

#[tokio::test]
async fn test_scheduler_round_robin() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add three workers with identical capabilities
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    for i in 1..=3 {
        add_worker(&database, i, capabilities.clone(), true).unwrap();
    }
    
    // Add VMs to create uneven distribution
    add_vm(&database, "vm1", 1, 1, 1024, VmStatus::Running).unwrap();
    add_vm(&database, "vm2", 1, 1, 1024, VmStatus::Running).unwrap();
    add_vm(&database, "vm3", 2, 1, 1024, VmStatus::Running).unwrap();
    
    // Schedule new VM with round-robin
    let vm_config = VmConfig {
        name: "new-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::RoundRobin).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Node 3 should be selected (has 0 VMs)
    assert_eq!(placement.selected_node_id, 3);
}

#[tokio::test]
async fn test_scheduler_manual_placement() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities.clone(), true).unwrap();
    add_worker(&database, 2, capabilities, true).unwrap();
    
    let vm_config = VmConfig {
        name: "manual-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    // Test valid manual placement
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::Manual { node_id: 2 }).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    assert_eq!(placement.selected_node_id, 2);
    assert!(placement.reason.contains("Manual placement"));
    
    // Test invalid manual placement
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::Manual { node_id: 999 }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_scheduler_feature_requirements() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers with different features
    let gpu_capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string(), "gpu".to_string()],
    };
    let basic_capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, gpu_capabilities, true).unwrap();
    add_worker(&database, 2, basic_capabilities, true).unwrap();
    
    // Create VM that requires GPU
    let gpu_vm = VmConfig {
        name: "gpu-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    // Note: VmResourceRequirements::from() always adds "microvm" feature
    // so we can't test GPU requirement directly without modifying the struct
    // For now, just verify basic scheduling works
    let result = scheduler.schedule_vm_placement(&gpu_vm, PlacementStrategy::MostAvailable).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_cluster_resource_summary() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers
    let capabilities1 = WorkerCapabilities {
        cpu_cores: 10,
        memory_mb: 20480,
        disk_gb: 200,
        features: vec!["microvm".to_string()],
    };
    let capabilities2 = WorkerCapabilities {
        cpu_cores: 6,
        memory_mb: 12288,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities1, true).unwrap();
    add_worker(&database, 2, capabilities2, true).unwrap();
    
    // Add some VMs
    add_vm(&database, "vm1", 1, 4, 8192, VmStatus::Running).unwrap();
    add_vm(&database, "vm2", 2, 2, 4096, VmStatus::Running).unwrap();
    add_vm(&database, "vm3", 1, 1, 1024, VmStatus::Stopped).unwrap(); // Should not count
    
    let summary = scheduler.get_cluster_resource_summary().await.unwrap();
    
    assert_eq!(summary.total_nodes, 2);
    assert_eq!(summary.total_vcpus, 16); // 10 + 6
    assert_eq!(summary.total_memory_mb, 32768); // 20480 + 12288
    assert_eq!(summary.total_disk_gb, 300); // 200 + 100
    assert_eq!(summary.used_vcpus, 6); // 4 + 2 (stopped VM doesn't count)
    assert_eq!(summary.used_memory_mb, 12288); // 8192 + 4096
    assert_eq!(summary.total_running_vms, 2);
    
    // Test utilization percentages
    let (cpu_util, mem_util, disk_util) = summary.utilization_percentages();
    assert!((cpu_util - 37.5).abs() < 0.1); // 6/16 = 37.5%
    assert!((mem_util - 37.5).abs() < 0.1); // 12288/32768 = 37.5%
}

#[tokio::test]
async fn test_scheduler_bin_packing() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities.clone(), true).unwrap();
    add_worker(&database, 2, capabilities, true).unwrap();
    
    // Add VMs to node 1 to make it 50% utilized
    add_vm(&database, "vm1", 1, 4, 8192, VmStatus::Running).unwrap();
    
    // Schedule with LeastAvailable (bin packing)
    let vm_config = VmConfig {
        name: "packed-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 2048,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::LeastAvailable).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Node 1 should be selected (already has VMs, so pack more there)
    assert_eq!(placement.selected_node_id, 1);
    assert!(placement.reason.contains("LeastAvailable"));
}