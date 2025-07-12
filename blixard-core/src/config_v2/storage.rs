//! Storage and backup configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Database file name
    pub db_name: String,

    /// Cache size in MB
    pub cache_size_mb: u64,

    /// Enable compression
    pub compression: bool,

    /// Sync mode (none, normal, full)
    pub sync_mode: String,

    /// Backup configuration
    pub backup: BackupConfig,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    /// Enable automatic backups
    pub enabled: bool,

    /// Backup directory
    pub backup_dir: PathBuf,

    /// Backup interval
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Maximum backups to retain
    pub max_backups: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_name: "blixard.db".to_string(),
            cache_size_mb: 512,
            compression: true,
            sync_mode: "normal".to_string(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backup_dir: PathBuf::from("./backups"),
            interval: Duration::from_secs(3600),
            max_backups: 7,
        }
    }
}