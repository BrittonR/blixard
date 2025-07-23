use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, safe_metrics, Timer};
use crate::raft_codec;
use raft::{Config, GetEntriesContext, RawNode};
use redb::{Database, ReadableTable, TableDefinition};
use slog::o;
use std::sync::Arc;
// Temporarily disabled: tracing_otel uses tonic
// use crate::tracing_otel;
use serde::{Deserialize, Serialize};

#[cfg(feature = "failpoints")]
use crate::fail_point;

/// Snapshot data structure containing all state machine data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotData {
    /// VM states
    pub vm_states: Vec<(String, Vec<u8>)>,
    /// Cluster state
    pub cluster_state: Vec<(String, Vec<u8>)>,
    /// Tasks
    pub tasks: Vec<(String, Vec<u8>)>,
    /// Task assignments
    pub task_assignments: Vec<(String, Vec<u8>)>,
    /// Task results
    pub task_results: Vec<(String, Vec<u8>)>,
    /// Workers
    pub workers: Vec<(Vec<u8>, Vec<u8>)>,
    /// Worker status
    pub worker_status: Vec<(Vec<u8>, Vec<u8>)>,
    /// IP pools
    pub ip_pools: Vec<(u64, Vec<u8>)>,
    /// IP allocations
    pub ip_allocations: Vec<(String, Vec<u8>)>,
    /// VM IP mappings
    pub vm_ip_mappings: Vec<(String, Vec<u8>)>,
}

// Database table definitions
pub const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
pub const CLUSTER_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cluster_state");

// Raft storage tables
pub const RAFT_LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
pub const RAFT_HARD_STATE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("raft_hard_state");
pub const RAFT_CONF_STATE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("raft_conf_state");
pub const RAFT_SNAPSHOT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_snapshot");

// Task management tables
pub const TASK_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tasks");
pub const TASK_ASSIGNMENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("task_assignments");
pub const TASK_RESULT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_results");

// Worker management tables
pub const WORKER_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("workers");
pub const WORKER_STATUS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("worker_status");
pub const NODE_TOPOLOGY_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("node_topology");

// Quota management tables
pub const TENANT_QUOTA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tenant_quotas");
pub const TENANT_USAGE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tenant_usage");

// IP pool management tables
pub const IP_POOL_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("ip_pools");
pub const IP_ALLOCATION_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ip_allocations");
pub const VM_IP_MAPPING_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("vm_ip_mappings");

// Resource management tables
pub const RESOURCE_POLICY_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("resource_policies");

#[derive(Clone, Debug)]
pub struct RedbRaftStorage {
    pub database: Arc<Database>,
}

