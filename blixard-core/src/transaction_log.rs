//! Transaction log management for point-in-time recovery
//!
//! This module provides functionality to record and replay transactions
//! for precise point-in-time recovery between backups.

use crate::error::{BlixardError, BlixardResult};
use crate::raft_manager::WorkerCapabilities;
use crate::types::VmStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Transaction log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionEntry {
    /// Unique transaction ID
    pub id: String,
    /// Timestamp of transaction
    pub timestamp: DateTime<Utc>,
    /// Raft index when transaction was applied
    pub raft_index: u64,
    /// Type of transaction
    pub operation: TransactionOperation,
    /// Whether transaction was successfully applied
    pub success: bool,
    /// Error message if transaction failed
    pub error: Option<String>,
}

/// Type of transaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionOperation {
    /// VM creation
    CreateVm {
        vm_name: String,
        config: crate::types::VmConfig,
        node_id: u64,
    },
    /// VM update
    UpdateVm {
        vm_name: String,
        new_status: VmStatus,
    },
    /// VM deletion
    DeleteVm { vm_name: String },
    /// Worker registration
    RegisterWorker {
        node_id: u64,
        capabilities: WorkerCapabilities,
    },
    /// Worker update
    UpdateWorker {
        node_id: u64,
        capabilities: WorkerCapabilities,
    },
    /// Worker removal
    RemoveWorker { node_id: u64 },
    /// Configuration change
    ConfigChange {
        change_type: String,
        details: serde_json::Value,
    },
}

/// Transaction log writer
pub struct TransactionLogWriter {
    log_path: PathBuf,
    current_file: Arc<RwLock<Option<File>>>,
    current_index: Arc<RwLock<u64>>,
    rotation_size: u64,
}

impl TransactionLogWriter {
    /// Create a new transaction log writer
    pub async fn new(log_dir: &Path, rotation_size: u64) -> BlixardResult<Self> {
        // Create log directory if it doesn't exist
        tokio::fs::create_dir_all(log_dir)
            .await
            .map_err(|e| BlixardError::Storage {
                operation: "create transaction log directory".to_string(),
                source: Box::new(e),
            })?;

        // Find the latest log file
        let (log_path, current_index) = Self::find_latest_log(log_dir).await?;

        // Open or create the log file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
            .map_err(|e| BlixardError::Storage {
                operation: format!("open transaction log {:?}", log_path),
                source: Box::new(e),
            })?;

        Ok(Self {
            log_path,
            current_file: Arc::new(RwLock::new(Some(file))),
            current_index: Arc::new(RwLock::new(current_index)),
            rotation_size,
        })
    }

