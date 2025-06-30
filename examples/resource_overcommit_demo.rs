//! Example demonstrating resource reservation and overcommit policies
//!
//! This example shows how to use different overcommit policies and
//! resource reservations to optimize cluster utilization.

use blixard_core::{
    resource_management::{
        OvercommitPolicy, NodeResourceState, ClusterResourceManager,
        ResourceReservation,
    },
    vm_scheduler::{VmScheduler, PlacementStrategy},
    types::VmConfig,
};
use std::sync::Arc;
use redb::Database;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("=== Blixard Resource Management Demo ===\n");
    
    // Create temporary database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("demo.db");
    let database = Arc::new(Database::create(&db_path)?);
    
    // Initialize tables
    {
        let write_txn = database.begin_write()?;
        let _ = write_txn.open_table(blixard_core::storage::WORKER_TABLE)?;
        let _ = write_txn.open_table(blixard_core::storage::WORKER_STATUS_TABLE)?;
        let _ = write_txn.open_table(blixard_core::storage::VM_STATE_TABLE)?;
        write_txn.commit()?;
    }
    
    // Demo 1: Conservative Policy (No Overcommit)
    println!("1. Conservative Policy - No Overcommit");
    println!("   - CPU: 1.0x, Memory: 1.0x, Disk: 1.0x");
    println!("   - 10% system reserve");
    demo_conservative_policy()?;
    
    // Demo 2: Moderate Policy (Reasonable Overcommit)
    println!("\n2. Moderate Policy - Balanced Overcommit");
    println!("   - CPU: 2.0x, Memory: 1.2x, Disk: 1.1x");
    println!("   - 15% system reserve");
    demo_moderate_policy()?;
    
    // Demo 3: Aggressive Policy (High Overcommit)
    println!("\n3. Aggressive Policy - Maximum Utilization");
    println!("   - CPU: 4.0x, Memory: 1.5x, Disk: 1.3x");
    println!("   - 5% system reserve");
    demo_aggressive_policy()?;
    
    // Demo 4: Resource Reservations
    println!("\n4. Resource Reservations");
    demo_resource_reservations()?;
    
    // Demo 5: VM Placement with Overcommit
    println!("\n5. VM Placement with Overcommit");
    demo_vm_placement_with_overcommit(&database).await?;
    
    Ok(())
}

fn demo_conservative_policy() -> Result<(), Box<dyn std::error::Error>> {
    let policy = OvercommitPolicy::conservative();
    let mut node = NodeResourceState::new(
        8,      // 8 CPU cores
        16384,  // 16 GB RAM
        100,    // 100 GB disk
        policy,
    );
    
    println!("   Physical Resources: 8 CPU, 16 GB RAM, 100 GB Disk");
    let (cpu, mem, disk) = node.effective_capacity();
    println!("   Effective Capacity: {} CPU, {} MB RAM, {} GB Disk", cpu, mem, disk);
    
    // Try to allocate a VM
    if node.can_allocate(4, 8192, 50) {
        node.allocate(4, 8192, 50)?;
        println!("   ✓ Allocated VM: 4 CPU, 8 GB RAM, 50 GB Disk");
        
        let (avail_cpu, avail_mem, avail_disk) = node.available_resources();
        println!("   Remaining: {} CPU, {} MB RAM, {} GB Disk", avail_cpu, avail_mem, avail_disk);
    }
    
    Ok(())
}

