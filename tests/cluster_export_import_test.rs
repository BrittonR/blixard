//! Integration tests for cluster state export/import functionality

#[cfg(test)]
mod tests {
    use blixard_core::cluster_state::{ClusterStateManager, ExportOptions};
    use blixard_core::storage::{Storage, RedbRaftStorage};
    use blixard_core::iroh_transport::IrohTransport;
    use tempfile::TempDir;
    use std::sync::Arc;
    use redb::Database;
    
    fn create_test_storage(temp_dir: &TempDir) -> Arc<dyn Storage> {
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());
        Arc::new(RedbRaftStorage { database })
    }
    
    #[tokio::test]
    async fn test_cluster_export_import_basic() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        let options = ExportOptions::default();
        
        // Export state
        let state = manager.export_state("test-cluster", &options).await.unwrap();
        
        assert_eq!(state.cluster_name, "test-cluster");
        assert_eq!(state.exported_by_node, 1);
        assert_eq!(state.version, "1.0");
        
        // Import state
        manager.import_state(&state, false).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_cluster_export_import_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        let options = ExportOptions {
            compress: true,
            ..Default::default()
        };
        
        let export_path = temp_dir.path().join("cluster-export.json.gz");
        
        // Export to file
        manager.export_to_file("prod-cluster", &export_path, &options).await.unwrap();
        
        // Verify file exists
        assert!(export_path.exists());
        
        // Import from file
        manager.import_from_file(&export_path, false).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_cluster_export_import_p2p() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();
        
        // Node 1: Export via P2P
        let storage1 = create_test_storage(&temp_dir1);
        let transport1 = Arc::new(IrohTransport::new(1, temp_dir1.path()).await.unwrap());
        let manager1 = ClusterStateManager::new(1, storage1, Some(transport1.clone()));
        
        let options = ExportOptions::default();
        let ticket = manager1.share_state_p2p("p2p-cluster", &options).await.unwrap();
        
        // Node 2: Import via P2P
        let storage2 = create_test_storage(&temp_dir2);
        let transport2 = Arc::new(IrohTransport::new(2, temp_dir2.path()).await.unwrap());
        let manager2 = ClusterStateManager::new(2, storage2, Some(transport2.clone()));
        
        // In a real scenario, this would work with the actual ticket
        // For testing, we'll skip the actual P2P import since it requires real network
        assert!(!ticket.is_empty());
        
        // Cleanup
        Arc::try_unwrap(transport1).unwrap().shutdown().await.unwrap();
        Arc::try_unwrap(transport2).unwrap().shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_export_with_options() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        
        // Test with various options
        let options = ExportOptions {
            include_vm_images: true,
            include_telemetry: true,
            compress: false,
            encryption_key: Some("test-key".to_string()),
        };
        
        let state = manager.export_state("feature-test", &options).await.unwrap();
        
        // Check metadata reflects options
        assert_eq!(state.metadata.get("telemetry_included"), 
                   Some(&serde_json::json!(true)));
    }
    
    #[tokio::test]
    async fn test_merge_import() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage(&temp_dir);
        
        let manager = ClusterStateManager::new(1, storage, None);
        let options = ExportOptions::default();
        
        // Export initial state
        let state1 = manager.export_state("cluster-v1", &options).await.unwrap();
        
        // Import with merge = false (replace)
        manager.import_state(&state1, false).await.unwrap();
        
        // Export second state
        let state2 = manager.export_state("cluster-v2", &options).await.unwrap();
        
        // Import with merge = true
        manager.import_state(&state2, true).await.unwrap();
    }
}