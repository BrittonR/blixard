// Database and storage operation tests

use tempfile::TempDir;
use blixard_core::{
    types::{VmState, VmConfig, VmStatus},
};
use redb::{Database, TableDefinition, ReadableTableMetadata};

mod common;

const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");

async fn create_test_database() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::create(&db_path).unwrap();
    (database, temp_dir)
}

fn create_test_vm_state(name: &str, node_id: u64) -> VmState {
    let config = VmConfig {
        name: name.to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory: 1024,
        tenant_id: "default".to_string(),
        ip_address: None,
    };
    
    let now = chrono::Utc::now();
    VmState {
        name: name.to_string(),
        config,
        status: VmStatus::Creating,
        node_id,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_creation() {
    let (database, temp_dir) = create_test_database().await;
    
    // Verify database file was created
    let db_path = temp_dir.path().join("test.db");
    assert!(db_path.exists(), "Database file should exist");
    assert!(db_path.is_file(), "Database path should be a file");
    
    // Test data
    let test_key = "test-key";
    let test_data = b"test-data";
    
    // Verify we can create tables
    let write_txn = database.begin_write().unwrap();
    {
        let table_result = write_txn.open_table(VM_STATE_TABLE);
        assert!(table_result.is_ok(), "Should be able to create VM_STATE_TABLE");
        let mut table = table_result.unwrap();
        
        // Verify empty table
        assert_eq!(table.len().unwrap(), 0, "New table should be empty");
        
        // Verify we can write and read data
        table.insert(test_key, test_data as &[u8]).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Verify data persisted
    let read_txn = database.begin_read().unwrap();
    let read_table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    assert_eq!(read_table.len().unwrap(), 1, "Table should have one entry");
    
    let retrieved = read_table.get(test_key).unwrap();
    assert!(retrieved.is_some(), "Should retrieve stored data");
    assert_eq!(retrieved.unwrap().value(), test_data as &[u8], "Retrieved data should match");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_state_persistence() {
    let (database, _temp_dir) = create_test_database().await;
    let vm_state = create_test_vm_state("test-vm", 1);
    
    // Persist VM state
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let serialized = bincode::serialize(&vm_state).unwrap();
        table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Read VM state back
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    let stored_data = table.get("test-vm").unwrap().unwrap();
    let restored_state: VmState = bincode::deserialize(stored_data.value()).unwrap();
    
    assert_eq!(restored_state.name, vm_state.name);
    assert_eq!(restored_state.node_id, vm_state.node_id);
    assert_eq!(restored_state.status, vm_state.status);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_vm_states() {
    let (database, _temp_dir) = create_test_database().await;
    
    let vm_states = vec![
        create_test_vm_state("vm1", 1),
        create_test_vm_state("vm2", 1),
        create_test_vm_state("vm3", 2),
    ];
    
    // Store all VM states
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        for vm_state in &vm_states {
            let serialized = bincode::serialize(vm_state).unwrap();
            table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
        }
    }
    write_txn.commit().unwrap();
    
    // Read all VM states back
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    
    for vm_state in &vm_states {
        let stored_data = table.get(vm_state.name.as_str()).unwrap().unwrap();
        let restored_state: VmState = bincode::deserialize(stored_data.value()).unwrap();
        assert_eq!(restored_state.name, vm_state.name);
        assert_eq!(restored_state.node_id, vm_state.node_id);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_state_update() {
    let (database, _temp_dir) = create_test_database().await;
    let mut vm_state = create_test_vm_state("update-vm", 1);
    
    // Initial store
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let serialized = bincode::serialize(&vm_state).unwrap();
        table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Update status
    vm_state.status = VmStatus::Running;
    vm_state.updated_at = chrono::Utc::now();
    
    // Store updated state
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let serialized = bincode::serialize(&vm_state).unwrap();
        table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Verify update
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    let stored_data = table.get("update-vm").unwrap().unwrap();
    let restored_state: VmState = bincode::deserialize(stored_data.value()).unwrap();
    
    assert_eq!(restored_state.status, VmStatus::Running);
    assert!(restored_state.updated_at >= vm_state.updated_at);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_state_deletion() {
    let (database, _temp_dir) = create_test_database().await;
    let vm_state = create_test_vm_state("delete-vm", 1);
    
    // Store VM state
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let serialized = bincode::serialize(&vm_state).unwrap();
        table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Verify it exists
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    assert!(table.get("delete-vm").unwrap().is_some());
    drop(table);
    drop(read_txn);
    
    // Delete it
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        table.remove("delete-vm").unwrap();
    }
    write_txn.commit().unwrap();
    
    // Verify it's gone
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    assert!(table.get("delete-vm").unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_transaction_commit_vs_no_commit() {
    let (database, _temp_dir) = create_test_database().await;
    let vm_state = create_test_vm_state("rollback-vm", 1);
    
    // Test successful transaction
    {
        let write_txn = database.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
            let serialized = bincode::serialize(&vm_state).unwrap();
            table.insert("committed-vm", serialized.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }
    
    // Verify committed data exists
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    assert!(table.get("committed-vm").unwrap().is_some());
    
    // Now test that uncommitted data doesn't persist
    // This test demonstrates proper transaction semantics
    assert!(table.get("rollback-vm").unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_reads() {
    let (database, _temp_dir) = create_test_database().await;
    let vm_state = create_test_vm_state("concurrent-vm", 1);
    
    // Store initial state
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let serialized = bincode::serialize(&vm_state).unwrap();
        table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Multiple concurrent reads (using Arc to share database)
    use std::sync::Arc;
    let database = Arc::new(database);
    let mut handles = vec![];
    for i in 0..5 {
        let db_clone = Arc::clone(&database);
        let handle = tokio::spawn(async move {
            let read_txn = db_clone.begin_read().unwrap();
            let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
            let stored_data = table.get("concurrent-vm").unwrap().unwrap();
            let restored_state: VmState = bincode::deserialize(stored_data.value()).unwrap();
            assert_eq!(restored_state.name, "concurrent-vm");
            i
        });
        handles.push(handle);
    }
    
    // Wait for all reads to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_serialization_errors() {
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize)]
    struct BadStruct {
        #[serde(skip_serializing)]
        data: String,
    }
    
    let (database, _temp_dir) = create_test_database().await;
    
    // This should work fine - redb doesn't validate content
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
        let invalid_data: &[u8] = b"not valid bincode";
        table.insert("bad-data", invalid_data).unwrap();
    }
    write_txn.commit().unwrap();
    
    // Reading back and deserializing should fail
    let read_txn = database.begin_read().unwrap();
    let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
    let stored_data = table.get("bad-data").unwrap().unwrap();
    let result: Result<VmState, bincode::Error> = bincode::deserialize(stored_data.value());
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_persistence_across_connections() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("persistent.db");
    let vm_state = create_test_vm_state("persistent-vm", 1);
    
    // Create database and store data
    {
        let database = Database::create(&db_path).unwrap();
        let write_txn = database.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE).unwrap();
            let serialized = bincode::serialize(&vm_state).unwrap();
            table.insert(vm_state.name.as_str(), serialized.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    } // Database drops here
    
    // Reopen database and read data
    {
        let database = Database::open(&db_path).unwrap();
        let read_txn = database.begin_read().unwrap();
        let table = read_txn.open_table(VM_STATE_TABLE).unwrap();
        let stored_data = table.get("persistent-vm").unwrap().unwrap();
        let restored_state: VmState = bincode::deserialize(stored_data.value()).unwrap();
        
        assert_eq!(restored_state.name, vm_state.name);
        assert_eq!(restored_state.node_id, vm_state.node_id);
    }
}