impl raft::Storage for RedbRaftStorage {
    fn initial_state(&self) -> raft::Result<raft::RaftState> {
        // Temporarily disabled: tracing_otel uses tonic
        // let span = tracing_otel::storage_span("initial_state", "raft_state");
        // let _enter = span.enter();

        #[cfg(feature = "observability")]
        let metrics = safe_metrics().ok();
        #[cfg(feature = "observability")]
        let _timer = metrics.as_ref().map(|m| Timer::with_attributes(
            m.storage_read_duration.clone(),
            vec![
                attributes::table("raft_state"),
                attributes::operation("initial_state"),
            ],
        ));

        let read_txn = self
            .database
            .begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        // Load hard state
        let hard_state = if let Ok(table) = read_txn.open_table(RAFT_HARD_STATE_TABLE) {
            #[cfg(feature = "observability")]
            if let Some(m) = &metrics {
                m.storage_reads
                    .add(1, &[attributes::table("raft_hard_state")]);
            }
            match table.get("hard_state") {
                Ok(Some(data)) => {
                    match raft_codec::deserialize_hard_state(data.value()) {
                        Ok(state) => state,
                        Err(e) => {
                            tracing::error!(
                                "Corrupted hard state detected in Raft storage: {}. Using default state. \
                                This may indicate data corruption.",
                                e
                            );
                            raft::prelude::HardState::default()
                        }
                    }
                }
                Ok(None) => {
                    tracing::debug!("No hard state found in Raft storage, using default");
                    raft::prelude::HardState::default()
                }
                Err(e) => {
                    tracing::warn!("Failed to read hard state from Raft storage: {}. Using default.", e);
                    raft::prelude::HardState::default()
                }
            }
        } else {
            tracing::warn!("Failed to open hard state table in Raft storage, using default");
            raft::prelude::HardState::default()
        };

        // Load conf state
        let conf_state = if let Ok(table) = read_txn.open_table(RAFT_CONF_STATE_TABLE) {
            if let Ok(Some(data)) = table.get("conf_state") {
                match raft_codec::deserialize_conf_state(data.value()) {
                    Ok(cs) => {
                        tracing::info!(
                            "[STORAGE] Loaded conf state from storage: voters={:?}",
                            cs.voters
                        );
                        cs
                    }
                    Err(e) => {
                        tracing::warn!("[STORAGE] Failed to deserialize conf state: {}", e);
                        raft::prelude::ConfState::default()
                    }
                }
            } else {
                tracing::info!("[STORAGE] No conf state found in storage, using default");
                raft::prelude::ConfState::default()
            }
        } else {
            tracing::info!("[STORAGE] RAFT_CONF_STATE_TABLE not found, using default conf state");
            raft::prelude::ConfState::default()
        };

        tracing::info!("[STORAGE] Returning initial state - hard_state: term={}, vote={}, commit={}, conf_state: voters={:?}", 
            hard_state.term, hard_state.vote, hard_state.commit, conf_state.voters);
        Ok(raft::RaftState::new(hard_state, conf_state))
    }

    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
        _context: GetEntriesContext,
    ) -> raft::Result<Vec<raft::prelude::Entry>> {
        // Temporarily disabled: tracing_otel uses tonic
        // let span = tracing_otel::storage_span("entries", "raft_log");
        // let _enter = span.enter();
        //
        // tracing_otel::add_attributes(&[
        //     ("range.low", &low),
        //     ("range.high", &high),
        // ]);

        let max_size = max_size.into();
        let read_txn = self
            .database
            .begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        let table = read_txn
            .open_table(RAFT_LOG_TABLE)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        // Pre-allocate with estimated capacity to avoid reallocations
        let estimated_entries = std::cmp::min(high - low, 100) as usize;
        let mut entries = Vec::with_capacity(estimated_entries);
        let mut size = 0u64;

        for idx in low..high {
            if let Ok(Some(data)) = table.get(&idx) {
                let entry_data = data.value();
                let entry_size = entry_data.len() as u64;

                // Check size limit before expensive deserialization
                if let Some(max) = max_size {
                    if size + entry_size > max {
                        break;
                    }
                }

                let entry = raft_codec::deserialize_entry(entry_data)
                    .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

                size += entry_size;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        let read_txn = self
            .database
            .begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        let table = read_txn
            .open_table(RAFT_LOG_TABLE)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        if let Ok(Some(data)) = table.get(&idx) {
            let entry = raft_codec::deserialize_entry(data.value())
                .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
            Ok(entry.term)
        } else {
            Ok(0)
        }
    }

    fn first_index(&self) -> raft::Result<u64> {
        let read_txn = self
            .database
            .begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        let table = read_txn
            .open_table(RAFT_LOG_TABLE)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        match table.iter() {
            Ok(mut iter) => {
                if let Some(Ok((key, _))) = iter.next() {
                    Ok(key.value())
                } else {
                    Ok(1)
                }
            }
            Err(_) => Ok(1),
        }
    }

    fn last_index(&self) -> raft::Result<u64> {
        let read_txn = self
            .database
            .begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        let table = read_txn
            .open_table(RAFT_LOG_TABLE)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        match table.iter() {
            Ok(iter) => {
                if let Some(Ok((key, _))) = iter.rev().next() {
                    Ok(key.value())
                } else {
                    Ok(0)
                }
            }
            Err(_) => Ok(0),
        }
    }

    fn snapshot(&self, request_index: u64, _to: u64) -> raft::Result<raft::prelude::Snapshot> {
        // Create a snapshot of the current state
        let mut snapshot = raft::prelude::Snapshot::default();

        // Get the current configuration state
        let conf_state = self
            .load_conf_state()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        // Get the last index from the log
        let last_index = self.last_index()?;
        let term = if last_index > 0 {
            self.term(last_index)?
        } else {
            0
        };

        // Build snapshot metadata
        let mut metadata = raft::prelude::SnapshotMetadata::default();
        metadata.set_conf_state(conf_state);
        metadata.index = std::cmp::min(request_index, last_index);
        metadata.term = term;

        let snapshot_index = metadata.index;
        let snapshot_term = metadata.term;

        snapshot.set_metadata(metadata);

        // Create snapshot data with all state machine tables
        let snapshot_data = self
            .create_snapshot_data()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        // Serialize the snapshot data
        snapshot.data = bincode::serialize(&snapshot_data)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;

        tracing::info!(
            "Created snapshot at index {} term {} with {} bytes of data",
            snapshot_index,
            snapshot_term,
            snapshot.data.len()
        );

        Ok(snapshot)
    }
}

impl RedbRaftStorage {
    /// Append entries to the log
    pub fn append(&self, entries: &[raft::prelude::Entry]) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("storage::append_entries");

        #[cfg(feature = "observability")]
        let metrics = safe_metrics().ok();
        #[cfg(feature = "observability")]
        let _timer = metrics.as_ref().map(|m| Timer::with_attributes(
            m.storage_write_duration.clone(),
            vec![
                attributes::table("raft_log"),
                attributes::operation("append"),
            ],
        ));

        let write_txn = self.database.begin_write()?;

        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;

            for entry in entries {
                let data = raft_codec::serialize_entry(entry)?;
                table.insert(&entry.index, data.as_slice())?;
                #[cfg(feature = "observability")]
                if let Some(m) = &metrics {
                    m.storage_writes
                        .add(1, &[attributes::table("raft_log")]);
                }
            }
        }

        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");

        write_txn.commit()?;
        Ok(())
    }

    /// Save hard state
    pub fn save_hard_state(&self, hard_state: &raft::prelude::HardState) -> BlixardResult<()> {
        #[cfg(feature = "observability")]
        let metrics = safe_metrics().ok();
        #[cfg(feature = "observability")]
        let _timer = metrics.as_ref().map(|m| Timer::with_attributes(
            m.storage_write_duration.clone(),
            vec![
                attributes::table("raft_hard_state"),
                attributes::operation("save"),
            ],
        ));

        let write_txn = self.database.begin_write()?;

        {
            let mut table = write_txn.open_table(RAFT_HARD_STATE_TABLE)?;
            let data = raft_codec::serialize_hard_state(hard_state)?;
            table.insert("hard_state", data.as_slice())?;
            #[cfg(feature = "observability")]
            if let Some(m) = &metrics {
                m.storage_writes
                    .add(1, &[attributes::table("raft_hard_state")]);
            }
        }

        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");

        write_txn.commit()?;
        Ok(())
    }

    /// Save configuration state
    pub fn save_conf_state(&self, conf_state: &raft::prelude::ConfState) -> BlixardResult<()> {
        tracing::info!(
            "[STORAGE] Saving conf state to storage: voters={:?}",
            conf_state.voters
        );
        let write_txn = self.database.begin_write()?;

        {
            let mut table = write_txn.open_table(RAFT_CONF_STATE_TABLE)?;
            let data = raft_codec::serialize_conf_state(conf_state)?;
            table.insert("conf_state", data.as_slice())?;
        }

        write_txn.commit()?;
        tracing::info!("[STORAGE] Successfully saved conf state");
        Ok(())
    }

    /// Initialize storage for a single-node cluster
    pub fn initialize_single_node(&self, node_id: u64) -> BlixardResult<()> {
        // Check if already initialized
        let read_txn = self.database.begin_read()?;
        if let Ok(table) = read_txn.open_table(RAFT_CONF_STATE_TABLE) {
            if let Ok(Some(_)) = table.get("conf_state") {
                // Already initialized
                return Ok(());
            }
        }
        drop(read_txn);

        // Create initial ConfState with this node as the sole voter
        let mut conf_state = raft::prelude::ConfState::default();
        conf_state.voters = vec![node_id];

        // Save the initial configuration
        self.save_conf_state(&conf_state)?;

        Ok(())
    }

    /// Initialize storage for a node joining an existing cluster
    pub fn initialize_joining_node(&self) -> BlixardResult<()> {
        // Check if already initialized
        let read_txn = self.database.begin_read()?;
        if let Ok(table) = read_txn.open_table(RAFT_CONF_STATE_TABLE) {
            if let Ok(Some(_)) = table.get("conf_state") {
                // Already initialized
                return Ok(());
            }
        }
        drop(read_txn);

        // Create empty ConfState for a joining node
        let conf_state = raft::prelude::ConfState::default();
        // No voters - this node isn't a voter yet

        // Create initial hard state with term 0, no vote
        let hard_state = raft::prelude::HardState {
            term: 0,
            vote: 0,
            commit: 0,
        };

        // Save both states
        self.save_conf_state(&conf_state)?;
        self.save_hard_state(&hard_state)?;

        Ok(())
    }

    /// Load configuration state from storage
    pub fn load_conf_state(&self) -> BlixardResult<raft::prelude::ConfState> {
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(RAFT_CONF_STATE_TABLE) {
            if let Ok(Some(data)) = table.get("conf_state") {
                return crate::raft_codec::deserialize_conf_state(data.value()).map_err(|e| {
                    BlixardError::Serialization {
                        operation: "deserialize conf state".to_string(),
                        source: Box::new(e),
                    }
                });
            }
        }

        // Return default if not found
        Ok(raft::prelude::ConfState::default())
    }

    /// Create a snapshot of all state machine data
    pub fn create_snapshot_data(&self) -> BlixardResult<SnapshotData> {
        #[cfg(feature = "failpoints")]
        fail_point!("storage::create_snapshot");

        let read_txn = self.database.begin_read()?;

        let mut snapshot_data = SnapshotData {
            vm_states: Vec::new(),
            cluster_state: Vec::new(),
            tasks: Vec::new(),
            task_assignments: Vec::new(),
            task_results: Vec::new(),
            workers: Vec::new(),
            worker_status: Vec::new(),
            ip_pools: Vec::new(),
            ip_allocations: Vec::new(),
            vm_ip_mappings: Vec::new(),
        };

        // Read VM states
        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .vm_states
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read cluster state
        if let Ok(table) = read_txn.open_table(CLUSTER_STATE_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .cluster_state
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read tasks
        if let Ok(table) = read_txn.open_table(TASK_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .tasks
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read task assignments
        if let Ok(table) = read_txn.open_table(TASK_ASSIGNMENT_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .task_assignments
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read task results
        if let Ok(table) = read_txn.open_table(TASK_RESULT_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .task_results
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read workers
        if let Ok(table) = read_txn.open_table(WORKER_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .workers
                    .push((key.value().to_vec(), value.value().to_vec()));
            }
        }

        // Read worker status
        if let Ok(table) = read_txn.open_table(WORKER_STATUS_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .worker_status
                    .push((key.value().to_vec(), value.value().to_vec()));
            }
        }

        // Read IP pools
        if let Ok(table) = read_txn.open_table(IP_POOL_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .ip_pools
                    .push((key.value(), value.value().to_vec()));
            }
        }

        // Read IP allocations
        if let Ok(table) = read_txn.open_table(IP_ALLOCATION_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .ip_allocations
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        // Read VM IP mappings
        if let Ok(table) = read_txn.open_table(VM_IP_MAPPING_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                snapshot_data
                    .vm_ip_mappings
                    .push((key.value().to_string(), value.value().to_vec()));
            }
        }

        Ok(snapshot_data)
    }

    /// Helper function to clear and restore a table with String keys
    fn restore_string_table<T>(
        write_txn: &redb::WriteTransaction,
        table_def: TableDefinition<&str, &[u8]>,
        data: T,
    ) -> BlixardResult<()>
    where
        T: IntoIterator<Item = (String, Vec<u8>)>,
    {
        let mut table = write_txn.open_table(table_def)?;
        
        // Clear all existing entries
        let keys_to_remove: Vec<String> = table
            .iter()?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in keys_to_remove {
            table.remove(key.as_str())?;
        }
        
        // Insert new entries
        for (key, value) in data {
            table.insert(key.as_str(), value.as_slice())?;
        }
        
        Ok(())
    }

    /// Helper function to clear and restore a table with Vec<u8> keys
    fn restore_bytes_table<T>(
        write_txn: &redb::WriteTransaction,
        table_def: TableDefinition<&[u8], &[u8]>,
        data: T,
    ) -> BlixardResult<()>
    where
        T: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        let mut table = write_txn.open_table(table_def)?;
        
        // Clear all existing entries
        let keys_to_remove: Vec<Vec<u8>> = table
            .iter()?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_vec()))
            .collect();
        for key in keys_to_remove {
            table.remove(key.as_slice())?;
        }
        
        // Insert new entries
        for (key, value) in data {
            table.insert(key.as_slice(), value.as_slice())?;
        }
        
        Ok(())
    }

    /// Helper function to clear and restore a table with u64 keys
    fn restore_u64_table<T>(
        write_txn: &redb::WriteTransaction,
        table_def: TableDefinition<u64, &[u8]>,
        data: T,
    ) -> BlixardResult<()>
    where
        T: IntoIterator<Item = (u64, Vec<u8>)>,
    {
        let mut table = write_txn.open_table(table_def)?;
        
        // Clear all existing entries
        let keys_to_remove: Vec<u64> = table
            .iter()?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value()))
            .collect();
        for key in keys_to_remove {
            table.remove(&key)?;
        }
        
        // Insert new entries
        for (key, value) in data {
            table.insert(&key, value.as_slice())?;
        }
        
        Ok(())
    }

    /// Restore state machine from snapshot data
    pub fn restore_from_snapshot(&self, data: &[u8]) -> BlixardResult<()> {
        let snapshot_data: SnapshotData =
            bincode::deserialize(data).map_err(|e| BlixardError::Serialization {
                operation: "deserialize snapshot data".to_string(),
                source: Box::new(e),
            })?;

        let write_txn = self.database.begin_write()?;

        // Restore all tables using helper functions
        Self::restore_string_table(&write_txn, VM_STATE_TABLE, snapshot_data.vm_states)?;
        Self::restore_string_table(&write_txn, CLUSTER_STATE_TABLE, snapshot_data.cluster_state)?;
        Self::restore_string_table(&write_txn, TASK_TABLE, snapshot_data.tasks)?;
        Self::restore_string_table(&write_txn, TASK_ASSIGNMENT_TABLE, snapshot_data.task_assignments)?;
        Self::restore_string_table(&write_txn, TASK_RESULT_TABLE, snapshot_data.task_results)?;
        Self::restore_bytes_table(&write_txn, WORKER_TABLE, snapshot_data.workers)?;
        Self::restore_bytes_table(&write_txn, WORKER_STATUS_TABLE, snapshot_data.worker_status)?;
        Self::restore_u64_table(&write_txn, IP_POOL_TABLE, snapshot_data.ip_pools)?;
        Self::restore_string_table(&write_txn, IP_ALLOCATION_TABLE, snapshot_data.ip_allocations)?;
        Self::restore_string_table(&write_txn, VM_IP_MAPPING_TABLE, snapshot_data.vm_ip_mappings)?;

        write_txn.commit()?;

        tracing::info!("Restored state machine from snapshot");
        Ok(())
    }

    /// Apply a snapshot to the storage
    pub fn apply_snapshot(&self, snapshot: &raft::prelude::Snapshot) -> BlixardResult<()> {
        let metadata = snapshot.get_metadata();

        // Save the configuration state from the snapshot
        self.save_conf_state(metadata.get_conf_state())?;

        // Update hard state with snapshot's term and commit index
        let mut hard_state = raft::prelude::HardState::default();
        hard_state.term = metadata.term;
        hard_state.commit = metadata.index;
        self.save_hard_state(&hard_state)?;

        // Restore state machine from snapshot data
        if !snapshot.data.is_empty() {
            self.restore_from_snapshot(&snapshot.data)?;
        }

        // Clear old log entries before the snapshot index
        self.compact_log_before(metadata.index)?;

        tracing::info!(
            "Applied snapshot at index {} term {} with voters {:?}",
            metadata.index,
            metadata.term,
            metadata.get_conf_state().voters
        );

        Ok(())
    }

    /// Compact log entries before the given index
    pub fn compact_log_before(&self, index: u64) -> BlixardResult<()> {
        if index == 0 {
            return Ok(());
        }

        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            // Remove all entries with index < compact_index
            let keys_to_remove: Vec<u64> = table
                .iter()?
                .filter_map(|entry| {
                    entry.ok().and_then(|(k, _)| {
                        let key = k.value();
                        if key < index {
                            Some(key)
                        } else {
                            None
                        }
                    })
                })
                .collect();

            for key in keys_to_remove {
                table.remove(&key)?;
            }
        }
        write_txn.commit()?;

        tracing::info!("Compacted log entries before index {}", index);
        Ok(())
    }

    /// Save tenant quota to storage
    pub fn save_tenant_quota(
        &self,
        quota: &crate::resource_quotas::TenantQuota,
    ) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(TENANT_QUOTA_TABLE)?;
            let serialized =
                bincode::serialize(quota).map_err(|e| BlixardError::Serialization {
                    operation: "serialize tenant quota".to_string(),
                    source: Box::new(e),
                })?;
            table.insert(quota.tenant_id.as_str(), serialized.as_slice())?;
        }
        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");

        write_txn.commit()?;
        Ok(())
    }

    /// Get tenant quota from storage
    pub fn get_tenant_quota(
        &self,
        tenant_id: &str,
    ) -> BlixardResult<Option<crate::resource_quotas::TenantQuota>> {
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(TENANT_QUOTA_TABLE) {
            if let Ok(Some(data)) = table.get(tenant_id) {
                let quota = bincode::deserialize(data.value()).map_err(|e| {
                    BlixardError::Serialization {
                        operation: "deserialize tenant quota".to_string(),
                        source: Box::new(e),
                    }
                })?;
                return Ok(Some(quota));
            }
        }

        Ok(None)
    }

    /// Get all tenant quotas from storage
    pub fn get_all_quotas(&self) -> BlixardResult<Vec<crate::resource_quotas::TenantQuota>> {
        let read_txn = self.database.begin_read()?;
        let mut quotas = Vec::new();

        if let Ok(table) = read_txn.open_table(TENANT_QUOTA_TABLE) {
            for entry in table.iter()? {
                if let Ok((_, data)) = entry {
                    match bincode::deserialize(data.value()) {
                        Ok(quota) => quotas.push(quota),
                        Err(e) => {
                            tracing::warn!("Failed to deserialize quota: {}", e);
                        }
                    }
                }
            }
        }

        Ok(quotas)
    }

    /// Delete tenant quota from storage
    pub fn delete_tenant_quota(&self, tenant_id: &str) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(TENANT_QUOTA_TABLE)?;
            table.remove(tenant_id)?;
        }
        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");

        write_txn.commit()?;
        Ok(())
    }

    /// Save node topology information
    pub fn save_node_topology(
        &self,
        node_id: u64,
        topology: &crate::types::NodeTopology,
    ) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(NODE_TOPOLOGY_TABLE)?;
            let serialized =
                bincode::serialize(topology).map_err(|e| BlixardError::Serialization {
                    operation: "serialize node topology".to_string(),
                    source: Box::new(e),
                })?;
            table.insert(node_id.to_le_bytes().as_slice(), serialized.as_slice())?;
        }
        #[cfg(feature = "failpoints")]
        fail_point!("storage::commit_transaction");

        write_txn.commit()?;
        Ok(())
    }

    /// Get node topology information
    pub fn get_node_topology(
        &self,
        node_id: u64,
    ) -> BlixardResult<Option<crate::types::NodeTopology>> {
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(NODE_TOPOLOGY_TABLE) {
            if let Ok(Some(data)) = table.get(node_id.to_le_bytes().as_slice()) {
                let topology = bincode::deserialize(data.value()).map_err(|e| {
                    BlixardError::Serialization {
                        operation: "deserialize node topology".to_string(),
                        source: Box::new(e),
                    }
                })?;
                return Ok(Some(topology));
            }
        }

        Ok(None)
    }
}

