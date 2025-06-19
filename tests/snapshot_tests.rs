#![cfg(feature = "test-helpers")]


use blixard::{
    error::BlixardResult,
    storage::{RedbRaftStorage, SnapshotData},
    raft_manager::RaftStateMachine,
};

mod common;

/// Test basic snapshot creation functionality
#[tokio::test]
async fn test_snapshot_creation() -> BlixardResult<()> {
    // Create temporary database
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let storage = RedbRaftStorage { database: database.clone() };
    
    // Add some test data
    {
        let write_txn = database.begin_write()?;
        {
            let mut vm_table = write_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
            vm_table.insert("test-vm-1", b"vm-state-1".as_slice())?;
            vm_table.insert("test-vm-2", b"vm-state-2".as_slice())?;
        }
        {
            let mut task_table = write_txn.open_table(blixard::storage::TASK_TABLE)?;
            task_table.insert("task-1", b"task-data-1".as_slice())?;
            task_table.insert("task-2", b"task-data-2".as_slice())?;
        }
        write_txn.commit()?;
    }
    
    // Create snapshot
    let snapshot_data = storage.create_snapshot_data()?;
    
    // Verify structure
    let snapshot = snapshot_data;
    assert_eq!(snapshot.vm_states.len(), 2, "Should have 2 VM states");
    assert_eq!(snapshot.tasks.len(), 2, "Should have 2 tasks");
    
    Ok(())
}

/// Test snapshot restoration
#[tokio::test]
async fn test_snapshot_restoration() -> BlixardResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let storage = RedbRaftStorage { database: database.clone() };
    
    // Create test data for snapshot
    let snapshot = SnapshotData {
        vm_states: vec![
            ("vm-1".to_string(), b"state-1".to_vec()),
            ("vm-2".to_string(), b"state-2".to_vec()),
        ],
        cluster_state: vec![
            ("config".to_string(), b"cluster-config".to_vec()),
        ],
        tasks: vec![
            ("task-1".to_string(), b"task-data-1".to_vec()),
        ],
        task_assignments: vec![
            ("task-1".to_string(), b"node-1".to_vec()),
        ],
        task_results: vec![
            ("task-1".to_string(), b"result-1".to_vec()),
        ],
        workers: vec![
            (b"worker-1".to_vec(), b"worker-data-1".to_vec()),
        ],
        worker_status: vec![
            (b"worker-1".to_vec(), b"active".to_vec()),
        ],
    };
    
    let snapshot_bytes = bincode::serialize(&snapshot)?;
    
    // Restore snapshot
    storage.restore_from_snapshot(&snapshot_bytes)?;
    
    // Verify data was restored
    {
        let read_txn = database.begin_read()?;
        
        // Check VM states
        let vm_table = read_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
        assert_eq!(vm_table.get("vm-1")?.map(|v| v.value().to_vec()), Some(b"state-1".to_vec()));
        assert_eq!(vm_table.get("vm-2")?.map(|v| v.value().to_vec()), Some(b"state-2".to_vec()));
        
        // Check tasks
        let task_table = read_txn.open_table(blixard::storage::TASK_TABLE)?;
        assert_eq!(task_table.get("task-1")?.map(|v| v.value().to_vec()), Some(b"task-data-1".to_vec()));
    }
    
    Ok(())
}

/// Test state machine snapshot application
#[tokio::test]
async fn test_state_machine_apply_snapshot() -> BlixardResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let state_machine = RaftStateMachine::new(database.clone(), std::sync::Weak::new());
    
    // Create test snapshot data
    let snapshot = SnapshotData {
        vm_states: vec![
            ("applied-vm-1".to_string(), b"applied-state-1".to_vec()),
            ("applied-vm-2".to_string(), b"applied-state-2".to_vec()),
        ],
        cluster_state: vec![],
        tasks: vec![
            ("applied-task-1".to_string(), b"task-data".to_vec()),
        ],
        task_assignments: vec![],
        task_results: vec![],
        workers: vec![],
        worker_status: vec![],
    };
    
    let snapshot_bytes = bincode::serialize(&snapshot)?;
    
    // Apply snapshot to state machine
    state_machine.apply_snapshot(&snapshot_bytes)?;
    
    // Verify snapshot was applied
    {
        let read_txn = database.begin_read()?;
        
        // Check VM states were applied
        let vm_table = read_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
        assert_eq!(vm_table.get("applied-vm-1")?.map(|v| v.value().to_vec()), 
                   Some(b"applied-state-1".to_vec()));
        assert_eq!(vm_table.get("applied-vm-2")?.map(|v| v.value().to_vec()),
                   Some(b"applied-state-2".to_vec()));
        
        // Check tasks were applied
        let task_table = read_txn.open_table(blixard::storage::TASK_TABLE)?;
        assert_eq!(task_table.get("applied-task-1")?.map(|v| v.value().to_vec()),
                   Some(b"task-data".to_vec()));
    }
    
    Ok(())
}

