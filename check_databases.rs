#!/usr/bin/env cargo +nightly -Zscript

//! ```cargo
//! [dependencies]
//! redb = "2.1.3"
//! ```

use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;

// Table definitions from blixard-core
const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Checking VM Backend Database (blixard-vm/example.db) ===");
    
    if let Ok(db) = Database::open("./blixard-vm/example.db") {
        let db = Arc::new(db);
        let read_txn = db.begin_read()?;
        
        // Check if VM_STATE_TABLE exists and has data
        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            println!("VM_STATE_TABLE found with {} entries:", table.len()?);
            
            for entry in table.iter()? {
                let (key, value) = entry?;
                println!("  - Key: {}, Value size: {} bytes", key.value(), value.value().len());
            }
        } else {
            println!("VM_STATE_TABLE not found in microvm backend database");
        }
    } else {
        println!("Could not open blixard-vm/example.db");
    }

    println!("\n=== Checking Main Node Database (data/blixard.db) ===");
    
    if let Ok(db) = Database::open("./data/blixard.db") {
        let db = Arc::new(db);
        let read_txn = db.begin_read()?;
        
        // Check if VM_STATE_TABLE exists and has data
        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            println!("VM_STATE_TABLE found with {} entries:", table.len()?);
            
            for entry in table.iter()? {
                let (key, value) = entry?;
                println!("  - Key: {}, Value size: {} bytes", key.value(), value.value().len());
            }
        } else {
            println!("VM_STATE_TABLE not found in main node database");
        }
    } else {
        println!("Could not open data/blixard.db");
    }

    Ok(())
}