fn demo_moderate_policy() -> Result<(), Box<dyn std::error::Error>> {
    let policy = OvercommitPolicy::moderate();
    let mut node = NodeResourceState::new(
        8,      // 8 CPU cores
        16384,  // 16 GB RAM
        100,    // 100 GB disk
        policy,
    );
    
    println!("   Physical Resources: 8 CPU, 16 GB RAM, 100 GB Disk");
    let (cpu, mem, disk) = node.effective_capacity();
    println!("   Effective Capacity: {} CPU, {} MB RAM, {} GB Disk", cpu, mem, disk);
    
    // Allocate multiple VMs that exceed physical resources
    let vms = vec![
        (4, 6144, 30, "web-1"),
        (4, 6144, 30, "web-2"),
        (4, 4096, 20, "web-3"),
    ];
    
    for (cpu, mem, disk, name) in vms {
        if node.can_allocate(cpu, mem, disk) {
            node.allocate(cpu, mem, disk)?;
            println!("   ✓ Allocated {}: {} CPU, {} MB RAM, {} GB Disk", name, cpu, mem, disk);
        } else {
            println!("   ✗ Cannot allocate {}: insufficient resources", name);
        }
    }
    
    let (cpu_util, mem_util, disk_util) = node.utilization_percentages();
    println!("   Utilization: CPU {:.1}%, Memory {:.1}%, Disk {:.1}%", 
             cpu_util, mem_util, disk_util);
    
    if node.is_overcommitted() {
        println!("   ⚠️  Node is overcommitted!");
    }
    
    Ok(())
}

fn demo_aggressive_policy() -> Result<(), Box<dyn std::error::Error>> {
    let policy = OvercommitPolicy::aggressive();
    let mut node = NodeResourceState::new(
        4,      // 4 CPU cores
        8192,   // 8 GB RAM
        50,     // 50 GB disk
        policy,
    );
    
    println!("   Physical Resources: 4 CPU, 8 GB RAM, 50 GB Disk");
    let (cpu, mem, disk) = node.effective_capacity();
    println!("   Effective Capacity: {} CPU, {} MB RAM, {} GB Disk", cpu, mem, disk);
    
    // Allocate many lightweight containers
    let containers = vec![
        (1, 512, 5, "app-1"),
        (1, 512, 5, "app-2"),
        (1, 512, 5, "app-3"),
        (1, 512, 5, "app-4"),
        (1, 512, 5, "app-5"),
        (1, 512, 5, "app-6"),
        (1, 512, 5, "app-7"),
        (1, 512, 5, "app-8"),
    ];
    
    let mut allocated = 0;
    for (cpu, mem, disk, name) in containers {
        if node.can_allocate(cpu, mem, disk) {
            node.allocate(cpu, mem, disk)?;
            allocated += 1;
            println!("   ✓ Allocated {}: {} CPU, {} MB RAM, {} GB Disk", name, cpu, mem, disk);
        }
    }
    
    println!("   Total containers allocated: {}", allocated);
    let (cpu_util, mem_util, disk_util) = node.utilization_percentages();
    println!("   Physical Utilization: CPU {:.1}%, Memory {:.1}%, Disk {:.1}%", 
             cpu_util, mem_util, disk_util);
    
    Ok(())
}

fn demo_resource_reservations() -> Result<(), Box<dyn std::error::Error>> {
    let mut node = NodeResourceState::new(
        16,     // 16 CPU cores
        32768,  // 32 GB RAM
        200,    // 200 GB disk
        OvercommitPolicy::moderate(),
    );
    
    println!("   Node Resources: 16 CPU, 32 GB RAM, 200 GB Disk");
    
    // Create reservations for different purposes
    let reservations = vec![
        ResourceReservation {
            id: "monitoring".to_string(),
            owner: "system".to_string(),
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 20,
            priority: 1000,  // High priority
            is_hard: true,
            expires_at: None,
        },
        ResourceReservation {
            id: "backup-job".to_string(),
            owner: "backup-service".to_string(),
            cpu_cores: 4,
            memory_mb: 8192,
            disk_gb: 50,
            priority: 500,
            is_hard: false,  // Soft reservation
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(2)),
        },
        ResourceReservation {
            id: "critical-db".to_string(),
            owner: "database".to_string(),
            cpu_cores: 8,
            memory_mb: 16384,
            disk_gb: 100,
            priority: 900,
            is_hard: true,
            expires_at: None,
        },
    ];
    
    for reservation in reservations {
        match node.add_reservation(reservation.clone()) {
            Ok(_) => {
                println!("   ✓ Reserved for {}: {} CPU, {} MB RAM, {} GB Disk (Priority: {})",
                         reservation.owner, reservation.cpu_cores, reservation.memory_mb,
                         reservation.disk_gb, reservation.priority);
            }
            Err(e) => {
                println!("   ✗ Failed to reserve for {}: {}", reservation.owner, e);
            }
        }
    }
    
    let (avail_cpu, avail_mem, avail_disk) = node.available_resources();
    println!("   Available after reservations: {} CPU, {} MB RAM, {} GB Disk",
             avail_cpu, avail_mem, avail_disk);
    
    Ok(())
}

