//! Automated backup management for cluster state
//!
//! This module provides functionality to automatically backup cluster state
//! at regular intervals and manage backup retention.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tokio::time;
use raft::Storage;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use crate::error::{BlixardError, BlixardResult};
use crate::storage::{RedbRaftStorage, SnapshotData};
use crate::types::VmState;
use tracing::{info, warn, error};

/// Backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Unique backup ID
    pub id: String,
    /// Timestamp when backup was created
    pub timestamp: DateTime<Utc>,
    /// Backup format version
    pub version: u32,
    /// Size of backup in bytes
    pub size_bytes: u64,
    /// Type of backup
    pub backup_type: BackupType,
    /// Cluster metadata at time of backup
    pub cluster_info: ClusterInfo,
    /// Checksum for verification
    pub checksum: String,
    /// Raft index at time of backup
    pub raft_index: u64,
    /// Raft term at time of backup
    pub raft_term: u64,
}

/// Type of backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupType {
    /// Full backup of all state
    Full,
    /// Incremental backup since last backup
    Incremental { 
        base_backup_id: String,
        from_index: u64,
        to_index: u64,
    },
    /// Snapshot-based backup from Raft
    Snapshot { index: u64, term: u64 },
}

/// Cluster information at time of backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    /// Number of nodes in cluster
    pub node_count: usize,
    /// Number of VMs
    pub vm_count: usize,
    /// Leader node ID
    pub leader_id: Option<u64>,
    /// Cluster ID
    pub cluster_id: String,
}

/// Incremental changes between backups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalChanges {
    /// Starting Raft index (exclusive)
    pub from_index: u64,
    /// Ending Raft index (inclusive)
    pub to_index: u64,
    /// Raft entries in this range
    pub entries: Vec<raft::prelude::Entry>,
    /// VM state changes
    pub vm_changes: Vec<VmStateChange>,
    /// Worker registration changes
    pub worker_changes: Vec<WorkerChange>,
}

/// VM state change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStateChange {
    /// Timestamp of change
    pub timestamp: DateTime<Utc>,
    /// Type of change
    pub change_type: VmChangeType,
    /// VM name
    pub vm_name: String,
    /// New state (if applicable)
    pub new_state: Option<VmState>,
}

/// Type of VM change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmChangeType {
    Created,
    Updated,
    Deleted,
    StatusChanged { from: String, to: String },
}

/// Worker registration change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerChange {
    /// Timestamp of change
    pub timestamp: DateTime<Utc>,
    /// Worker node ID
    pub node_id: u64,
    /// Type of change
    pub change_type: WorkerChangeType,
}

/// Type of worker change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerChangeType {
    Registered { capabilities: crate::raft_manager::WorkerCapabilities },
    Updated { capabilities: crate::raft_manager::WorkerCapabilities },
    Unregistered,
}

/// Backup storage backend trait
#[async_trait]
pub trait BackupStorage: Send + Sync {
    /// Store a backup
    async fn store(&self, backup_id: &str, data: &[u8], metadata: &BackupMetadata) -> BlixardResult<()>;
    
    /// Retrieve a backup
    async fn retrieve(&self, backup_id: &str) -> BlixardResult<(Vec<u8>, BackupMetadata)>;
    
    /// List available backups
    async fn list(&self) -> BlixardResult<Vec<BackupMetadata>>;
    
    /// Delete a backup
    async fn delete(&self, backup_id: &str) -> BlixardResult<()>;
    
    /// Check if backup exists
    async fn exists(&self, backup_id: &str) -> BlixardResult<bool>;
}

/// Local filesystem backup storage
pub struct LocalBackupStorage {
    base_path: PathBuf,
}

impl LocalBackupStorage {
    pub fn new(base_path: PathBuf) -> BlixardResult<Self> {
        // Create backup directory if it doesn't exist
        std::fs::create_dir_all(&base_path)
            .map_err(|e| BlixardError::Storage {
                operation: "create backup directory".to_string(),
                source: Box::new(e),
            })?;
        
        Ok(Self { base_path })
    }
    
