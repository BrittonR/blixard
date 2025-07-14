use redb::{Database, ReadableTable, TableDefinition, TableHandle};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Analyzing VM Backend Database Structure ===");
    
    if let Ok(db) = Database::open("../blixard-vm/example.db") {
        let db = Arc::new(db);
        let read_txn = db.begin_read()?;
        
        // List all tables in the database
        println!("Tables in VM backend database:");
        for table_name in read_txn.list_tables()? {
            println!("  - {}", table_name.name());
        }
        
        // Check VM state table specifically
        const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            println!("\nVM_STATE_TABLE entries:");
            for entry in table.iter()? {
                let (key, value) = entry?;
                let value_bytes = value.value();
                
                // Try to decode as JSON for readability
                if let Ok(json_str) = std::str::from_utf8(value_bytes) {
                    println!("  Key: '{}' -> {}", key.value(), json_str);
                } else {
                    println!("  Key: '{}' -> {} bytes (binary)", key.value(), value_bytes.len());
                }
            }
        }
    }

    println!("\n=== Analyzing Main Node Database Structure ===");
    
    if let Ok(db) = Database::open("../data/blixard.db") {
        let db = Arc::new(db);
        let read_txn = db.begin_read()?;
        
        println!("Tables in main node database:");
        for table_name in read_txn.list_tables()? {
            println!("  - {}", table_name.name());
        }
        
        // Check VM state table specifically
        const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            println!("\nVM_STATE_TABLE entries:");
            for entry in table.iter()? {
                let (key, value) = entry?;
                let value_bytes = value.value();
                
                // Try to decode as JSON for readability
                if let Ok(json_str) = std::str::from_utf8(value_bytes) {
                    println!("  Key: '{}' -> {}", key.value(), json_str);
                } else {
                    println!("  Key: '{}' -> {} bytes (binary)", key.value(), value_bytes.len());
                }
            }
        } else {
            println!("VM_STATE_TABLE not found");
        }
    } else {
        println!("Could not open main node database");
    }

    Ok(())
}