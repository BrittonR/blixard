//! Cross-region backup replication for disaster recovery
//!
//! This module provides functionality to replicate backups across multiple
//! regions for disaster recovery and data durability.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use crate::error::{BlixardError, BlixardResult};
use crate::backup_manager::{BackupStorage, BackupMetadata, LocalBackupStorage};
use crate::audit_log::{AuditLogger, AuditEvent, AuditCategory, AuditSeverity, AuditActor, AuditOutcome};
use tracing::{info, warn, error, debug};

/// Replication status for a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Backup ID
    pub backup_id: String,
    /// Primary region where backup was created
    pub primary_region: String,
    /// Status of replication to each region
    pub regional_status: HashMap<String, RegionalReplicationStatus>,
    /// Overall replication health
    pub health: ReplicationHealth,
    /// Last update time
    pub last_updated: DateTime<Utc>,
}

/// Status of replication to a specific region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalReplicationStatus {
    /// Current state of replication
    pub state: ReplicationState,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Last successful sync time
    pub last_success: Option<DateTime<Utc>>,
    /// Last error if any
    pub last_error: Option<String>,
    /// Size of replicated data
    pub replicated_bytes: u64,
}

/// State of replication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationState {
    /// Not yet started
    Pending,
    /// Currently replicating
    InProgress,
    /// Successfully replicated
    Completed,
    /// Failed after retries
    Failed,
    /// Verification in progress
    Verifying,
    /// Verified and consistent
    Verified,
}

/// Overall replication health
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationHealth {
    /// All regions successfully replicated
    Healthy,
    /// Some regions pending or in progress
    Degraded,
    /// One or more regions failed
    Unhealthy,
}

/// Consistency report for cross-region verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    /// Backup ID being verified
    pub backup_id: String,
    /// Verification timestamp
    pub timestamp: DateTime<Utc>,
    /// Consistency status for each region
    pub regional_consistency: HashMap<String, ConsistencyStatus>,
    /// Overall consistency
    pub overall_consistent: bool,
}

/// Consistency status for a region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyStatus {
    /// Whether the backup exists in this region
    pub exists: bool,
    /// Whether checksums match
    pub checksum_match: bool,
    /// Size comparison
    pub size_match: bool,
    /// Metadata consistency
    pub metadata_match: bool,
    /// Any discrepancies found
    pub discrepancies: Vec<String>,
}

/// Replication policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationPolicy {
    /// Target regions for replication
    pub target_regions: Vec<String>,
    /// Minimum number of regions for durability
    pub min_regions: usize,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry backoff configuration
    pub retry_backoff: Duration,
    /// Replication timeout
    pub timeout: Duration,
    /// Whether to verify after replication
    pub verify_after_replication: bool,
    /// Parallel replication limit
    pub max_parallel_transfers: usize,
}

impl Default for ReplicationPolicy {
    fn default() -> Self {
        Self {
            target_regions: vec!["us-west-2".to_string(), "eu-west-1".to_string()],
            min_regions: 2,
            max_retries: 3,
            retry_backoff: Duration::from_secs(60),
            timeout: Duration::from_secs(3600), // 1 hour
            verify_after_replication: true,
            max_parallel_transfers: 3,
        }
    }
}

/// Replication task for async processing
#[derive(Debug, Clone)]
struct ReplicationTask {
    pub backup_id: String,
    pub source_region: String,
    pub target_region: String,
    pub metadata: BackupMetadata,
    pub priority: ReplicationPriority,
    pub created_at: SystemTime,
}

/// Priority for replication tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReplicationPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Extended backup storage trait with replication capabilities
#[async_trait]
pub trait ReplicatedBackupStorage: BackupStorage {
    /// Get the region identifier for this storage
    async fn region(&self) -> BlixardResult<String>;
    
    /// List available regions for replication
    async fn list_regions(&self) -> BlixardResult<Vec<String>>;
    
    /// Check if a backup exists in this region
    async fn exists_in_region(&self, backup_id: &str, region: &str) -> BlixardResult<bool>;
    
    /// Get storage endpoint for a specific region
    async fn get_regional_storage(&self, region: &str) -> BlixardResult<Arc<dyn BackupStorage>>;
}

/// Mock implementation of replicated storage for testing
pub struct MockReplicatedStorage {
    local_storage: Arc<LocalBackupStorage>,
    region: String,
    available_regions: Vec<String>,
    regional_storages: Arc<RwLock<HashMap<String, Arc<LocalBackupStorage>>>>,
}