    /// Find the latest transaction log file
    async fn find_latest_log(log_dir: &Path) -> BlixardResult<(PathBuf, u64)> {
        let mut max_index = 0;
        let mut latest_log = None;

        let mut entries =
            tokio::fs::read_dir(log_dir)
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: "read transaction log directory".to_string(),
                    source: Box::new(e),
                })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| BlixardError::Storage {
                operation: "read directory entry".to_string(),
                source: Box::new(e),
            })?
        {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("txn-") && name.ends_with(".log") {
                    // Parse index from filename: txn-00001.log
                    if let Ok(index) = name[4..9].parse::<u64>() {
                        if index >= max_index {
                            max_index = index;
                            latest_log = Some(path);
                        }
                    }
                }
            }
        }

        let log_path =
            latest_log.unwrap_or_else(|| log_dir.join(format!("txn-{:05}.log", max_index + 1)));

        Ok((log_path, max_index))
    }

    /// Write a transaction entry
    pub async fn write(&self, entry: TransactionEntry) -> BlixardResult<()> {
        let json = serde_json::to_string(&entry).map_err(|e| BlixardError::Serialization {
            operation: "serialize transaction entry".to_string(),
            source: Box::new(e),
        })?;

        let mut file_guard = self.current_file.write().await;
        if let Some(file) = file_guard.as_mut() {
            // Write entry with newline
            file.write_all(json.as_bytes())
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: "write transaction entry".to_string(),
                    source: Box::new(e),
                })?;

            file.write_all(b"\n")
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: "write transaction newline".to_string(),
                    source: Box::new(e),
                })?;

            // Flush to ensure durability
            file.flush().await.map_err(|e| BlixardError::Storage {
                operation: "flush transaction log".to_string(),
                source: Box::new(e),
            })?;

            // Check if rotation is needed
            let metadata = file.metadata().await.map_err(|e| BlixardError::Storage {
                operation: "get transaction log metadata".to_string(),
                source: Box::new(e),
            })?;

            if metadata.len() >= self.rotation_size {
                drop(file_guard);
                self.rotate().await?;
            }
        }

        Ok(())
    }

    /// Rotate to a new log file
    async fn rotate(&self) -> BlixardResult<()> {
        let mut index = self.current_index.write().await;
        *index += 1;

        let parent = self
            .log_path
            .parent()
            .ok_or_else(|| BlixardError::Internal {
                message: "Transaction log path has no parent directory".to_string(),
            })?;
        let new_path = parent.join(format!("txn-{:05}.log", *index));

        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await
            .map_err(|e| BlixardError::Storage {
                operation: format!("create new transaction log {:?}", new_path),
                source: Box::new(e),
            })?;

        let mut file_guard = self.current_file.write().await;
        *file_guard = Some(new_file);

        info!("Rotated transaction log to {:?}", new_path);
        Ok(())
    }

    /// Close the transaction log
    pub async fn close(&self) -> BlixardResult<()> {
        let mut file_guard = self.current_file.write().await;
        if let Some(mut file) = file_guard.take() {
            file.flush().await.map_err(|e| BlixardError::Storage {
                operation: "flush transaction log on close".to_string(),
                source: Box::new(e),
            })?;
        }
        Ok(())
    }
}

/// Transaction log reader for recovery
pub struct TransactionLogReader {
    log_dir: PathBuf,
}

impl TransactionLogReader {
    /// Create a new transaction log reader
    pub fn new(log_dir: PathBuf) -> Self {
        Self { log_dir }
    }

    /// Read all transactions in a time range
    pub async fn read_range(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> BlixardResult<Vec<TransactionEntry>> {
        let mut transactions = Vec::new();

        // List all log files
        let mut log_files = self.list_log_files().await?;
        log_files.sort();

        // Read each log file
        for log_path in log_files {
            let file = File::open(&log_path)
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: format!("open transaction log {:?}", log_path),
                    source: Box::new(e),
                })?;

            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await.map_err(|e| BlixardError::Storage {
                operation: "read transaction log line".to_string(),
                source: Box::new(e),
            })? {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<TransactionEntry>(&line) {
                    Ok(entry) => {
                        if entry.timestamp >= start_time && entry.timestamp <= end_time {
                            transactions.push(entry);
                        } else if entry.timestamp > end_time {
                            // Logs are chronological, so we can stop here
                            return Ok(transactions);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse transaction log entry: {}", e);
                    }
                }
            }
        }

        Ok(transactions)
    }

    /// Read transactions after a specific Raft index
    pub async fn read_after_index(&self, after_index: u64) -> BlixardResult<Vec<TransactionEntry>> {
        let mut transactions = Vec::new();

        // List all log files
        let mut log_files = self.list_log_files().await?;
        log_files.sort();

        // Read each log file
        for log_path in log_files {
            let file = File::open(&log_path)
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: format!("open transaction log {:?}", log_path),
                    source: Box::new(e),
                })?;

            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await.map_err(|e| BlixardError::Storage {
                operation: "read transaction log line".to_string(),
                source: Box::new(e),
            })? {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<TransactionEntry>(&line) {
                    Ok(entry) => {
                        if entry.raft_index > after_index {
                            transactions.push(entry);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse transaction log entry: {}", e);
                    }
                }
            }
        }

