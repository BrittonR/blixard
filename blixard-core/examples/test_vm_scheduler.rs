//! Simple example to test VM scheduler functionality

use std::sync::Arc;
use tempfile::TempDir;
use redb::Database;

use blixard_core::{
    vm_scheduler::{VmScheduler, PlacementStrategy},
    raft_manager::{WorkerCapabilities, WorkerStatus},
    types::{VmConfig, LocalityPreference},
    storage::{init_database_tables, WORKER_TABLE, WORKER_STATUS_TABLE},
    error::BlixardResult,
};

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Initialize metrics (using noop for testing)
    blixard_core::metrics_otel::init_noop();
    
    println!("=== VM Scheduler Test ===");
    
    // Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path)?);
    
    // Initialize tables
    init_database_tables(&database)?;
    
    // Add some test workers
    add_test_workers(&database)?;
    
    // Create scheduler
    let scheduler = VmScheduler::new(database.clone());
    
    // Test 1: Basic placement with MostAvailable strategy
    println!("\n1. Testing MostAvailable strategy:");
    let vm_config = create_test_vm("test-vm-1", 2, 2048);
    match scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await {
        Ok(decision) => {
            println!("  ✓ VM placement successful!");
            println!("    - Selected node: {}", decision.selected_node_id);
            println!("    - Reason: {}", decision.reason);
            println!("    - Alternatives: {:?}", decision.alternative_nodes);
        }
        Err(e) => println!("  ✗ VM placement failed: {}", e),
    }
    
    // Test 2: Resource constraints
    println!("\n2. Testing resource constraints:");
    let large_vm = create_test_vm("large-vm", 16, 32768);
    match scheduler.schedule_vm_placement(&large_vm, PlacementStrategy::MostAvailable).await {
        Ok(decision) => {
            println!("  ✓ Large VM placement successful on node {}", decision.selected_node_id);
        }
        Err(e) => println!("  ✗ Large VM placement failed (expected): {}", e),
    }
    
    // Test 3: Round-robin placement
    println!("\n3. Testing RoundRobin strategy:");
    for i in 1..=3 {
        let vm_config = create_test_vm(&format!("rr-vm-{}", i), 1, 1024);
        match scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::RoundRobin).await {
            Ok(decision) => {
                println!("  - VM {} placed on node {}", i, decision.selected_node_id);
            }
            Err(e) => println!("  ✗ VM {} placement failed: {}", i, e),
        }
    }
    
    // Test 4: Cluster resource summary
    println!("\n4. Getting cluster resource summary:");
    match scheduler.get_cluster_resource_summary().await {
        Ok(summary) => {
            println!("  ✓ Cluster summary:");
            println!("    - Total nodes: {}", summary.total_nodes);
            println!("    - Total vCPUs: {}", summary.total_vcpus);
            println!("    - Used vCPUs: {}", summary.used_vcpus);
            println!("    - Total memory: {} MB", summary.total_memory_mb);
            println!("    - Used memory: {} MB", summary.used_memory_mb);
            let (cpu_util, mem_util, _disk_util) = summary.utilization_percentages();
            println!("    - CPU utilization: {:.1}%", cpu_util);
            println!("    - Memory utilization: {:.1}%", mem_util);
        }
        Err(e) => println!("  ✗ Failed to get cluster summary: {}", e),
    }
    
    println!("\n=== Test completed ===");
    Ok(())
}

fn add_test_workers(database: &Database) -> BlixardResult<()> {
    let workers = vec![
        (1u64, 8u32, 16384u64, 500u64),  // Node 1: 8 cores, 16GB RAM, 500GB disk
        (2u64, 4u32, 8192u64, 250u64),   // Node 2: 4 cores, 8GB RAM, 250GB disk
        (3u64, 16u32, 32768u64, 1000u64), // Node 3: 16 cores, 32GB RAM, 1TB disk
    ];
    
    for (node_id, cpu_cores, memory_mb, disk_gb) in workers {
        let capabilities = WorkerCapabilities {
            cpu_cores,
            memory_mb,
            disk_gb,
            features: vec!["microvm".to_string()],
        };
        
        let write_txn = database.begin_write()?;
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE)?;
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE)?;
            
            let worker_data = bincode::serialize(&(format!("127.0.0.1:{}", 7000 + node_id), capabilities))?;
            worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
            
            status_table.insert(node_id.to_le_bytes().as_slice(), [WorkerStatus::Online as u8].as_slice())?;
        }
        write_txn.commit()?;
        
        println!("Added worker node {} ({}c/{}GB/{}GB)", node_id, cpu_cores, memory_mb/1024, disk_gb);
    }
    
    Ok(())
}

fn create_test_vm(name: &str, vcpus: u32, memory: u32) -> VmConfig {
    VmConfig {
        name: name.to_string(),
        config_path: format!("/etc/blixard/vms/{}.yaml", name),
        vcpus,
        memory,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 100,
        locality_preference: LocalityPreference::default(),
        ..Default::default()
    }
}