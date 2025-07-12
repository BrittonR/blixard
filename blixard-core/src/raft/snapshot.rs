//! Raft snapshot management and log compaction
//!
//! This module handles creating, distributing, and applying Raft snapshots
//! for state transfer and log compaction purposes.

use crate::common::error_context::StorageContext;
use crate::error::{BlixardError, BlixardResult};
use crate::raft_storage::{
    RedbRaftStorage, SnapshotData, CLUSTER_STATE_TABLE, IP_ALLOCATION_TABLE, IP_POOL_TABLE,
    TASK_ASSIGNMENT_TABLE, TASK_RESULT_TABLE, TASK_TABLE, VM_IP_MAPPING_TABLE, VM_STATE_TABLE,
    WORKER_STATUS_TABLE, WORKER_TABLE,
};

use raft::prelude::{RawNode, Ready, SnapshotStatus};
use raft::Storage;
use redb::{Database, ReadableTable, TableDefinition, WriteTransaction};
use slog::{info, warn, Logger};
use std::sync::{Arc, RwLock};
use tracing::{debug, instrument};

/// Snapshot manager for Raft log compaction and state transfer
pub struct RaftSnapshotManager {
    database: Arc<Database>,
    logger: Logger,

    // Track last log compaction index to prevent thrashing
    last_compaction_index: Arc<RwLock<Option<u64>>>,
}

impl RaftSnapshotManager {
    pub fn new(database: Arc<Database>, logger: Logger) -> Self {
        Self {
            database,
            logger,
            last_compaction_index: Arc::new(RwLock::new(None)),
        }
    }

    /// Process snapshot if present in ready state
    #[instrument(skip(self, ready), fields(has_snapshot = !ready.snapshot().is_empty()))]
    pub async fn process_snapshot_if_present(
        &self,
        ready: &Ready,
        storage: &RedbRaftStorage,
    ) -> BlixardResult<()> {
        if !ready.snapshot().is_empty() {
            info!(
                self.logger,
                "[RAFT-SNAPSHOT] Applying snapshot from ready state"
            );
            let snapshot = ready.snapshot();

            // Apply the snapshot to storage layer
            storage.apply_snapshot(snapshot)?;

            // Apply the snapshot to the state machine
            let snapshot_data = snapshot.get_data();
            if !snapshot_data.is_empty() {
                self.apply_snapshot_to_state_machine(snapshot_data)?;
            }

            let metadata = snapshot.get_metadata();
            info!(self.logger, "[RAFT-SNAPSHOT] Snapshot applied successfully";
                "index" => metadata.index,
                "term" => metadata.term,
                "voters" => ?metadata.get_conf_state().voters
            );
        }
        Ok(())
    }

    /// Apply snapshot data to the state machine
    pub fn apply_snapshot_to_state_machine(&self, snapshot_data: &[u8]) -> BlixardResult<()> {
        // Deserialize the snapshot data
        let snapshot: SnapshotData =
            bincode::deserialize(snapshot_data).map_err(|e| BlixardError::Serialization {
                operation: "deserialize snapshot".to_string(),
                source: Box::new(e),
            })?;

        debug!(
            "Applying snapshot with {} VM states, {} workers, {} tasks",
            snapshot.vm_states.len(),
            snapshot.workers.len(),
            snapshot.tasks.len()
        );

        // Apply the snapshot within a single transaction for atomicity
        let write_txn = self
            .database
            .begin_write()
            .storage_context("begin snapshot transaction")?;

        // Restore all string-keyed tables
        self.clear_and_restore_string_table(
            &write_txn,
            VM_STATE_TABLE,
            &snapshot.vm_states,
            "VM_STATE",
        )?;
        self.clear_and_restore_string_table(
            &write_txn,
            CLUSTER_STATE_TABLE,
            &snapshot.cluster_state,
            "CLUSTER_STATE",
        )?;
        self.clear_and_restore_string_table(&write_txn, TASK_TABLE, &snapshot.tasks, "TASK")?;
        self.clear_and_restore_string_table(
            &write_txn,
            TASK_ASSIGNMENT_TABLE,
            &snapshot.task_assignments,
            "TASK_ASSIGNMENT",
        )?;
        self.clear_and_restore_string_table(
            &write_txn,
            TASK_RESULT_TABLE,
            &snapshot.task_results,
            "TASK_RESULT",
        )?;

        // Restore binary-keyed tables
        self.clear_and_restore_binary_table(&write_txn, WORKER_TABLE, &snapshot.workers, "WORKER")?;
        self.clear_and_restore_binary_table(
            &write_txn,
            WORKER_STATUS_TABLE,
            &snapshot.worker_status,
            "WORKER_STATUS",
        )?;

        // Restore u64-keyed tables
        self.clear_and_restore_u64_table(&write_txn, IP_POOL_TABLE, &snapshot.ip_pools, "IP_POOL")?;

        // Restore remaining string-keyed tables
        self.clear_and_restore_string_table(
            &write_txn,
            IP_ALLOCATION_TABLE,
            &snapshot.ip_allocations,
            "IP_ALLOCATION",
        )?;
        self.clear_and_restore_string_table(
            &write_txn,
            VM_IP_MAPPING_TABLE,
            &snapshot.vm_ip_mappings,
            "VM_IP_MAPPING",
        )?;

        // Commit all changes atomically
        write_txn
            .commit()
            .storage_context("commit snapshot transaction")?;

        info!(self.logger, "Applied snapshot to state machine");
        Ok(())
    }

