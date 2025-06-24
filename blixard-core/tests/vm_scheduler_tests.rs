use std::sync::Arc;
use tempfile::TempDir;
use redb::Database;

use blixard_core::{
    vm_scheduler::{VmScheduler, PlacementStrategy, VmResourceRequirements, NodeResourceUsage},
    raft_manager::WorkerCapabilities,
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
        
        let status = if is_online { 
            blixard_core::raft_manager::WorkerStatus::Online as u8 
        } else { 
            blixard_core::raft_manager::WorkerStatus::Offline as u8 
        };
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
async fn test_vm_resource_requirements_from_config() {
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 4,
        memory: 8192,
    };
    
    let requirements = VmResourceRequirements::from(&vm_config);
    assert_eq!(requirements.vcpus, 4);
    assert_eq!(requirements.memory_mb, 8192);
    assert_eq!(requirements.disk_gb, 5); // Default
    assert_eq!(requirements.required_features, vec!["microvm".to_string()]);
}

#[tokio::test]
async fn test_node_resource_usage_calculations() {
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    let usage = NodeResourceUsage {
        node_id: 1,
        capabilities,
        used_vcpus: 4,
        used_memory_mb: 8192,
        used_disk_gb: 50,
        running_vms: 2,
    };
    
    // Test available resource calculations
    assert_eq!(usage.available_vcpus(), 4);
    assert_eq!(usage.available_memory_mb(), 8192);
    assert_eq!(usage.available_disk_gb(), 50);
    
    // Test accommodation checks
    let small_requirements = VmResourceRequirements {
        vcpus: 2,
        memory_mb: 4096,
        disk_gb: 10,
        required_features: vec!["microvm".to_string()],
    };
    
    let large_requirements = VmResourceRequirements {
        vcpus: 8,
        memory_mb: 16384,
        disk_gb: 100,
        required_features: vec!["microvm".to_string()],
    };
    
    assert!(usage.can_accommodate(&small_requirements));
    assert!(!usage.can_accommodate(&large_requirements));
    
    // Test feature requirements
    let gpu_requirements = VmResourceRequirements {
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        required_features: vec!["gpu".to_string()],
    };
    
    assert!(!usage.can_accommodate(&gpu_requirements));
}

#[tokio::test]
async fn test_placement_strategy_scoring() {
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    let usage = NodeResourceUsage {
        node_id: 1,
        capabilities,
        used_vcpus: 4,
        used_memory_mb: 8192,
        used_disk_gb: 50,
        running_vms: 2,
    };
    
    // Most available should score based on available resources (50% available = 0.5 score)
    let most_available_score = usage.placement_score(&PlacementStrategy::MostAvailable);
    assert!((most_available_score - 0.5).abs() < 0.01);
    
    // Least available should score based on used resources (50% used = 0.5 score)
    let least_available_score = usage.placement_score(&PlacementStrategy::LeastAvailable);
    assert!((least_available_score - 0.5).abs() < 0.01);
    
    // Round robin should score based on VM count (1/(1+2) = 0.333)
    let round_robin_score = usage.placement_score(&PlacementStrategy::RoundRobin);
    assert!((round_robin_score - (1.0/3.0)).abs() < 0.01);
}

#[tokio::test]
async fn test_scheduler_no_workers_available() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database);
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 1024,
    };
    
    // No workers in database - should fail
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No healthy worker nodes available"));
}

#[tokio::test]
async fn test_scheduler_no_suitable_workers() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add a worker with insufficient resources
    let small_capabilities = WorkerCapabilities {
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 10,
        features: vec!["microvm".to_string()],
    };
    add_worker(&database, 1, small_capabilities, true).unwrap();
    
    // Try to place a VM that requires more resources
    let vm_config = VmConfig {
        name: "large-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 4,
        memory: 8192,
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No nodes can accommodate"));
}

#[tokio::test]
async fn test_scheduler_most_available_strategy() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add two workers with different available resources
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
    
    // Add some VMs to node 1 to reduce its available resources
    add_vm(&database, "vm1", 1, 4, 4096, VmStatus::Running).unwrap();
    add_vm(&database, "vm2", 1, 2, 2048, VmStatus::Running).unwrap();
    
    let vm_config = VmConfig {
        name: "new-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Node 2 should be selected as it has more available resources (percentage-wise)
    assert_eq!(placement.selected_node_id, 2);
    assert!(placement.reason.contains("MostAvailable"));
}

#[tokio::test]
async fn test_scheduler_least_available_strategy() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add two workers 
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
    
    // Add some VMs to node 1 to increase its used resources
    add_vm(&database, "vm1", 1, 4, 4096, VmStatus::Running).unwrap();
    
    let vm_config = VmConfig {
        name: "new-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::LeastAvailable).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Node 1 should be selected as it has more used resources (bin packing)
    assert_eq!(placement.selected_node_id, 1);
    assert!(placement.reason.contains("LeastAvailable"));
}

#[tokio::test]
async fn test_scheduler_round_robin_strategy() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add two workers with identical capabilities
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities.clone(), true).unwrap();
    add_worker(&database, 2, capabilities, true).unwrap();
    
    // Add more VMs to node 1
    add_vm(&database, "vm1", 1, 1, 1024, VmStatus::Running).unwrap();
    add_vm(&database, "vm2", 1, 1, 1024, VmStatus::Running).unwrap();
    add_vm(&database, "vm3", 2, 1, 1024, VmStatus::Running).unwrap();
    
    let vm_config = VmConfig {
        name: "new-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::RoundRobin).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Node 2 should be selected as it has fewer running VMs
    assert_eq!(placement.selected_node_id, 2);
    assert!(placement.reason.contains("RoundRobin"));
}