impl MockReplicatedStorage {
    pub async fn new(base_path: std::path::PathBuf, region: String) -> BlixardResult<Self> {
        let local_storage = Arc::new(LocalBackupStorage::new(base_path.join(&region))?);
        
        let available_regions = vec![
            "us-east-1".to_string(),
            "us-west-2".to_string(),
            "eu-west-1".to_string(),
            "ap-southeast-1".to_string(),
        ];
        
        let mut regional_storages = HashMap::new();
        for r in &available_regions {
            let storage = Arc::new(LocalBackupStorage::new(base_path.join(r))?);
            regional_storages.insert(r.clone(), storage);
        }
        
        Ok(Self {
            local_storage,
            region,
            available_regions,
            regional_storages: Arc::new(RwLock::new(regional_storages)),
        })
    }
}

#[async_trait]
impl BackupStorage for MockReplicatedStorage {
    async fn store(&self, backup_id: &str, data: &[u8], metadata: &BackupMetadata) -> BlixardResult<()> {
        self.local_storage.store(backup_id, data, metadata).await
    }
    
    async fn retrieve(&self, backup_id: &str) -> BlixardResult<(Vec<u8>, BackupMetadata)> {
        self.local_storage.retrieve(backup_id).await
    }
    
    async fn list(&self) -> BlixardResult<Vec<BackupMetadata>> {
        self.local_storage.list().await
    }
    
    async fn delete(&self, backup_id: &str) -> BlixardResult<()> {
        self.local_storage.delete(backup_id).await
    }
    
    async fn exists(&self, backup_id: &str) -> BlixardResult<bool> {
        self.local_storage.exists(backup_id).await
    }
}

#[async_trait]
impl ReplicatedBackupStorage for MockReplicatedStorage {
    async fn region(&self) -> BlixardResult<String> {
        Ok(self.region.clone())
    }
    
    async fn list_regions(&self) -> BlixardResult<Vec<String>> {
        Ok(self.available_regions.clone())
    }
    
    async fn exists_in_region(&self, backup_id: &str, region: &str) -> BlixardResult<bool> {
        let storages = self.regional_storages.read().await;
        if let Some(storage) = storages.get(region) {
            storage.exists(backup_id).await
        } else {
            Ok(false)
        }
    }
    
    async fn get_regional_storage(&self, region: &str) -> BlixardResult<Arc<dyn BackupStorage>> {
        let storages = self.regional_storages.read().await;
        storages.get(region)
            .cloned()
            .map(|s| s as Arc<dyn BackupStorage>)
            .ok_or_else(|| BlixardError::NotFound {
                entity: "region".to_string(),
                id: region.to_string(),
            })
    }
}

/// Cross-region backup replicator
pub struct CrossRegionReplicator {
    /// Primary storage in the local region
    primary_storage: Arc<dyn ReplicatedBackupStorage>,
    /// Current region
    primary_region: String,
    /// Replication policy
    policy: ReplicationPolicy,
    /// Replication status tracking
    status_tracker: Arc<RwLock<HashMap<String, ReplicationStatus>>>,
    /// Task queue for async replication
    task_queue: Arc<Mutex<Vec<ReplicationTask>>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Audit logger
    audit_logger: Option<Arc<AuditLogger>>,
}

impl CrossRegionReplicator {
    /// Create a new cross-region replicator
    pub async fn new(
        primary_storage: Arc<dyn ReplicatedBackupStorage>,
        policy: ReplicationPolicy,
        audit_logger: Option<Arc<AuditLogger>>,
    ) -> BlixardResult<Self> {
        let primary_region = primary_storage.region().await?;
        
        Ok(Self {
            primary_storage,
            primary_region,
            policy,
            status_tracker: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(Mutex::new(Vec::new())),
            shutdown_tx: None,
            audit_logger,
        })
    }
    