    /// Check if log compaction is needed and trigger it
    #[instrument(skip(self, raft_node, storage), fields(applied_index))]
    pub async fn check_and_trigger_log_compaction(
        &self,
        raft_node: &mut RawNode<RedbRaftStorage>,
        storage: &RedbRaftStorage,
    ) -> BlixardResult<()> {
        const COMPACTION_THRESHOLD: u64 = 1000;

        let applied_index = raft_node.raft.raft_log.applied;
        tracing::Span::current().record("applied_index", applied_index);

        if applied_index > COMPACTION_THRESHOLD {
            // Check if it's time to compact (don't compact too frequently)
            let last_compact = *self.last_compaction_index.read().unwrap();
            let should_compact = match last_compact {
                Some(last_compact_idx) => {
                    applied_index - last_compact_idx > COMPACTION_THRESHOLD / 2
                }
                None => true,
            };

            if should_compact {
                info!(self.logger, "[RAFT-COMPACT] Triggering log compaction";
                    "applied_index" => applied_index,
                    "last_compaction" => ?last_compact
                );

                self.perform_log_compaction(raft_node, storage, applied_index)
                    .await?;
            }
        }

        Ok(())
    }

    /// Perform the actual log compaction
    async fn perform_log_compaction(
        &self,
        raft_node: &mut RawNode<RedbRaftStorage>,
        storage: &RedbRaftStorage,
        applied_index: u64,
    ) -> BlixardResult<()> {
        // Create a snapshot
        match raft_node.store().snapshot(applied_index, 0) {
            Ok(snapshot) => {
                let compact_index = snapshot.get_metadata().index;

                // Compact the log
                if let Err(e) = storage.compact_log_before(compact_index) {
                    warn!(self.logger, "[RAFT-COMPACT] Failed to compact log"; "error" => %e);
                } else {
                    info!(
                        self.logger,
                        "[RAFT-COMPACT] Successfully compacted log before index {}", compact_index
                    );
                    *self.last_compaction_index.write().unwrap() = Some(compact_index);
                }
            }
            Err(e) => {
                warn!(self.logger, "[RAFT-COMPACT] Failed to create snapshot for compaction"; "error" => %e);
            }
        }

        Ok(())
    }

    /// Check and send snapshots to followers that need them
    #[instrument(skip(self, raft_node))]
    pub async fn check_and_send_snapshots(
        &self,
        raft_node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        let _progress = raft_node.raft.prs();

        let _followers_needing_snapshots: Vec<u64> = Vec::new();

        // TODO: Check the correct way to iterate over voters in the raft crate
        // For now, skip this functionality
        /*
        for (follower_id, progress) in voters.iter() {
            if *follower_id == raft_node.raft.id {
                continue; // Skip self
            }

            let needs_snapshot = match progress {
                Some(progress) => {
                    // Send snapshot if follower is new (matched = 0) or significantly behind
                    progress.matched == 0 ||
                    (raft_node.raft.raft_log.last_index() - progress.matched > 5)
                }
                None => true, // No progress info means new follower
            };

            if needs_snapshot {
                followers_needing_snapshots.push(*follower_id);
            }
        }
        */

        // if !followers_needing_snapshots.is_empty() {
        //     info!(self.logger, "[RAFT-SNAPSHOT] Sending snapshots to {} followers",
        //           followers_needing_snapshots.len());
        //
        //     for follower_id in followers_needing_snapshots {
        //         // Trigger snapshot sending
        //         raft_node.raft.send_append(follower_id);
        //         debug!("Triggered snapshot sending to follower {}", follower_id);
        //     }
        // }

        Ok(())
    }

