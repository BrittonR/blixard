// Simple utility to inspect VM data in the redb database
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::Path;

// Table definitions (from raft_storage.rs)
const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
const VM_IP_MAPPING_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_ip_mappings");
const IP_ALLOCATION_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ip_allocations");
const IP_POOL_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("ip_pools");
const TASK_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tasks");
const CLUSTER_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cluster_state");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "/home/brittonr/git/blixard/data/blixard.db";
    
    if !Path::new(db_path).exists() {
        println!("Database file {} does not exist", db_path);
        return Ok(());
    }
    
    let db = Database::open(db_path)?;
    let read_txn = db.begin_read()?;
    
    println!("=== Blixard Database VM Data Inspection ===\n");
    
    // Check VM states
    println!("🖥️  VM States:");
    if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
        let count = table.len()?;
        println!("  - Total VM states: {}", count);
        
        for entry in table.iter()? {
            let (vm_name, state_data) = entry?;
            println!("  - VM: {} (data size: {} bytes)", vm_name.value(), state_data.value().len());
            
            // Try to parse the state data as string for basic inspection
            if let Ok(state_str) = String::from_utf8(state_data.value().to_vec()) {
                if state_str.len() < 200 {
                    println!("    State: {}", state_str);
                } else {
                    println!("    State: {} (truncated...)", &state_str[..200]);
                }
            } else {
                println!("    State: Binary data ({} bytes)", state_data.value().len());
            }
        }
    } else {
        println!("  - VM states table not found or empty");
    }
    
    // Check VM IP mappings
    println!("\n🌐 VM IP Mappings:");
    if let Ok(table) = read_txn.open_table(VM_IP_MAPPING_TABLE) {
        let count = table.len()?;
        println!("  - Total VM IP mappings: {}", count);
        
        for entry in table.iter()? {
            let (vm_name, ip_data) = entry?;
            println!("  - VM: {} (IP data: {} bytes)", vm_name.value(), ip_data.value().len());
            
            if let Ok(ip_str) = String::from_utf8(ip_data.value().to_vec()) {
                println!("    IP: {}", ip_str);
            }
        }
    } else {
        println!("  - VM IP mappings table not found or empty");
    }
    
    // Check IP allocations
    println!("\n📍 IP Allocations:");
    if let Ok(table) = read_txn.open_table(IP_ALLOCATION_TABLE) {
        let count = table.len()?;
        println!("  - Total IP allocations: {}", count);
        
        for entry in table.iter()? {
            let (ip, allocation_data) = entry?;
            println!("  - IP: {} (allocation data: {} bytes)", ip.value(), allocation_data.value().len());
        }
    } else {
        println!("  - IP allocations table not found or empty");
    }
    
    // Check IP pools
    println!("\n🏊 IP Pools:");
    if let Ok(table) = read_txn.open_table(IP_POOL_TABLE) {
        let count = table.len()?;
        println!("  - Total IP pools: {}", count);
        
        for entry in table.iter()? {
            let (pool_id, pool_data) = entry?;
            println!("  - Pool ID: {} (data size: {} bytes)", pool_id.value(), pool_data.value().len());
        }
    } else {
        println!("  - IP pools table not found or empty");
    }
    
    // Check tasks (VM-related tasks)
    println!("\n📋 Tasks:");
    if let Ok(table) = read_txn.open_table(TASK_TABLE) {
        let count = table.len()?;
        println!("  - Total tasks: {}", count);
        
        for entry in table.iter()? {
            let (task_id, task_data) = entry?;
            println!("  - Task: {} (data size: {} bytes)", task_id.value(), task_data.value().len());
            
            // Try to parse task data to see if it's VM-related
            if let Ok(task_str) = String::from_utf8(task_data.value().to_vec()) {
                if task_str.contains("vm") || task_str.contains("VM") {
                    if task_str.len() < 200 {
                        println!("    VM-related task: {}", task_str);
                    } else {
                        println!("    VM-related task: {} (truncated...)", &task_str[..200]);
                    }
                }
            }
        }
    } else {
        println!("  - Tasks table not found or empty");
    }
    
    // Check cluster state for any VM information
    println!("\n🔗 Cluster State (VM-related entries):");
    if let Ok(table) = read_txn.open_table(CLUSTER_STATE_TABLE) {
        let count = table.len()?;
        println!("  - Total cluster state entries: {}", count);
        
        for entry in table.iter()? {
            let (key, value) = entry?;
            let key_str = key.value();
            
            // Only show VM-related cluster state
            if key_str.contains("vm") || key_str.contains("VM") {
                println!("  - Key: {} (data size: {} bytes)", key_str, value.value().len());
                
                if let Ok(value_str) = String::from_utf8(value.value().to_vec()) {
                    if value_str.len() < 200 {
                        println!("    Value: {}", value_str);
                    } else {
                        println!("    Value: {} (truncated...)", &value_str[..200]);
                    }
                }
            }
        }
    } else {
        println!("  - Cluster state table not found or empty");
    }
    
    println!("\n=== End of VM Data Inspection ===");
    
    Ok(())
}