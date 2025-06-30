use blixard_core::backup_replication::{
    CrossRegionReplicator, ReplicationPolicy, ReplicationPriority,
    MockReplicatedStorage, ReplicationState, ReplicationHealth,
};
use blixard_core::backup_manager::{BackupMetadata, BackupType, ClusterInfo};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use chrono::Utc;

#[tokio::test]
async fn test_multi_region_replication() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create replicated storage with multiple regions
    let storage = Arc::new(
        MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "us-east-1".to_string())
            .await
            .unwrap()
    );
    
    // Configure replication policy
    let policy = ReplicationPolicy {
        target_regions: vec![
            "us-east-1".to_string(),
            "us-west-2".to_string(), 
            "eu-west-1".to_string(),
            "ap-southeast-1".to_string(),
        ],
        min_regions: 3,
        max_retries: 2,
        retry_backoff: Duration::from_millis(100), // Short for testing
        timeout: Duration::from_secs(30),
        verify_after_replication: true,
        max_parallel_transfers: 2,
    };
    
    let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None)
        .await
        .unwrap();
    
    replicator.start().await.unwrap();
    
    // Create multiple test backups
    let backups = vec![
        ("backup-001", b"critical backup data", ReplicationPriority::Critical),
        ("backup-002", b"normal backup data", ReplicationPriority::Normal),
        ("backup-003", b"low priority backup", ReplicationPriority::Low),
    ];
    
    for (backup_id, data, priority) in &backups {
        let metadata = create_test_metadata(backup_id, data);
        
        // Store in primary region
        storage.store(backup_id, data, &metadata).await.unwrap();
        
        // Queue for replication
        replicator.replicate_backup(backup_id, metadata, *priority).await.unwrap();
    }
    
    // Wait for replication to complete
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Verify all backups were replicated
    for (backup_id, data, _) in &backups {
        let status = replicator.get_replication_status(backup_id)
            .await
            .unwrap()
            .expect("Status should exist");
        
        assert_eq!(status.backup_id, *backup_id);
        assert_eq!(status.health, ReplicationHealth::Healthy);
        
        // Check each target region
        for region in ["us-west-2", "eu-west-1", "ap-southeast-1"] {
            let regional_status = status.regional_status.get(region)
                .expect("Regional status should exist");
            
            assert_eq!(regional_status.state, ReplicationState::Verified);
            assert!(regional_status.last_success.is_some());
            assert_eq!(regional_status.replicated_bytes, data.len() as u64);
            assert!(regional_status.last_error.is_none());
        }
        
        // Verify data exists in all regions
        let regions = storage.list_regions().await.unwrap();
        for region in regions {
            if region != "us-east-1" {
                let exists = storage.exists_in_region(backup_id, &region).await.unwrap();
                assert!(exists, "Backup {} should exist in region {}", backup_id, region);
            }
        }
    }
    
    replicator.stop().await;
}

#[tokio::test]
async fn test_replication_priority_ordering() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "us-east-1".to_string())
            .await
            .unwrap()
    );
    
    let policy = ReplicationPolicy {
        target_regions: vec!["us-east-1".to_string(), "us-west-2".to_string()],
        max_parallel_transfers: 1, // Process one at a time to verify ordering
        ..Default::default()
    };
    
    let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None)
        .await
        .unwrap();
    
    // Don't start the replicator yet - we'll queue tasks first
    
    // Queue tasks in reverse priority order
    let tasks = vec![
        ("low-1", ReplicationPriority::Low),
        ("normal-1", ReplicationPriority::Normal),
        ("critical-1", ReplicationPriority::Critical),
        ("low-2", ReplicationPriority::Low),
        ("high-1", ReplicationPriority::High),
    ];
    
    for (backup_id, priority) in tasks {
        let data = format!("data for {}", backup_id).into_bytes();
        let metadata = create_test_metadata(backup_id, &data);
        storage.store(backup_id, &data, &metadata).await.unwrap();
        replicator.replicate_backup(backup_id, metadata, priority).await.unwrap();
    }
    
    // Now start replication
    replicator.start().await.unwrap();
    
    // Monitor replication order
    let start_time = std::time::Instant::now();
    let mut completion_order = Vec::new();
    
    while completion_order.len() < 5 && start_time.elapsed() < Duration::from_secs(10) {
        for (backup_id, _) in &tasks {
            if !completion_order.contains(&backup_id.to_string()) {
                if let Some(status) = replicator.get_replication_status(backup_id).await.unwrap() {
                    if status.health == ReplicationHealth::Healthy {
                        completion_order.push(backup_id.to_string());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Verify critical and high priority were processed first
    assert_eq!(completion_order[0], "critical-1");
    assert_eq!(completion_order[1], "high-1");
    
    replicator.stop().await;
}

#[tokio::test]
async fn test_consistency_verification() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "us-east-1".to_string())
            .await
            .unwrap()
    );
    
    let policy = ReplicationPolicy {
        target_regions: vec![
            "us-east-1".to_string(),
            "us-west-2".to_string(),
            "eu-west-1".to_string(),
        ],
        verify_after_replication: true,
        ..Default::default()
    };
    
    let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None)
        .await
        .unwrap();
    
    replicator.start().await.unwrap();
    
    // Create and replicate a backup
    let backup_id = "consistency-test";
    let data = b"data to verify consistency across regions";
    let metadata = create_test_metadata(backup_id, data);
    
    storage.store(backup_id, data, &metadata).await.unwrap();
    replicator.replicate_backup(backup_id, metadata, ReplicationPriority::High).await.unwrap();
    
    // Wait for replication and verification
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Run consistency check
    let report = replicator.verify_consistency(backup_id).await.unwrap();
    
    assert_eq!(report.backup_id, backup_id);
    assert!(report.overall_consistent, "All regions should be consistent");
    
    // Check each region
    for (region, status) in &report.regional_consistency {
        assert!(status.exists, "Backup should exist in {}", region);
        assert!(status.checksum_match, "Checksum should match in {}", region);
        assert!(status.size_match, "Size should match in {}", region);
        assert!(status.metadata_match, "Metadata should match in {}", region);
        assert!(status.discrepancies.is_empty(), "No discrepancies in {}", region);
    }
    
    replicator.stop().await;
}