        Ok(transactions)
    }

    /// List all transaction log files
    async fn list_log_files(&self) -> BlixardResult<Vec<PathBuf>> {
        let mut log_files = Vec::new();

        let mut entries =
            tokio::fs::read_dir(&self.log_dir)
                .await
                .map_err(|e| BlixardError::Storage {
                    operation: "read transaction log directory".to_string(),
                    source: Box::new(e),
                })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| BlixardError::Storage {
                operation: "read directory entry".to_string(),
                source: Box::new(e),
            })?
        {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("txn-") && name.ends_with(".log") {
                    log_files.push(path);
                }
            }
        }

        Ok(log_files)
    }
}

/// Apply a transaction to the system state
pub async fn apply_transaction(
    _transaction: &TransactionEntry,
    _storage: &Arc<crate::raft_storage::RedbRaftStorage>,
) -> BlixardResult<()> {
    // TODO: Implement transaction replay logic
    // This would apply the transaction operation to the storage
    match &_transaction.operation {
        TransactionOperation::CreateVm { .. } => {
            // Apply VM creation
        }
        TransactionOperation::UpdateVm { .. } => {
            // Apply VM update
        }
        TransactionOperation::DeleteVm { .. } => {
            // Apply VM deletion
        }
        TransactionOperation::RegisterWorker { .. } => {
            // Apply worker registration
        }
        TransactionOperation::UpdateWorker { .. } => {
            // Apply worker update
        }
        TransactionOperation::RemoveWorker { .. } => {
            // Apply worker removal
        }
        TransactionOperation::ConfigChange { .. } => {
            // Apply configuration change
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_transaction_log_write_read() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("txlogs");

        // Create writer
        let writer = TransactionLogWriter::new(&log_dir, 1024 * 1024)
            .await
            .unwrap();

        // Write some transactions
        let txn1 = TransactionEntry {
            id: "txn-1".to_string(),
            timestamp: Utc::now(),
            raft_index: 100,
            operation: TransactionOperation::CreateVm {
                vm_name: "test-vm".to_string(),
                config: Default::default(),
                node_id: 1,
            },
            success: true,
            error: None,
        };

        let txn2 = TransactionEntry {
            id: "txn-2".to_string(),
            timestamp: Utc::now(),
            raft_index: 101,
            operation: TransactionOperation::UpdateVm {
                vm_name: "test-vm".to_string(),
                new_status: VmStatus::Running,
            },
            success: true,
            error: None,
        };

        writer.write(txn1.clone()).await.unwrap();
        writer.write(txn2.clone()).await.unwrap();
        writer.close().await.unwrap();

        // Read transactions
        let reader = TransactionLogReader::new(log_dir);
        let transactions = reader.read_after_index(99).await.unwrap();

        assert_eq!(transactions.len(), 2);
        assert_eq!(transactions[0].id, "txn-1");
        assert_eq!(transactions[1].id, "txn-2");
    }

    #[tokio::test]
    async fn test_transaction_log_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("txlogs");

        // Create writer with small rotation size
        let writer = TransactionLogWriter::new(&log_dir, 100).await.unwrap();

        // Write enough to trigger rotation
        for i in 0..10 {
            let txn = TransactionEntry {
                id: format!("txn-{}", i),
                timestamp: Utc::now(),
                raft_index: 100 + i,
                operation: TransactionOperation::CreateVm {
                    vm_name: format!("vm-{}", i),
                    config: Default::default(),
                    node_id: 1,
                },
                success: true,
                error: None,
            };
            writer.write(txn).await.unwrap();
        }

        writer.close().await.unwrap();

        // Check that multiple log files were created
        let mut count = 0;
        let mut entries = tokio::fs::read_dir(&log_dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("log") {
                count += 1;
            }
        }

        assert!(
            count > 1,
            "Should have created multiple log files due to rotation"
        );
    }
}