async fn demo_vm_placement_with_overcommit(
    database: &Arc<Database>
) -> Result<(), Box<dyn std::error::Error>> {
    // Create scheduler with moderate overcommit
    let scheduler = VmScheduler::with_overcommit_policy(
        database.clone(),
        OvercommitPolicy::moderate()
    );
    
    // Add worker nodes
    add_demo_worker(database, 1, 8, 16384, 100).await?;
    add_demo_worker(database, 2, 8, 16384, 100).await?;
    
    // Sync resource manager
    scheduler.sync_resource_manager().await?;
    
    // Create VMs that would exceed physical capacity but fit with overcommit
    let vms = vec![
        ("web-frontend-1", 4, 4096),
        ("web-frontend-2", 4, 4096),
        ("api-server-1", 4, 8192),
        ("api-server-2", 4, 8192),
        ("cache-server", 2, 16384),
    ];
    
    println!("   Scheduling VMs with overcommit enabled:");
    for (name, vcpus, memory) in vms {
        let vm_config = VmConfig {
            name: name.to_string(),
            config_path: "/test.nix".to_string(),
            vcpus,
            memory,
            tenant_id: "demo".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
        };
        
        match scheduler.schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable).await {
            Ok(decision) => {
                println!("   ✓ {} scheduled on node {}: {}",
                         name, decision.selected_node_id, decision.reason);
            }
            Err(e) => {
                println!("   ✗ {} failed to schedule: {}", name, e);
            }
        }
    }
    
    // Show cluster summary
    let summary = scheduler.get_cluster_resource_summary().await?;
    println!("\n   Cluster Summary:");
    println!("   - Total VCPUs: {} (used: {})", summary.total_vcpus, summary.used_vcpus);
    println!("   - Total Memory: {} MB (used: {} MB)", 
             summary.total_memory_mb, summary.used_memory_mb);
    println!("   - Total Disk: {} GB (used: {} GB)", 
             summary.total_disk_gb, summary.used_disk_gb);
    
    Ok(())
}

async fn add_demo_worker(
    database: &Database,
    node_id: u64,
    cpu: u32,
    memory: u64,
    disk: u64
) -> Result<(), Box<dyn std::error::Error>> {
    use blixard_core::raft_manager::WorkerCapabilities;
    use blixard_core::storage::{WORKER_TABLE, WORKER_STATUS_TABLE};
    
    let write_txn = database.begin_write()?;
    
    // Add worker capabilities
    {
        let mut worker_table = write_txn.open_table(WORKER_TABLE)?;
        let capabilities = WorkerCapabilities {
            cpu_cores: cpu,
            memory_mb: memory,
            disk_gb: disk,
            features: vec!["microvm".to_string()],
        };
        let worker_data = bincode::serialize(&("demo-worker".to_string(), capabilities))?;
        worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
    }
    
    // Mark as online
    {
        let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE)?;
        status_table.insert(
            node_id.to_le_bytes().as_slice(),
            vec![blixard_core::raft_manager::WorkerStatus::Online as u8].as_slice()
        )?;
    }
    
    write_txn.commit()?;
    Ok(())
}