/// Test that snapshot application clears previous state
#[tokio::test]
async fn test_snapshot_clears_previous_state() -> BlixardResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let state_machine = RaftStateMachine::new(database.clone(), std::sync::Weak::new());
    
    // Add initial data
    {
        let write_txn = database.begin_write()?;
        {
            let mut vm_table = write_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
            vm_table.insert("old-vm", b"old-state".as_slice())?;
        }
        write_txn.commit()?;
    }
    
    // Create snapshot without the old VM
    let snapshot = SnapshotData {
        vm_states: vec![
            ("new-vm".to_string(), b"new-state".to_vec()),
        ],
        cluster_state: vec![],
        tasks: vec![],
        task_assignments: vec![],
        task_results: vec![],
        workers: vec![],
        worker_status: vec![],
    };
    
    let snapshot_bytes = bincode::serialize(&snapshot)?;
    
    // Apply snapshot
    state_machine.apply_snapshot(&snapshot_bytes)?;
    
    // Verify old state was cleared and new state exists
    {
        let read_txn = database.begin_read()?;
        let vm_table = read_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
        
        // Old VM should be gone
        assert!(vm_table.get("old-vm")?.is_none(), "Old VM should be removed");
        
        // New VM should exist
        assert_eq!(vm_table.get("new-vm")?.map(|v| v.value().to_vec()),
                   Some(b"new-state".to_vec()));
    }
    
    Ok(())
}

/// Test snapshot with large dataset
#[tokio::test]
async fn test_large_snapshot() -> BlixardResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let storage = RedbRaftStorage { database: database.clone() };
    
    // Create large dataset
    {
        let write_txn = database.begin_write()?;
        {
            let mut vm_table = write_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
            for i in 0..100 {
                let key = format!("vm-{}", i);
                let value = format!("state-{}", i);
                vm_table.insert(key.as_str(), value.as_bytes())?;
            }
        }
        {
            let mut task_table = write_txn.open_table(blixard::storage::TASK_TABLE)?;
            for i in 0..100 {
                let key = format!("task-{}", i);
                let value = format!("task-data-{}", i);
                task_table.insert(key.as_str(), value.as_bytes())?;
            }
        }
        write_txn.commit()?;
    }
    
    // Create snapshot
    let snapshot_data = storage.create_snapshot_data()?;
    
    // Verify snapshot contains all data
    assert_eq!(snapshot_data.vm_states.len(), 100, "Should have 100 VM states");
    assert_eq!(snapshot_data.tasks.len(), 100, "Should have 100 tasks");
    
    // Create new database and restore
    let new_db_path = temp_dir.path().join("restored.db");
    let new_database = std::sync::Arc::new(redb::Database::create(&new_db_path)?);
    let new_storage = RedbRaftStorage { database: new_database.clone() };
    
    let snapshot_bytes = bincode::serialize(&snapshot_data)?;
    new_storage.restore_from_snapshot(&snapshot_bytes)?;
    
    // Verify all data was restored
    {
        let read_txn = new_database.begin_read()?;
        let vm_table = read_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
        
        for i in 0..100 {
            let key = format!("vm-{}", i);
            let expected_value = format!("state-{}", i);
            let actual = vm_table.get(key.as_str())?.map(|v| v.value().to_vec());
            assert_eq!(actual, Some(expected_value.into_bytes()));
        }
    }
    
    Ok(())
}

/// Test concurrent snapshot operations
#[tokio::test]
async fn test_concurrent_snapshot_safety() -> BlixardResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let database = std::sync::Arc::new(redb::Database::create(&db_path)?);
    let storage = std::sync::Arc::new(RedbRaftStorage { database: database.clone() });
    
    // Add some initial data
    {
        let write_txn = database.begin_write()?;
        {
            let mut vm_table = write_txn.open_table(blixard::storage::VM_STATE_TABLE)?;
            vm_table.insert("initial-vm", b"initial-state".as_slice())?;
        }
        write_txn.commit()?;
    }
    
    // Spawn multiple tasks creating snapshots concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            // Each task creates a snapshot
            let snapshot = storage_clone.create_snapshot_data()?;
            
            // Verify snapshot is valid
            assert!(!snapshot.vm_states.is_empty(), "Task {} snapshot should have data", i);
            Ok::<_, blixard::error::BlixardError>(())
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap()?;
    }
    
    Ok(())
}