/// Initialize all required database tables
pub fn init_database_tables(database: &Arc<Database>) -> BlixardResult<()> {
    let write_txn = database.begin_write()?;

    // Create all necessary tables
    let _ = write_txn.open_table(VM_STATE_TABLE)?;
    let _ = write_txn.open_table(CLUSTER_STATE_TABLE)?;
    let _ = write_txn.open_table(RAFT_LOG_TABLE)?;
    let _ = write_txn.open_table(RAFT_HARD_STATE_TABLE)?;
    let _ = write_txn.open_table(RAFT_CONF_STATE_TABLE)?;
    let _ = write_txn.open_table(TASK_TABLE)?;
    let _ = write_txn.open_table(TASK_ASSIGNMENT_TABLE)?;
    let _ = write_txn.open_table(TASK_RESULT_TABLE)?;
    let _ = write_txn.open_table(WORKER_TABLE)?;
    let _ = write_txn.open_table(WORKER_STATUS_TABLE)?;
    let _ = write_txn.open_table(NODE_TOPOLOGY_TABLE)?;
    let _ = write_txn.open_table(TENANT_QUOTA_TABLE)?;
    let _ = write_txn.open_table(TENANT_USAGE_TABLE)?;

    write_txn.commit()?;
    Ok(())
}

/// Initialize Raft node with the given configuration and storage
pub fn init_raft(node_id: u64, database: Arc<Database>) -> BlixardResult<RawNode<RedbRaftStorage>> {
    let raft_config = Config {
        id: node_id,
        election_tick: 10,
        heartbeat_tick: 3,
        ..Default::default()
    };

    let storage = RedbRaftStorage { database };

    // Create a simple logger for Raft
    let drain = slog::Discard;
    let logger = slog::Logger::root(drain, o!());

    RawNode::new(&raft_config, storage, &logger).map_err(|e| BlixardError::Raft {
        operation: "initialize raft node".to_string(),
        source: Box::new(e),
    })
}