    fn backup_path(&self, backup_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.backup", backup_id))
    }
    
    fn metadata_path(&self, backup_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.meta", backup_id))
    }
}

#[async_trait]
impl BackupStorage for LocalBackupStorage {
    async fn store(&self, backup_id: &str, data: &[u8], metadata: &BackupMetadata) -> BlixardResult<()> {
        // Write backup data
        let backup_path = self.backup_path(backup_id);
        tokio::fs::write(&backup_path, data).await
            .map_err(|e| BlixardError::Storage {
                operation: format!("write backup {}", backup_id),
                source: Box::new(e),
            })?;
        
        // Write metadata
        let metadata_json = serde_json::to_vec_pretty(metadata)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize backup metadata".to_string(),
                source: Box::new(e),
            })?;
        
        let metadata_path = self.metadata_path(backup_id);
        tokio::fs::write(&metadata_path, metadata_json).await
            .map_err(|e| BlixardError::Storage {
                operation: format!("write backup metadata {}", backup_id),
                source: Box::new(e),
            })?;
        
        Ok(())
    }
    
    async fn retrieve(&self, backup_id: &str) -> BlixardResult<(Vec<u8>, BackupMetadata)> {
        // Read backup data
        let backup_path = self.backup_path(backup_id);
        let data = tokio::fs::read(&backup_path).await
            .map_err(|e| BlixardError::Storage {
                operation: format!("read backup {}", backup_id),
                source: Box::new(e),
            })?;
        
        // Read metadata
        let metadata_path = self.metadata_path(backup_id);
        let metadata_json = tokio::fs::read(&metadata_path).await
            .map_err(|e| BlixardError::Storage {
                operation: format!("read backup metadata {}", backup_id),
                source: Box::new(e),
            })?;
        
        let metadata: BackupMetadata = serde_json::from_slice(&metadata_json)
            .map_err(|e| BlixardError::Serialization {
                operation: "deserialize backup metadata".to_string(),
                source: Box::new(e),
            })?;
        
        Ok((data, metadata))
    }
    
    async fn list(&self) -> BlixardResult<Vec<BackupMetadata>> {
        let mut backups = Vec::new();
        
        let mut entries = tokio::fs::read_dir(&self.base_path).await
            .map_err(|e| BlixardError::Storage {
                operation: "list backup directory".to_string(),
                source: Box::new(e),
            })?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| BlixardError::Storage {
                operation: "read directory entry".to_string(),
                source: Box::new(e),
            })? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("meta") {
                let metadata_json = tokio::fs::read(&path).await
                    .map_err(|e| BlixardError::Storage {
                        operation: format!("read metadata file {:?}", path),
                        source: Box::new(e),
                    })?;
                
                match serde_json::from_slice::<BackupMetadata>(&metadata_json) {
                    Ok(metadata) => backups.push(metadata),
                    Err(e) => warn!("Failed to parse backup metadata {:?}: {}", path, e),
                }
            }
        }
        
        // Sort by timestamp (newest first)
        backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(backups)
    }
    
    async fn delete(&self, backup_id: &str) -> BlixardResult<()> {
        // Delete backup data
        let backup_path = self.backup_path(backup_id);
        if backup_path.exists() {
            tokio::fs::remove_file(&backup_path).await
                .map_err(|e| BlixardError::Storage {
                    operation: format!("delete backup {}", backup_id),
                    source: Box::new(e),
                })?;
        }
        
        // Delete metadata
        let metadata_path = self.metadata_path(backup_id);
        if metadata_path.exists() {
            tokio::fs::remove_file(&metadata_path).await
                .map_err(|e| BlixardError::Storage {
                    operation: format!("delete backup metadata {}", backup_id),
                    source: Box::new(e),
                })?;
        }
        
        Ok(())
    }
    
    async fn exists(&self, backup_id: &str) -> BlixardResult<bool> {
        Ok(self.backup_path(backup_id).exists() && self.metadata_path(backup_id).exists())
    }
}