#[tokio::test]
async fn test_scheduler_manual_strategy() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add a worker
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    add_worker(&database, 1, capabilities, true).unwrap();
    
    let vm_config = VmConfig {
        name: "manual-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 2,
        memory: 2048,
    };
    
    // Test valid manual placement
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::Manual { node_id: 1 }).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    assert_eq!(placement.selected_node_id, 1);
    assert!(placement.reason.contains("Manual placement"));
    
    // Test invalid manual placement (non-existent node)
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::Manual { node_id: 999 }).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_scheduler_offline_workers_ignored() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add online and offline workers
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    
    add_worker(&database, 1, capabilities.clone(), true).unwrap(); // Online
    add_worker(&database, 2, capabilities, false).unwrap(); // Offline
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
    };
    
    let result = scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await;
    assert!(result.is_ok());
    
    let placement = result.unwrap();
    // Only node 1 should be considered (node 2 is offline)
    assert_eq!(placement.selected_node_id, 1);
    assert!(placement.alternative_nodes.is_empty());
}

#[tokio::test]
async fn test_cluster_resource_summary() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add workers
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
    
    // Add some VMs
    add_vm(&database, "vm1", 1, 2, 2048, VmStatus::Running).unwrap();
    add_vm(&database, "vm2", 2, 1, 1024, VmStatus::Running).unwrap();
    add_vm(&database, "vm3", 1, 1, 512, VmStatus::Stopped).unwrap(); // Stopped VM shouldn't count
    
    let result = scheduler.get_cluster_resource_summary().await;
    assert!(result.is_ok());
    
    let summary = result.unwrap();
    assert_eq!(summary.total_nodes, 2);
    assert_eq!(summary.total_vcpus, 12); // 8 + 4
    assert_eq!(summary.used_vcpus, 3); // 2 + 1 (stopped VM doesn't count)
    assert_eq!(summary.total_memory_mb, 24576); // 16384 + 8192
    assert_eq!(summary.used_memory_mb, 3072); // 2048 + 1024
    assert_eq!(summary.total_disk_gb, 150); // 100 + 50
    assert_eq!(summary.used_disk_gb, 10); // 5 + 5 (default per running VM)
    assert_eq!(summary.total_running_vms, 2); // Only running VMs
    assert_eq!(summary.nodes.len(), 2);
    
    // Test utilization calculations
    let (cpu_util, memory_util, disk_util) = summary.utilization_percentages();
    assert!((cpu_util - 25.0).abs() < 0.01); // 3/12 = 25%
    assert!((memory_util - 12.5).abs() < 0.01); // 3072/24576 = 12.5%
    assert!((disk_util - 6.67).abs() < 0.1); // 10/150 = 6.67%
}

#[tokio::test]
async fn test_vm_status_resource_counting() {
    let (database, _temp_dir) = create_test_database().unwrap();
    let scheduler = VmScheduler::new(database.clone());
    
    // Add a worker
    let capabilities = WorkerCapabilities {
        cpu_cores: 8,
        memory_mb: 16384,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };
    add_worker(&database, 1, capabilities, true).unwrap();
    
    // Add VMs with different statuses
    add_vm(&database, "running-vm", 1, 2, 2048, VmStatus::Running).unwrap();
    add_vm(&database, "starting-vm", 1, 1, 1024, VmStatus::Starting).unwrap();
    add_vm(&database, "stopped-vm", 1, 4, 4096, VmStatus::Stopped).unwrap();
    add_vm(&database, "failed-vm", 1, 2, 2048, VmStatus::Failed).unwrap();
    
    let summary = scheduler.get_cluster_resource_summary().await.unwrap();
    
    // Only running and starting VMs should count toward resource usage
    assert_eq!(summary.used_vcpus, 3); // 2 + 1
    assert_eq!(summary.used_memory_mb, 3072); // 2048 + 1024
    assert_eq!(summary.total_running_vms, 2); // Only running and starting
    
    // Verify node-level statistics
    assert_eq!(summary.nodes[0].used_vcpus, 3);
    assert_eq!(summary.nodes[0].used_memory_mb, 3072);
    assert_eq!(summary.nodes[0].running_vms, 2);
}

#[tokio::test]
async fn test_feature_requirements() {
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
    
    // Create a VM config that will require GPU features
    let vm_config = VmConfig {
        name: "gpu-vm".to_string(),
        config_path: "".to_string(),
        vcpus: 1,
        memory: 1024,
    };
    
    // Manually create requirements with GPU feature
    let gpu_requirements = VmResourceRequirements {
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        required_features: vec!["microvm".to_string(), "gpu".to_string()],
    };
    
    // Test that only the GPU-capable node can accommodate GPU requirements
    let node_usage = scheduler.get_cluster_resource_usage().await.unwrap();
    assert_eq!(node_usage.len(), 2);
    
    let gpu_node = node_usage.iter().find(|n| n.node_id == 1).unwrap();
    let basic_node = node_usage.iter().find(|n| n.node_id == 2).unwrap();
    
    assert!(gpu_node.can_accommodate(&gpu_requirements));
    assert!(!basic_node.can_accommodate(&gpu_requirements));
    
    // Regular microvm requirements should work on both
    let basic_requirements = VmResourceRequirements::from(&vm_config);
    assert!(gpu_node.can_accommodate(&basic_requirements));
    assert!(basic_node.can_accommodate(&basic_requirements));
}