    /// Start the replication worker
    pub async fn start(&mut self) -> BlixardResult<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        let storage = self.primary_storage.clone();
        let policy = self.policy.clone();
        let status_tracker = self.status_tracker.clone();
        let task_queue = self.task_queue.clone();
        let audit_logger = self.audit_logger.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Cross-region replicator shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(e) = Self::process_replication_queue(
                            &storage,
                            &policy,
                            &status_tracker,
                            &task_queue,
                            audit_logger.as_ref(),
                        ).await {
                            error!("Error processing replication queue: {}", e);
                        }
                    }
                }
            }
        });
        
        info!("Cross-region replicator started for region {}", self.primary_region);
        Ok(())
    }
    
    /// Stop the replication worker
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
    
    /// Queue a backup for replication
    pub async fn replicate_backup(
        &self,
        backup_id: &str,
        metadata: BackupMetadata,
        priority: ReplicationPriority,
    ) -> BlixardResult<()> {
        // Initialize replication status
        let mut status = ReplicationStatus {
            backup_id: backup_id.to_string(),
            primary_region: self.primary_region.clone(),
            regional_status: HashMap::new(),
            health: ReplicationHealth::Degraded,
            last_updated: Utc::now(),
        };
        
        // Create tasks for each target region
        let mut queue = self.task_queue.lock().await;
        for region in &self.policy.target_regions {
            if region != &self.primary_region {
                queue.push(ReplicationTask {
                    backup_id: backup_id.to_string(),
                    source_region: self.primary_region.clone(),
                    target_region: region.clone(),
                    metadata: metadata.clone(),
                    priority,
                    created_at: SystemTime::now(),
                });
                
                status.regional_status.insert(region.clone(), RegionalReplicationStatus {
                    state: ReplicationState::Pending,
                    retry_count: 0,
                    last_success: None,
                    last_error: None,
                    replicated_bytes: 0,
                });
            }
        }
        
        // Sort queue by priority
        queue.sort_by_key(|t| std::cmp::Reverse(t.priority));
        
        // Store initial status
        self.status_tracker.write().await.insert(backup_id.to_string(), status);
        
        info!("Queued backup {} for replication to {} regions", 
            backup_id, self.policy.target_regions.len() - 1);
        
        Ok(())
    }
    
    /// Process the replication queue
    async fn process_replication_queue(
        storage: &Arc<dyn ReplicatedBackupStorage>,
        policy: &ReplicationPolicy,
        status_tracker: &Arc<RwLock<HashMap<String, ReplicationStatus>>>,
        task_queue: &Arc<Mutex<Vec<ReplicationTask>>>,
        audit_logger: Option<&Arc<AuditLogger>>,
    ) -> BlixardResult<()> {
        // Get next batch of tasks
        let tasks = {
            let mut queue = task_queue.lock().await;
            let batch_size = policy.max_parallel_transfers.min(queue.len());
            queue.drain(0..batch_size).collect::<Vec<_>>()
        };
        
        if tasks.is_empty() {
            return Ok(());
        }
        
        debug!("Processing {} replication tasks", tasks.len());
        
        // Process tasks in parallel
        let mut handles = Vec::new();
        for task in tasks {
            let storage = storage.clone();
            let status_tracker = status_tracker.clone();
            let task_queue = task_queue.clone();
            let policy = policy.clone();
            let audit_logger = audit_logger.cloned();
            
            let handle = tokio::spawn(async move {
                Self::replicate_single_backup(
                    &storage,
                    &task,
                    &policy,
                    &status_tracker,
                    &task_queue,
                    audit_logger.as_ref(),
                ).await
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Replication task failed: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Replicate a single backup to a target region
    async fn replicate_single_backup(
        storage: &Arc<dyn ReplicatedBackupStorage>,
        task: &ReplicationTask,
        policy: &ReplicationPolicy,
        status_tracker: &Arc<RwLock<HashMap<String, ReplicationStatus>>>,
        task_queue: &Arc<Mutex<Vec<ReplicationTask>>>,
        audit_logger: Option<&Arc<AuditLogger>>,
    ) -> BlixardResult<()> {
        info!("Replicating backup {} from {} to {}", 
            task.backup_id, task.source_region, task.target_region);
        
        // Update status to in-progress
        Self::update_regional_status(
            status_tracker,
            &task.backup_id,
            &task.target_region,
            ReplicationState::InProgress,
            None,
        ).await?;
        
        // Retrieve backup data
        let (data, metadata) = match storage.retrieve(&task.backup_id).await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to retrieve backup {}: {}", task.backup_id, e);
                return Self::handle_replication_failure(
                    task,
                    policy,
                    status_tracker,
                    task_queue,
                    format!("Failed to retrieve backup: {}", e),
                ).await;
            }
        };
        
        // Get target region storage
        let target_storage = match storage.get_regional_storage(&task.target_region).await {
            Ok(s) => s,
            Err(e) => {
                return Self::handle_replication_failure(
                    task,
                    policy,
                    status_tracker,
                    task_queue,
                    format!("Failed to get regional storage: {}", e),
                ).await;
            }
        };
        
        // Perform the replication
        match target_storage.store(&task.backup_id, &data, &metadata).await {
            Ok(_) => {
                info!("Successfully replicated backup {} to {}", 
                    task.backup_id, task.target_region);
                
                // Update status
                Self::update_regional_status(
                    status_tracker,
                    &task.backup_id,
                    &task.target_region,
                    ReplicationState::Completed,
                    Some(data.len() as u64),
                ).await?;
                
                // Verify if configured
                if policy.verify_after_replication {
                    Self::verify_replication(
                        storage,
                        &task.backup_id,
                        &task.target_region,
                        &metadata,
                        status_tracker,
                    ).await?;
                }
                
                // Audit log success
                if let Some(logger) = audit_logger {
                    Self::audit_replication_event(
                        logger,
                        &task.backup_id,
                        &task.target_region,
                        true,
                        None,
                    ).await?;
                }
                
                Ok(())
            }
            Err(e) => {
                Self::handle_replication_failure(
                    task,
                    policy,
                    status_tracker,
                    task_queue,
                    format!("Failed to store in target region: {}", e),
                ).await
            }
        }
    }
    
    /// Handle replication failure with retry logic
    async fn handle_replication_failure(
        task: &ReplicationTask,
        policy: &ReplicationPolicy,
        status_tracker: &Arc<RwLock<HashMap<String, ReplicationStatus>>>,
        task_queue: &Arc<Mutex<Vec<ReplicationTask>>>,
        error: String,
    ) -> BlixardResult<()> {
        error!("Replication failed for {} to {}: {}", 
            task.backup_id, task.target_region, error);
        
        // Get current retry count
        let retry_count = {
            let tracker = status_tracker.read().await;
            tracker.get(&task.backup_id)
                .and_then(|s| s.regional_status.get(&task.target_region))
                .map(|rs| rs.retry_count)
                .unwrap_or(0)
        };
        
        if retry_count < policy.max_retries {
            // Queue for retry
            let mut retry_task = task.clone();
            retry_task.created_at = SystemTime::now() + policy.retry_backoff * (retry_count + 1);
            
            task_queue.lock().await.push(retry_task);
            
            // Update status with retry info
            let mut tracker = status_tracker.write().await;
            if let Some(status) = tracker.get_mut(&task.backup_id) {
                if let Some(regional) = status.regional_status.get_mut(&task.target_region) {
                    regional.retry_count = retry_count + 1;
                    regional.last_error = Some(error);
                }
                status.last_updated = Utc::now();
            }
            
            info!("Queued retry {} for backup {} to {}", 
                retry_count + 1, task.backup_id, task.target_region);
        } else {
            // Mark as failed after max retries
            Self::update_regional_status(
                status_tracker,
                &task.backup_id,
                &task.target_region,
                ReplicationState::Failed,
                None,
            ).await?;
            
            error!("Replication permanently failed for {} to {} after {} retries", 
                task.backup_id, task.target_region, policy.max_retries);
        }
        
        Ok(())
    }
    
    /// Verify replication consistency
    async fn verify_replication(
        storage: &Arc<dyn ReplicatedBackupStorage>,
        backup_id: &str,
        target_region: &str,
        expected_metadata: &BackupMetadata,
        status_tracker: &Arc<RwLock<HashMap<String, ReplicationStatus>>>,
    ) -> BlixardResult<()> {
        debug!("Verifying replication of {} to {}", backup_id, target_region);
        
        // Update status to verifying
        Self::update_regional_status(
            status_tracker,
            backup_id,
            target_region,
            ReplicationState::Verifying,
            None,
        ).await?;
        
        // Get target storage and verify
        let target_storage = storage.get_regional_storage(target_region).await?;
        
        match target_storage.retrieve(backup_id).await {
            Ok((data, metadata)) => {
                // Verify metadata matches
                let metadata_match = metadata.id == expected_metadata.id
                    && metadata.size_bytes == expected_metadata.size_bytes
                    && metadata.checksum == expected_metadata.checksum;
                
                if metadata_match {
                    // Calculate actual checksum
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    let actual_checksum = format!("{:x}", hasher.finalize());
                    
                    if actual_checksum == metadata.checksum {
                        Self::update_regional_status(
                            status_tracker,
                            backup_id,
                            target_region,
                            ReplicationState::Verified,
                            None,
                        ).await?;
                        
                        debug!("Verification successful for {} in {}", backup_id, target_region);
                    } else {
                        warn!("Checksum mismatch for {} in {}", backup_id, target_region);
                        Self::update_regional_status(
                            status_tracker,
                            backup_id,
                            target_region,
                            ReplicationState::Failed,
                            None,
                        ).await?;
                    }
                } else {
                    warn!("Metadata mismatch for {} in {}", backup_id, target_region);
                    Self::update_regional_status(
                        status_tracker,
                        backup_id,
                        target_region,
                        ReplicationState::Failed,
                        None,
                    ).await?;
                }
            }
            Err(e) => {
                error!("Failed to verify replication: {}", e);
                Self::update_regional_status(
                    status_tracker,
                    backup_id,
                    target_region,
                    ReplicationState::Failed,
                    None,
                ).await?;
            }
        }
        
        Ok(())
    }
    
    /// Update regional replication status
    async fn update_regional_status(
        status_tracker: &Arc<RwLock<HashMap<String, ReplicationStatus>>>,
        backup_id: &str,
        region: &str,
        state: ReplicationState,
        replicated_bytes: Option<u64>,
    ) -> BlixardResult<()> {
        let mut tracker = status_tracker.write().await;
        
        if let Some(status) = tracker.get_mut(backup_id) {
            if let Some(regional) = status.regional_status.get_mut(region) {
                regional.state = state;
                
                if state == ReplicationState::Completed || state == ReplicationState::Verified {
                    regional.last_success = Some(Utc::now());
                    if let Some(bytes) = replicated_bytes {
                        regional.replicated_bytes = bytes;
                    }
                }
            }
            
            // Update overall health
            let failed_count = status.regional_status.values()
                .filter(|rs| rs.state == ReplicationState::Failed)
                .count();
            
            let completed_count = status.regional_status.values()
                .filter(|rs| matches!(rs.state, ReplicationState::Completed | ReplicationState::Verified))
                .count();
            
            status.health = if failed_count > 0 {
                ReplicationHealth::Unhealthy
            } else if completed_count == status.regional_status.len() {
                ReplicationHealth::Healthy
            } else {
                ReplicationHealth::Degraded
            };
            
            status.last_updated = Utc::now();
        }
        
        Ok(())
    }
    
    /// Audit log replication event
    async fn audit_replication_event(
        logger: &Arc<AuditLogger>,
        backup_id: &str,
        target_region: &str,
        success: bool,
        error: Option<String>,
    ) -> BlixardResult<()> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category: AuditCategory::DataAccess,
            severity: if success { AuditSeverity::Info } else { AuditSeverity::Warning },
            event_type: "backup.replicated".to_string(),
            actor: AuditActor::System {
                component: "backup-replicator".to_string(),
                node_id: 0, // TODO: Get actual node ID
            },
            resource: None,
            outcome: if success {
                AuditOutcome::Success
            } else {
                AuditOutcome::Failure {
                    reason: error.unwrap_or_else(|| "Unknown error".to_string()),
                }
            },
            details: serde_json::json!({
                "backup_id": backup_id,
                "target_region": target_region,
            }),
            correlation_id: None,
            source_ip: None,
            node_id: 0, // TODO: Get actual node ID
        };
        
        logger.log(event).await
    }
    
    /// Get replication status for a backup
    pub async fn get_replication_status(&self, backup_id: &str) -> BlixardResult<Option<ReplicationStatus>> {
        Ok(self.status_tracker.read().await.get(backup_id).cloned())
    }
    
    /// Get all replication statuses
    pub async fn list_replication_status(&self) -> BlixardResult<Vec<ReplicationStatus>> {
        Ok(self.status_tracker.read().await.values().cloned().collect())
    }
    
    /// Verify consistency across all regions
    pub async fn verify_consistency(&self, backup_id: &str) -> BlixardResult<ConsistencyReport> {
        let regions = self.primary_storage.list_regions().await?;
        let mut report = ConsistencyReport {
            backup_id: backup_id.to_string(),
            timestamp: Utc::now(),
            regional_consistency: HashMap::new(),
            overall_consistent: true,
        };
        
        // Get primary backup metadata
        let (primary_data, primary_metadata) = self.primary_storage.retrieve(backup_id).await?;
        let primary_checksum = {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(&primary_data);
            format!("{:x}", hasher.finalize())
        };
        
        // Check each region
        for region in regions {
            let mut status = ConsistencyStatus {
                exists: false,
                checksum_match: false,
                size_match: false,
                metadata_match: false,
                discrepancies: Vec::new(),
            };
            
            if let Ok(storage) = self.primary_storage.get_regional_storage(&region).await {
                if let Ok((data, metadata)) = storage.retrieve(backup_id).await {
                    status.exists = true;
                    
                    // Check size
                    status.size_match = data.len() as u64 == primary_metadata.size_bytes;
                    if !status.size_match {
                        status.discrepancies.push(format!(
                            "Size mismatch: {} vs {}", 
                            data.len(), 
                            primary_metadata.size_bytes
                        ));
                    }
                    
                    // Check checksum
                    let checksum = {
                        use sha2::{Sha256, Digest};
                        let mut hasher = Sha256::new();
                        hasher.update(&data);
                        format!("{:x}", hasher.finalize())
                    };
                    
                    status.checksum_match = checksum == primary_checksum;
                    if !status.checksum_match {
                        status.discrepancies.push("Checksum mismatch".to_string());
                    }
                    
                    // Check metadata
                    status.metadata_match = metadata.id == primary_metadata.id
                        && metadata.timestamp == primary_metadata.timestamp;
                    if !status.metadata_match {
                        status.discrepancies.push("Metadata mismatch".to_string());
                    }
                } else {
                    status.discrepancies.push("Backup not found in region".to_string());
                }
            } else {
                status.discrepancies.push("Regional storage unavailable".to_string());
            }
            
            if !status.exists || !status.checksum_match || !status.size_match || !status.metadata_match {
                report.overall_consistent = false;
            }
            
            report.regional_consistency.insert(region, status);
        }
        
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_cross_region_replication() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(
            MockReplicatedStorage::new(temp_dir.path().to_path_buf(), "us-east-1".to_string())
                .await
                .unwrap()
        );
        
        let policy = ReplicationPolicy {
            target_regions: vec!["us-east-1".to_string(), "us-west-2".to_string(), "eu-west-1".to_string()],
            verify_after_replication: true,
            ..Default::default()
        };
        
        let mut replicator = CrossRegionReplicator::new(storage.clone(), policy, None).await.unwrap();
        replicator.start().await.unwrap();
        
        // Create a test backup
        let backup_id = "test-backup-001";
        let data = b"test backup data for replication";
        let metadata = BackupMetadata {
            id: backup_id.to_string(),
            timestamp: Utc::now(),
            version: 1,
            size_bytes: data.len() as u64,
            backup_type: crate::backup_manager::BackupType::Full,
            cluster_info: crate::backup_manager::ClusterInfo {
                node_count: 3,
                vm_count: 10,
                leader_id: Some(1),
                cluster_id: "test-cluster".to_string(),
            },
            checksum: {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(data);
                format!("{:x}", hasher.finalize())
            },
            raft_index: 100,
            raft_term: 5,
        };
        
        // Store in primary region
        storage.store(backup_id, data, &metadata).await.unwrap();
        
        // Queue for replication
        replicator.replicate_backup(backup_id, metadata.clone(), ReplicationPriority::Normal).await.unwrap();
        
        // Wait for replication to complete
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Check replication status
        let status = replicator.get_replication_status(backup_id).await.unwrap().unwrap();
        assert_eq!(status.backup_id, backup_id);
        assert_eq!(status.primary_region, "us-east-1");
        
        // Verify replicated to other regions
        for region in ["us-west-2", "eu-west-1"] {
            let regional_status = status.regional_status.get(region).unwrap();
            assert_eq!(regional_status.state, ReplicationState::Verified);
            assert!(regional_status.last_success.is_some());
            assert_eq!(regional_status.replicated_bytes, data.len() as u64);
        }
        
        // Verify consistency
        let consistency = replicator.verify_consistency(backup_id).await.unwrap();
        assert!(consistency.overall_consistent);
        
        replicator.stop().await;
    }
    
    #[tokio::test]
    async fn test_replication_failure_and_retry() {
        // Test will simulate failures and verify retry logic
        // Implementation depends on being able to inject failures
        // This is a placeholder for the test structure
    }
}