/// Backup manager configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// Interval between automatic backups
    pub backup_interval: Duration,
    /// Maximum number of backups to retain
    pub max_backups: usize,
    /// Minimum interval between backups (to prevent backup storms)
    pub min_backup_interval: Duration,
    /// Enable compression
    pub compression_enabled: bool,
    /// Enable encryption
    pub encryption_enabled: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            backup_interval: Duration::from_secs(3600), // 1 hour
            max_backups: 24,
            min_backup_interval: Duration::from_secs(300), // 5 minutes
            compression_enabled: true,
            encryption_enabled: false,
        }
    }
}

/// Automated backup manager
pub struct BackupManager {
    storage: Arc<dyn BackupStorage>,
    raft_storage: Arc<RedbRaftStorage>,
    config: BackupConfig,
    last_backup: Arc<RwLock<Option<SystemTime>>>,
    last_backup_index: Arc<RwLock<u64>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(
        storage: Arc<dyn BackupStorage>,
        raft_storage: Arc<RedbRaftStorage>,
        config: BackupConfig,
    ) -> Self {
        Self {
            storage,
            raft_storage,
            config,
            last_backup: Arc::new(RwLock::new(None)),
            last_backup_index: Arc::new(RwLock::new(0)),
            shutdown_tx: None,
        }
    }
    
    /// Start automatic backups
    pub async fn start(&mut self) -> BlixardResult<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        let storage = self.storage.clone();
        let raft_storage = self.raft_storage.clone();
        let config = self.config.clone();
        let last_backup = self.last_backup.clone();
        let last_backup_index = self.last_backup_index.clone();
        