/// Storage trait for quota operations
#[async_trait::async_trait]
pub trait Storage: Send + Sync + std::fmt::Debug {
    /// Save tenant quota to storage
    async fn save_tenant_quota(
        &self,
        quota: &crate::resource_quotas::TenantQuota,
    ) -> BlixardResult<()>;

    /// Get tenant quota from storage
    async fn get_tenant_quota(
        &self,
        tenant_id: &str,
    ) -> BlixardResult<Option<crate::resource_quotas::TenantQuota>>;

    /// Get all tenant quotas from storage
    async fn get_all_quotas(&self) -> BlixardResult<Vec<crate::resource_quotas::TenantQuota>>;

    /// Delete tenant quota from storage
    async fn delete_tenant_quota(&self, tenant_id: &str) -> BlixardResult<()>;
}

/// Implement Storage trait for RedbRaftStorage
#[async_trait::async_trait]
impl Storage for RedbRaftStorage {
    async fn save_tenant_quota(
        &self,
        quota: &crate::resource_quotas::TenantQuota,
    ) -> BlixardResult<()> {
        self.save_tenant_quota(quota)
    }

    async fn get_tenant_quota(
        &self,
        tenant_id: &str,
    ) -> BlixardResult<Option<crate::resource_quotas::TenantQuota>> {
        self.get_tenant_quota(tenant_id)
    }

    async fn get_all_quotas(&self) -> BlixardResult<Vec<crate::resource_quotas::TenantQuota>> {
        self.get_all_quotas()
    }

    async fn delete_tenant_quota(&self, tenant_id: &str) -> BlixardResult<()> {
        self.delete_tenant_quota(tenant_id)
    }
}
