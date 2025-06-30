use blixard_core::backup_manager::{
    BackupManager, BackupConfig, LocalBackupStorage, BackupStorage, BackupType,
};
use blixard_core::storage::{RedbRaftStorage, init_database_tables};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use redb::Database;
use tokio::time;

#[tokio::test]
async fn test_backup_manager_lifecycle() {
    // Create temporary directories
    let backup_dir = TempDir::new().unwrap();
    let db_dir = TempDir::new().unwrap();
    
    // Create database
    let db_path = db_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path).unwrap());
    init_database_tables(&database).unwrap();
    
    // Create Raft storage
    let raft_storage = Arc::new(RedbRaftStorage { database });
    
    // Create backup storage
    let backup_storage = Arc::new(LocalBackupStorage::new(backup_dir.path().to_path_buf()).unwrap());
    
    // Create backup manager with short interval for testing
    let config = BackupConfig {
        backup_interval: Duration::from_secs(1),
        max_backups: 3,
        min_backup_interval: Duration::from_millis(100),
        compression_enabled: true,
        encryption_enabled: false,
    };
    
    let mut manager = BackupManager::new(backup_storage.clone(), raft_storage, config);
    
    // Start automatic backups
    manager.start().await.unwrap();
    
    // Wait for some backups to be created
    time::sleep(Duration::from_secs(3)).await;
    
    // List backups
    let backups = manager.list_backups().await.unwrap();
    assert!(backups.len() >= 2, "Should have at least 2 backups, got {}", backups.len());
    
    // Verify backup types
    for backup in &backups {
        match &backup.backup_type {
            BackupType::Full => {
                assert!(backup.size_bytes > 0);
            }
            _ => panic!("Expected full backup type"),
        }
    }
    
    // Stop automatic backups
    manager.stop().await;
    
    // Manual backup
    let backup_id = manager.backup_now().await.unwrap();
    assert!(backup_storage.exists(&backup_id).await.unwrap());
    
    // Verify max backups retention
    time::sleep(Duration::from_secs(2)).await;
    let final_backups = manager.list_backups().await.unwrap();
    assert!(final_backups.len() <= 3, "Should have at most 3 backups due to retention policy");
}

#[tokio::test]
async fn test_backup_and_restore() {
    // Create temporary directories
    let backup_dir = TempDir::new().unwrap();
    let db_dir = TempDir::new().unwrap();
    
    // Create database
    let db_path = db_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path).unwrap());
    init_database_tables(&database).unwrap();
    
    // Add some test data
    {
        let write_txn = database.begin_write().unwrap();
        {
            let mut vm_table = write_txn.open_table(blixard_core::storage::VM_STATE_TABLE).unwrap();
            let test_vm = blixard_core::types::VmState {
                name: "test-vm".to_string(),
                config: blixard_core::types::VmConfig {
                    name: "test-vm".to_string(),
                    config_path: "/test".to_string(),
                    vcpus: 2,
                    memory: 1024,
                    ..Default::default()
                },
                status: blixard_core::types::VmStatus::Running,
                node_id: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let serialized = bincode::serialize(&test_vm).unwrap();
            vm_table.insert("test-vm", serialized.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }
    
    // Create Raft storage
    let raft_storage = Arc::new(RedbRaftStorage { database: database.clone() });
    
    // Create backup storage
    let backup_storage = Arc::new(LocalBackupStorage::new(backup_dir.path().to_path_buf()).unwrap());
    
    // Create backup manager
    let config = BackupConfig::default();
    let manager = BackupManager::new(backup_storage, raft_storage.clone(), config);
    
    // Create backup
    let backup_id = manager.backup_now().await.unwrap();
    
    // Clear the database
    {
        let write_txn = database.begin_write().unwrap();
        {
            let mut vm_table = write_txn.open_table(blixard_core::storage::VM_STATE_TABLE).unwrap();
            vm_table.remove("test-vm").unwrap();
        }
        write_txn.commit().unwrap();
    }
    
    // Verify VM is gone
    {
        let read_txn = database.begin_read().unwrap();
        let vm_table = read_txn.open_table(blixard_core::storage::VM_STATE_TABLE).unwrap();
        assert!(vm_table.get("test-vm").unwrap().is_none());
    }
    
    // Restore from backup
    manager.restore(&backup_id).await.unwrap();
    
    // Verify VM is restored
    {
        let read_txn = database.begin_read().unwrap();
        let vm_table = read_txn.open_table(blixard_core::storage::VM_STATE_TABLE).unwrap();
        let vm_data = vm_table.get("test-vm").unwrap().expect("VM should be restored");
        let restored_vm: blixard_core::types::VmState = bincode::deserialize(vm_data.value()).unwrap();
        assert_eq!(restored_vm.name, "test-vm");
        assert_eq!(restored_vm.config.vcpus, 2);
        assert_eq!(restored_vm.config.memory, 1024);
    }
}

#[tokio::test]
async fn test_backup_metadata() {
    let backup_dir = TempDir::new().unwrap();
    let storage = LocalBackupStorage::new(backup_dir.path().to_path_buf()).unwrap();
    
    // Create test backup
    let backup_id = "test-backup-metadata";
    let data = b"test backup data";
    let metadata = blixard_core::backup_manager::BackupMetadata {
        id: backup_id.to_string(),
        timestamp: chrono::Utc::now(),
        version: 1,
        size_bytes: data.len() as u64,
        backup_type: BackupType::Full,
        cluster_info: blixard_core::backup_manager::ClusterInfo {
            node_count: 3,
            vm_count: 10,
            leader_id: Some(1),
            cluster_id: "test-cluster".to_string(),
        },
        checksum: "test-checksum".to_string(),
    };
    
    // Store backup
    storage.store(backup_id, data, &metadata).await.unwrap();
    
    // List backups
    let backups = storage.list().await.unwrap();
    assert_eq!(backups.len(), 1);
    
    let listed = &backups[0];
    assert_eq!(listed.id, backup_id);
    assert_eq!(listed.cluster_info.node_count, 3);
    assert_eq!(listed.cluster_info.vm_count, 10);
    assert_eq!(listed.cluster_info.leader_id, Some(1));
}