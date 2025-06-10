use anyhow::Result;
use blixard::storage::Storage;
use blixard::state_machine::{ServiceInfo, ServiceState};
use tempfile::TempDir;

#[test]
fn test_basic_storage_operations() -> Result<()> {
    println!("Creating temporary directory...");
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    
    println!("Creating storage at: {:?}", db_path);
    let storage = Storage::new(&db_path)?;
    
    // Test service storage
    let service = ServiceInfo {
        name: "test-service".to_string(),
        state: ServiceState::Running,
        node: "node1".to_string(),
        timestamp: chrono::Utc::now(),
    };
    
    println!("Storing service...");
    storage.store_service(&service)?;
    
    println!("Retrieving service...");
    let retrieved = storage.get_service("test-service")?;
    assert!(retrieved.is_some(), "Service should exist");
    
    let retrieved_service = retrieved.unwrap();
    assert_eq!(retrieved_service.name, "test-service");
    assert_eq!(retrieved_service.state, ServiceState::Running);
    assert_eq!(retrieved_service.node, "node1");
    
    println!("✓ Basic storage operations work!");
    Ok(())
}

#[test] 
fn test_persistence_across_reopens() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("persist.db");
    
    // Create and store data
    {
        println!("First session: creating storage and storing service...");
        let storage = Storage::new(&db_path)?;
        let service = ServiceInfo {
            name: "persistent-service".to_string(),
            state: ServiceState::Stopped,
            node: "node2".to_string(),
            timestamp: chrono::Utc::now(),
        };
        storage.store_service(&service)?;
        println!("Service stored, closing database...");
    } // Storage dropped here
    
    // Reopen and verify
    {
        println!("\nSecond session: reopening storage...");
        let storage = Storage::new(&db_path)?;
        let service = storage.get_service("persistent-service")?;
        
        assert!(service.is_some(), "Service should persist across reopens");
        let service = service.unwrap();
        assert_eq!(service.name, "persistent-service");
        assert_eq!(service.state, ServiceState::Stopped);
        assert_eq!(service.node, "node2");
        
        println!("✓ Data persisted correctly!");
    }
    
    Ok(())
}