    /// Report snapshot transmission status
    pub async fn report_snapshot(
        &self,
        raft_node: &mut RawNode<RedbRaftStorage>,
        to: u64,
        status: SnapshotStatus,
    ) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-SNAPSHOT] Reporting snapshot status";
            "to" => to,
            "status" => ?status
        );

        raft_node.report_snapshot(to, status);
        Ok(())
    }

    // Helper functions for table restoration

    /// Helper function to clear and restore a table with String keys
    fn clear_and_restore_string_table(
        &self,
        write_txn: &WriteTransaction,
        table_def: TableDefinition<&str, &[u8]>,
        snapshot_data: &[(String, Vec<u8>)],
        table_name: &str,
    ) -> BlixardResult<()> {
        let mut table = write_txn
            .open_table(table_def)
            .storage_context(&format!("open {} table", table_name))?;

        // Clear existing entries
        let keys_to_remove: Vec<String> = table
            .iter()
            .storage_context(&format!("iterate {} table", table_name))?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in keys_to_remove {
            table
                .remove(key.as_str())
                .storage_context(&format!("remove from {} table", table_name))?;
        }

        // Apply new entries
        for (key, value) in snapshot_data {
            table
                .insert(key.as_str(), value.as_slice())
                .storage_context(&format!("insert into {} table", table_name))?;
        }

        debug!(
            "Restored {} entries to {} table",
            snapshot_data.len(),
            table_name
        );
        Ok(())
    }

    /// Helper function to clear and restore a table with Vec<u8> keys  
    fn clear_and_restore_binary_table(
        &self,
        write_txn: &WriteTransaction,
        table_def: TableDefinition<&[u8], &[u8]>,
        snapshot_data: &[(Vec<u8>, Vec<u8>)],
        table_name: &str,
    ) -> BlixardResult<()> {
        let mut table = write_txn
            .open_table(table_def)
            .storage_context(&format!("open {} table", table_name))?;

        // Clear existing entries
        let keys_to_remove: Vec<Vec<u8>> = table
            .iter()
            .storage_context(&format!("iterate {} table", table_name))?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_vec()))
            .collect();
        for key in keys_to_remove {
            table
                .remove(key.as_slice())
                .storage_context(&format!("remove from {} table", table_name))?;
        }

        // Apply new entries
        for (key, value) in snapshot_data {
            table
                .insert(key.as_slice(), value.as_slice())
                .storage_context(&format!("insert into {} table", table_name))?;
        }

        debug!(
            "Restored {} entries to {} table",
            snapshot_data.len(),
            table_name
        );
        Ok(())
    }

    /// Helper function to clear and restore a table with u64 keys
    fn clear_and_restore_u64_table(
        &self,
        write_txn: &WriteTransaction,
        table_def: TableDefinition<u64, &[u8]>,
        snapshot_data: &[(u64, Vec<u8>)],
        table_name: &str,
    ) -> BlixardResult<()> {
        let mut table = write_txn
            .open_table(table_def)
            .storage_context(&format!("open {} table", table_name))?;

        // Clear existing entries
        let keys_to_remove: Vec<u64> = table
            .iter()
            .storage_context(&format!("iterate {} table", table_name))?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value()))
            .collect();
        for key in keys_to_remove {
            table
                .remove(&key)
                .storage_context(&format!("remove from {} table", table_name))?;
        }

        // Apply new entries
        for (key, value) in snapshot_data {
            table
                .insert(key, value.as_slice())
                .storage_context(&format!("insert into {} table", table_name))?;
        }

        debug!(
            "Restored {} entries to {} table",
            snapshot_data.len(),
            table_name
        );
        Ok(())
    }

    /// Get the last compaction index for monitoring
    pub async fn get_last_compaction_index(&self) -> Option<u64> {
        *self.last_compaction_index.read().unwrap()
    }
}