        tokio::spawn(async move {
            let mut interval = time::interval(config.backup_interval);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Backup manager shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        // Check minimum interval
                        let should_backup = {
                            let last = last_backup.read().await;
                            match *last {
                                Some(last_time) => {
                                    let elapsed = SystemTime::now()
                                        .duration_since(last_time)
                                        .unwrap_or(Duration::from_secs(0));
                                    elapsed >= config.min_backup_interval
                                }
                                None => true,
                            }
                        };
                        
                        if should_backup {
                            match perform_backup(&storage, &raft_storage, &config).await {
                                Ok((backup_id, metadata)) => {
                                    info!("Automated backup completed: {}", backup_id);
                                    *last_backup.write().await = Some(SystemTime::now());
                                    *last_backup_index.write().await = metadata.raft_index;
                                    
                                    // Clean up old backups
                                    if let Err(e) = cleanup_old_backups(&storage, config.max_backups).await {
                                        warn!("Failed to cleanup old backups: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("Automated backup failed: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
        
        info!("Backup manager started with interval: {:?}", self.config.backup_interval);
        Ok(())
    }
    
    /// Stop automatic backups
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
            info!("Backup manager stopped");
        }
    }
    
    /// Manually trigger a backup
    pub async fn backup_now(&self) -> BlixardResult<String> {
        let (backup_id, metadata) = perform_backup(&self.storage, &self.raft_storage, &self.config).await?;
        *self.last_backup.write().await = Some(SystemTime::now());
        *self.last_backup_index.write().await = metadata.raft_index;
        Ok(backup_id)
    }
    
    /// Restore from a backup
    pub async fn restore(&self, backup_id: &str) -> BlixardResult<()> {
        let (data, metadata) = self.storage.retrieve(backup_id).await?;
        
        // Verify checksum
        let calculated_checksum = calculate_checksum(&data);
        if calculated_checksum != metadata.checksum {
            return Err(BlixardError::Internal {
                message: format!("Backup checksum mismatch for {}", backup_id),
            });
        }
        
        // Decompress if needed
        let decompressed_data = if self.config.compression_enabled {
            decompress_data(&data)?
        } else {
            data
        };
        
        // Decrypt if needed
        let decrypted_data = if self.config.encryption_enabled {
            // TODO: Implement decryption
            decompressed_data
        } else {
            decompressed_data
        };
        
        // Restore based on backup type
        match &metadata.backup_type {
            BackupType::Full => {
                // Deserialize snapshot data
                let snapshot_data: SnapshotData = bincode::deserialize(&decrypted_data)
                    .map_err(|e| BlixardError::Serialization {
                        operation: "deserialize backup data".to_string(),
                        source: Box::new(e),
                    })?;
                
                // Restore to storage
                self.raft_storage.restore_from_snapshot(&decrypted_data)?;
                
                info!("Restored from full backup {} (index: {})", backup_id, metadata.raft_index);
            }
            BackupType::Incremental { base_backup_id, from_index, to_index } => {
                // First restore the base backup
                self.restore(base_backup_id).await?;
                
                // Then apply the incremental changes
                let changes: IncrementalChanges = bincode::deserialize(&decrypted_data)
                    .map_err(|e| BlixardError::Serialization {
                        operation: "deserialize incremental data".to_string(),
                        source: Box::new(e),
                    })?;
                
                self.apply_incremental_changes(changes)?;
                
                info!("Restored from incremental backup {} (base: {}, index: {}-{})", 
                    backup_id, base_backup_id, from_index, to_index);
            }
            BackupType::Snapshot { index, term } => {
                // Apply as Raft snapshot
                let snapshot = raft::prelude::Snapshot {
                    data: decrypted_data,
                    metadata: {
                        let mut meta = raft::prelude::SnapshotMetadata::default();
                        meta.index = *index;
                        meta.term = *term;
                        meta
                    },
                };
                
                self.raft_storage.apply_snapshot(&snapshot)?;
                
                info!("Restored from snapshot backup {} (index: {}, term: {})", 
                    backup_id, index, term);
            }
        }
        
        Ok(())
    }
    
    /// Restore to a specific point in time
    pub async fn restore_to_point_in_time(&self, target_time: DateTime<Utc>) -> BlixardResult<()> {
        // Find the latest backup before the target time
        let backups = self.list_backups().await?;
        
        let mut base_backup = None;
        let mut incremental_backups = Vec::new();
        
        for backup in backups.iter() {
            if backup.timestamp <= target_time {
                match &backup.backup_type {
                    BackupType::Full => {
                        base_backup = Some(backup);
                        incremental_backups.clear();
                    }
                    BackupType::Incremental { .. } => {
                        if base_backup.is_some() {
                            incremental_backups.push(backup);
                        }
                    }
                    _ => {}
                }
            }
        }
        
        let base = base_backup.ok_or_else(|| BlixardError::NotFound {
            entity: "backup".to_string(),
            id: format!("before {}", target_time),
        })?;
        
        // Restore base backup
        info!("Restoring base backup {} for point-in-time recovery", base.id);
        self.restore(&base.id).await?;
        
        // Apply incremental backups in order
        for incremental in incremental_backups {
            info!("Applying incremental backup {} for point-in-time recovery", incremental.id);
            self.restore(&incremental.id).await?;
        }
        
        // TODO: Apply transaction logs up to the exact target time
        
        info!("Point-in-time recovery completed to {}", target_time);
        Ok(())
    }
    
    /// Create an incremental backup
    pub async fn backup_incremental(&self) -> BlixardResult<String> {
        let last_index = *self.last_backup_index.read().await;
        
        // Get the latest full backup
        let backups = self.list_backups().await?;
        let base_backup = backups.iter()
            .find(|b| matches!(b.backup_type, BackupType::Full))
            .ok_or_else(|| BlixardError::Internal {
                message: "No full backup found for incremental backup".to_string(),
            })?;
        
        // Get current Raft state
        let current_state = self.raft_storage.initial_state()?
            .ok_or_else(|| BlixardError::Internal {
                message: "No Raft state available".to_string(),
            })?;
        
        let current_index = current_state.hard_state.commit;
        
        if current_index <= last_index {
            return Err(BlixardError::Internal {
                message: "No new changes since last backup".to_string(),
            });
        }
        
        // Collect changes since last backup
        let changes = self.collect_incremental_changes(last_index, current_index)?;
        
        // Serialize changes
        let serialized = bincode::serialize(&changes)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize incremental changes".to_string(),
                source: Box::new(e),
            })?;
        
        // Compress and encrypt
        let compressed = if self.config.compression_enabled {
            compress_data(&serialized)?
        } else {
            serialized
        };
        
        let encrypted = if self.config.encryption_enabled {
            // TODO: Implement encryption
            compressed
        } else {
            compressed
        };
        
        // Calculate checksum
        let checksum = calculate_checksum(&encrypted);
        
        // Generate backup ID
        let backup_id = format!("incremental-{}", Utc::now().format("%Y%m%d-%H%M%S"));
        
        // Create metadata
        let metadata = BackupMetadata {
            id: backup_id.clone(),
            timestamp: Utc::now(),
            version: 1,
            size_bytes: encrypted.len() as u64,
            backup_type: BackupType::Incremental {
                base_backup_id: base_backup.id.clone(),
                from_index: last_index,
                to_index: current_index,
            },
            cluster_info: ClusterInfo {
                node_count: 0, // TODO: Get actual cluster info
                vm_count: 0,
                leader_id: None,
                cluster_id: "default".to_string(),
            },
            checksum,
            raft_index: current_index,
            raft_term: current_state.hard_state.term,
        };
        
        // Store backup
        self.storage.store(&backup_id, &encrypted, &metadata).await?;
        
        // Update last backup index
        *self.last_backup_index.write().await = current_index;
        *self.last_backup.write().await = Some(SystemTime::now());
        
        Ok(backup_id)
    }
    
    fn collect_incremental_changes(&self, from_index: u64, to_index: u64) -> BlixardResult<IncrementalChanges> {
        // TODO: Implement actual change collection from Raft logs
        // This would involve reading entries from from_index+1 to to_index
        Ok(IncrementalChanges {
            from_index,
            to_index,
            entries: Vec::new(), // TODO: Collect actual entries
            vm_changes: Vec::new(), // TODO: Collect VM state changes
            worker_changes: Vec::new(), // TODO: Collect worker changes
        })
    }
    
    fn apply_incremental_changes(&self, changes: IncrementalChanges) -> BlixardResult<()> {
        // TODO: Apply incremental changes to the current state
        // This would involve replaying the Raft entries and state changes
        Ok(())
    }
    
    /// List available backups
    pub async fn list_backups(&self) -> BlixardResult<Vec<BackupMetadata>> {
        self.storage.list().await
    }
}

/// Perform a backup
async fn perform_backup(
    storage: &Arc<dyn BackupStorage>,
    raft_storage: &Arc<RedbRaftStorage>,
    config: &BackupConfig,
) -> BlixardResult<(String, BackupMetadata)> {
    // Generate backup ID
    let backup_id = format!("backup-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    
    // Create snapshot data
    let snapshot_data = raft_storage.create_snapshot_data()?;
    
    // Serialize snapshot data
    let serialized = bincode::serialize(&snapshot_data)
        .map_err(|e| BlixardError::Serialization {
            operation: "serialize snapshot data".to_string(),
            source: Box::new(e),
        })?;
    
    // Compress if enabled
    let compressed = if config.compression_enabled {
        compress_data(&serialized)?
    } else {
        serialized
    };
    
    // Encrypt if enabled
    let encrypted = if config.encryption_enabled {
        // TODO: Implement encryption
        compressed
    } else {
        compressed
    };
    
    // Calculate checksum
    let checksum = calculate_checksum(&encrypted);
    
    // Get current Raft state for metadata
    let raft_state = raft_storage.initial_state()?
        .unwrap_or_else(|| raft::prelude::RaftState {
            hard_state: raft::prelude::HardState::default(),
            conf_state: raft::prelude::ConfState::default(),
        });
    
    // Create metadata
    let metadata = BackupMetadata {
        id: backup_id.clone(),
        timestamp: Utc::now(),
        version: 1,
        size_bytes: encrypted.len() as u64,
        backup_type: BackupType::Full,
        cluster_info: ClusterInfo {
            node_count: 0, // TODO: Get actual cluster info
            vm_count: snapshot_data.vm_states.len(),
            leader_id: None,
            cluster_id: "default".to_string(),
        },
        checksum,
        raft_index: raft_state.hard_state.commit,
        raft_term: raft_state.hard_state.term,
    };
    
    // Store backup
    storage.store(&backup_id, &encrypted, &metadata).await?;
    
    Ok((backup_id, metadata))
}

/// Clean up old backups
async fn cleanup_old_backups(
    storage: &Arc<dyn BackupStorage>,
    max_backups: usize,
) -> BlixardResult<()> {
    let backups = storage.list().await?;
    
    if backups.len() > max_backups {
        // Delete oldest backups
        for backup in backups.iter().skip(max_backups) {
            info!("Deleting old backup: {} ({})", backup.id, backup.timestamp);
            storage.delete(&backup.id).await?;
        }
    }
    
    Ok(())
}

/// Compress data using zstd
fn compress_data(data: &[u8]) -> BlixardResult<Vec<u8>> {
    use zstd::stream::encode_all;
    
    encode_all(data, 3) // Compression level 3
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to compress data: {}", e),
        })
}

/// Decompress data using zstd
fn decompress_data(data: &[u8]) -> BlixardResult<Vec<u8>> {
    use zstd::stream::decode_all;
    
    decode_all(data)
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to decompress data: {}", e),
        })
}

