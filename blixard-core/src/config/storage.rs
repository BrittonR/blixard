//! Storage configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use crate::error::{BlixardError, BlixardResult};
use super::defaults::*;
use super::parse_duration_secs_from_env;

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Data directory path
    pub data_dir: PathBuf,
    
    /// Maximum database size
    pub max_db_size: usize,
    
    /// Database compaction interval
    #[serde(with = "humantime_serde")]
    pub compaction_interval: Duration,
    
    /// Cache size for database
    pub cache_size: usize,
    
    /// Write-ahead log buffer size
    pub wal_buffer_size: usize,
    
    /// Enable database compression
    pub enable_compression: bool,
    
    /// Enable sync on every write
    pub sync_on_write: bool,
    
    /// Snapshot storage configuration
    pub snapshots: SnapshotConfig,
    
    /// Backup configuration
    pub backup: BackupConfig,
}

/// Snapshot storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotConfig {
    /// Directory for snapshots
    pub dir: PathBuf,
    
    /// Maximum snapshots to retain
    pub max_snapshots: usize,
    
    /// Snapshot compression level (0-9)
    pub compression_level: u32,
    
    /// Enable incremental snapshots
    pub incremental: bool,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    /// Enable automatic backups
    pub enabled: bool,
    
    /// Backup directory
    pub dir: PathBuf,
    
    /// Backup interval
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
    
    /// Maximum backups to retain
    pub max_backups: usize,
    
    /// Enable backup compression
    pub compress: bool,
    
    /// S3 backup configuration
    pub s3: Option<S3BackupConfig>,
}

/// S3 backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3BackupConfig {
    /// S3 bucket name
    pub bucket: String,
    
    /// S3 region
    pub region: String,
    
    /// S3 key prefix
    pub prefix: String,
    
    /// Enable server-side encryption
    pub encryption: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            max_db_size: DEFAULT_MAX_DB_SIZE,
            compaction_interval: duration_secs(DEFAULT_COMPACTION_INTERVAL_SECS),
            cache_size: DEFAULT_CACHE_SIZE,
            wal_buffer_size: DEFAULT_WAL_BUFFER_SIZE,
            enable_compression: true,
            sync_on_write: true,
            snapshots: SnapshotConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("./data/snapshots"),
            max_snapshots: 10,
            compression_level: 6,
            incremental: false,
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: PathBuf::from("./data/backups"),
            interval: Duration::from_secs(86400), // 24 hours
            max_backups: 7,
            compress: true,
            s3: None,
        }
    }
}

impl StorageConfig {
    /// Load storage configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();
        
        if let Ok(dir) = std::env::var("BLIXARD_DATA_DIR") {
            config.data_dir = PathBuf::from(dir);
        }
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_DB_SIZE") {
            config.max_db_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_DB_SIZE".to_string())
            )?;
        }
        
        config.compaction_interval = parse_duration_secs_from_env(
            "BLIXARD_COMPACTION_INTERVAL_SECS",
            config.compaction_interval
        );
        
        if let Ok(val) = std::env::var("BLIXARD_CACHE_SIZE") {
            config.cache_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_CACHE_SIZE".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_WAL_BUFFER_SIZE") {
            config.wal_buffer_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_WAL_BUFFER_SIZE".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_ENABLE_COMPRESSION") {
            config.enable_compression = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_ENABLE_COMPRESSION".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_SYNC_ON_WRITE") {
            config.sync_on_write = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_SYNC_ON_WRITE".to_string())
            )?;
        }
        
        // Snapshot configuration
        if let Ok(dir) = std::env::var("BLIXARD_SNAPSHOT_DIR") {
            config.snapshots.dir = PathBuf::from(dir);
        }
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_SNAPSHOTS") {
            config.snapshots.max_snapshots = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_SNAPSHOTS".to_string())
            )?;
        }
        
        // Backup configuration
        if let Ok(val) = std::env::var("BLIXARD_BACKUP_ENABLED") {
            config.backup.enabled = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_BACKUP_ENABLED".to_string())
            )?;
        }
        
        if let Ok(dir) = std::env::var("BLIXARD_BACKUP_DIR") {
            config.backup.dir = PathBuf::from(dir);
        }
        
        // S3 backup configuration
        if let (Ok(bucket), Ok(region)) = (
            std::env::var("BLIXARD_S3_BACKUP_BUCKET"),
            std::env::var("BLIXARD_S3_BACKUP_REGION")
        ) {
            config.backup.s3 = Some(S3BackupConfig {
                bucket,
                region,
                prefix: std::env::var("BLIXARD_S3_BACKUP_PREFIX")
                    .unwrap_or_else(|_| "blixard-backups/".to_string()),
                encryption: std::env::var("BLIXARD_S3_BACKUP_ENCRYPTION")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true),
            });
        }
        
        Ok(config)
    }
    
    /// Validate storage configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate data directory
        if self.data_dir.as_os_str().is_empty() {
            return Err(BlixardError::ConfigError(
                "data_dir cannot be empty".to_string()
            ));
        }
        
        // Validate database size
        if self.max_db_size < 100 * 1024 * 1024 { // 100MB minimum
            return Err(BlixardError::ConfigError(
                "max_db_size too small (min 100MB)".to_string()
            ));
        }
        
        // Validate cache size
        if self.cache_size > self.max_db_size {
            return Err(BlixardError::ConfigError(
                "cache_size cannot exceed max_db_size".to_string()
            ));
        }
        
        // Validate WAL buffer
        if self.wal_buffer_size < 1024 * 1024 { // 1MB minimum
            return Err(BlixardError::ConfigError(
                "wal_buffer_size too small (min 1MB)".to_string()
            ));
        }
        
        // Validate snapshot config
        if self.snapshots.max_snapshots == 0 {
            return Err(BlixardError::ConfigError(
                "max_snapshots must be at least 1".to_string()
            ));
        }
        
        if self.snapshots.compression_level > 9 {
            return Err(BlixardError::ConfigError(
                "compression_level must be between 0 and 9".to_string()
            ));
        }
        
        // Validate backup config
        if self.backup.enabled {
            if self.backup.max_backups == 0 {
                return Err(BlixardError::ConfigError(
                    "max_backups must be at least 1 when backups are enabled".to_string()
                ));
            }
            
            if self.backup.interval < Duration::from_secs(3600) { // 1 hour minimum
                return Err(BlixardError::ConfigError(
                    "backup interval too small (min 1 hour)".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Get the full snapshot directory path
    pub fn snapshot_dir(&self) -> PathBuf {
        if self.snapshots.dir.is_absolute() {
            self.snapshots.dir.clone()
        } else {
            self.data_dir.join(&self.snapshots.dir)
        }
    }
    
    /// Get the full backup directory path
    pub fn backup_dir(&self) -> PathBuf {
        if self.backup.dir.is_absolute() {
            self.backup.dir.clone()
        } else {
            self.data_dir.join(&self.backup.dir)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_storage_config_is_valid() {
        let config = StorageConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_relative_paths() {
        let config = StorageConfig::default();
        let snapshot_dir = config.snapshot_dir();
        assert!(snapshot_dir.starts_with(&config.data_dir));
        
        let backup_dir = config.backup_dir();
        assert!(backup_dir.starts_with(&config.data_dir));
    }
}