#[tokio::test]
async fn test_replication_with_region_failure() {
    // This test would require a more sophisticated mock that can simulate failures
    // For now, we'll test the basic retry mechanism
    
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "us-east-1".to_string())
            .await
            .unwrap()
    );
    
    let policy = ReplicationPolicy {
        target_regions: vec![
            "us-east-1".to_string(),
            "us-west-2".to_string(),
            "nonexistent-region".to_string(), // This will fail
        ],
        max_retries: 2,
        retry_backoff: Duration::from_millis(100),
        ..Default::default()
    };
    
    let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None)
        .await
        .unwrap();
    
    replicator.start().await.unwrap();
    
    // Create a backup
    let backup_id = "retry-test";
    let data = b"test data for retry logic";
    let metadata = create_test_metadata(backup_id, data);
    
    storage.store(backup_id, data, &metadata).await.unwrap();
    replicator.replicate_backup(backup_id, metadata, ReplicationPriority::Normal).await.unwrap();
    
    // Wait for initial attempt and retries
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check status
    let status = replicator.get_replication_status(backup_id).await.unwrap().unwrap();
    
    // Should succeed for valid region
    let west_status = status.regional_status.get("us-west-2").unwrap();
    assert_eq!(west_status.state, ReplicationState::Verified);
    
    // Should fail for nonexistent region after retries
    let failed_status = status.regional_status.get("nonexistent-region").unwrap();
    assert_eq!(failed_status.state, ReplicationState::Failed);
    assert!(failed_status.retry_count >= 2);
    assert!(failed_status.last_error.is_some());
    
    // Overall health should be unhealthy due to failure
    assert_eq!(status.health, ReplicationHealth::Unhealthy);
    
    replicator.stop().await;
}

#[tokio::test]
async fn test_list_replication_status() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(
        MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "eu-west-1".to_string())
            .await
            .unwrap()
    );
    
    let policy = ReplicationPolicy {
        target_regions: vec![
            "eu-west-1".to_string(),
            "us-east-1".to_string(),
        ],
        ..Default::default()
    };
    
    let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None)
        .await
        .unwrap();
    
    replicator.start().await.unwrap();
    
    // Create multiple backups
    for i in 0..5 {
        let backup_id = format!("list-test-{}", i);
        let data = format!("backup data {}", i).into_bytes();
        let metadata = create_test_metadata(&backup_id, &data);
        
        storage.store(&backup_id, &data, &metadata).await.unwrap();
        replicator.replicate_backup(&backup_id, metadata, ReplicationPriority::Normal).await.unwrap();
    }
    
    // Wait for replication
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // List all statuses
    let statuses = replicator.list_replication_status().await.unwrap();
    
    assert_eq!(statuses.len(), 5);
    
    for status in statuses {
        assert!(status.backup_id.starts_with("list-test-"));
        assert_eq!(status.primary_region, "eu-west-1");
        assert_eq!(status.health, ReplicationHealth::Healthy);
    }
    
    replicator.stop().await;
}

// Helper function to create test metadata
fn create_test_metadata(backup_id: &str, data: &[u8]) -> BackupMetadata {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(data);
    let checksum = format!("{:x}", hasher.finalize());
    
    BackupMetadata {
        id: backup_id.to_string(),
        timestamp: Utc::now(),
        version: 1,
        size_bytes: data.len() as u64,
        backup_type: BackupType::Full,
        cluster_info: ClusterInfo {
            node_count: 3,
            vm_count: 10,
            leader_id: Some(1),
            cluster_id: "test-cluster".to_string(),
        },
        checksum,
        raft_index: 100,
        raft_term: 5,
    }
}