/// Calculate SHA-256 checksum
fn calculate_checksum(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_local_backup_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalBackupStorage::new(temp_dir.path().to_path_buf()).unwrap();
        
        let backup_id = "test-backup-001";
        let data = b"test backup data";
        let metadata = BackupMetadata {
            id: backup_id.to_string(),
            timestamp: Utc::now(),
            version: 1,
            size_bytes: data.len() as u64,
            backup_type: BackupType::Full,
            cluster_info: ClusterInfo {
                node_count: 3,
                vm_count: 5,
                leader_id: Some(1),
                cluster_id: "test-cluster".to_string(),
            },
            checksum: calculate_checksum(data),
        };
        
        // Store backup
        storage.store(backup_id, data, &metadata).await.unwrap();
        assert!(storage.exists(backup_id).await.unwrap());
        
        // Retrieve backup
        let (retrieved_data, retrieved_metadata) = storage.retrieve(backup_id).await.unwrap();
        assert_eq!(retrieved_data, data);
        assert_eq!(retrieved_metadata.id, metadata.id);
        assert_eq!(retrieved_metadata.checksum, metadata.checksum);
        
        // List backups
        let backups = storage.list().await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].id, backup_id);
        
        // Delete backup
        storage.delete(backup_id).await.unwrap();
        assert!(!storage.exists(backup_id).await.unwrap());
    }
    
    #[test]
    fn test_compression() {
        let original = b"This is test data that should be compressed. ".repeat(100);
        let compressed = compress_data(&original).unwrap();
        let decompressed = decompress_data(&compressed).unwrap();
        
        assert!(compressed.len() < original.len());
        assert_eq!(decompressed, original);
    }
    
    #[test]
    fn test_checksum() {
        let data1 = b"test data 1";
        let data2 = b"test data 2";
        
        let checksum1 = calculate_checksum(data1);
        let checksum2 = calculate_checksum(data2);
        let checksum1_again = calculate_checksum(data1);
        
        assert_eq!(checksum1, checksum1_again);
        assert_ne!(checksum1, checksum2);
    }
}