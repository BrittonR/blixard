use std::sync::Arc;
use redb::{Database, TableDefinition, ReadableTable};
use raft::{Config, RawNode, GetEntriesContext};
use slog::o;
use crate::error::{BlixardError, BlixardResult};
use crate::raft_codec;

// Database table definitions
pub const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
pub const CLUSTER_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cluster_state");

// Raft storage tables
pub const RAFT_LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
pub const RAFT_HARD_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_hard_state");
pub const RAFT_CONF_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_conf_state");

// Task management tables
pub const TASK_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tasks");
pub const TASK_ASSIGNMENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_assignments");
pub const TASK_RESULT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_results");

// Worker management tables
pub const WORKER_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("workers");
pub const WORKER_STATUS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("worker_status");

#[derive(Clone)]
pub struct RedbRaftStorage {
    pub database: Arc<Database>,
}

impl raft::Storage for RedbRaftStorage {
    fn initial_state(&self) -> raft::Result<raft::RaftState> {
        let read_txn = self.database.begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        // Load hard state
        let hard_state = if let Ok(table) = read_txn.open_table(RAFT_HARD_STATE_TABLE) {
            if let Ok(Some(data)) = table.get("hard_state") {
                raft_codec::deserialize_hard_state(data.value())
                    .unwrap_or_else(|_| raft::prelude::HardState::default())
            } else {
                raft::prelude::HardState::default()
            }
        } else {
            raft::prelude::HardState::default()
        };
        
        // Load conf state
        let conf_state = if let Ok(table) = read_txn.open_table(RAFT_CONF_STATE_TABLE) {
            if let Ok(Some(data)) = table.get("conf_state") {
                raft_codec::deserialize_conf_state(data.value())
                    .unwrap_or_else(|_| raft::prelude::ConfState::default())
            } else {
                raft::prelude::ConfState::default()
            }
        } else {
            raft::prelude::ConfState::default()
        };
        
        Ok(raft::RaftState::new(hard_state, conf_state))
    }

    fn entries(&self, low: u64, high: u64, max_size: impl Into<Option<u64>>, _context: GetEntriesContext) -> raft::Result<Vec<raft::prelude::Entry>> {
        let max_size = max_size.into();
        let read_txn = self.database.begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        let table = read_txn.open_table(RAFT_LOG_TABLE)
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        let mut entries = Vec::new();
        let mut size = 0u64;
        
        for idx in low..high {
            if let Ok(Some(data)) = table.get(&idx) {
                let entry = raft_codec::deserialize_entry(data.value())
                    .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
                
                let entry_size = data.value().len() as u64;
                size += entry_size;
                entries.push(entry);
                
                if let Some(max) = max_size {
                    if size > max {
                        break;
                    }
                }
            }
        }
        
        Ok(entries)
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        let read_txn = self.database.begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        let table = read_txn.open_table(RAFT_LOG_TABLE)
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
        let read_txn = self.database.begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        let table = read_txn.open_table(RAFT_LOG_TABLE)
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
        let read_txn = self.database.begin_read()
            .map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?;
        
        let table = read_txn.open_table(RAFT_LOG_TABLE)
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

    fn snapshot(&self, _request_index: u64, _to: u64) -> raft::Result<raft::prelude::Snapshot> {
        // TODO: Implement snapshot creation from redb state
        Err(raft::Error::Store(raft::StorageError::SnapshotTemporarilyUnavailable))
    }
}

impl RedbRaftStorage {
    /// Append entries to the log
    pub fn append(&self, entries: &[raft::prelude::Entry]) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            
            for entry in entries {
                let data = raft_codec::serialize_entry(entry)?;
                table.insert(&entry.index, data.as_slice())?;
            }
        }
        
        write_txn.commit()?;
        Ok(())
    }
    
    /// Save hard state
    pub fn save_hard_state(&self, hard_state: &raft::prelude::HardState) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        
        {
            let mut table = write_txn.open_table(RAFT_HARD_STATE_TABLE)?;
            let data = raft_codec::serialize_hard_state(hard_state)?;
            table.insert("hard_state", data.as_slice())?;
        }
        
        write_txn.commit()?;
        Ok(())
    }
    
    /// Save configuration state
    pub fn save_conf_state(&self, conf_state: &raft::prelude::ConfState) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        
        {
            let mut table = write_txn.open_table(RAFT_CONF_STATE_TABLE)?;
            let data = raft_codec::serialize_conf_state(conf_state)?;
            table.insert("conf_state", data.as_slice())?;
        }
        
        write_txn.commit()?;
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
    
    write_txn.commit()?;
    Ok(())
}

/// Initialize Raft node with the given configuration and storage
pub fn init_raft(
    node_id: u64,
    database: Arc<Database>,
) -> BlixardResult<RawNode<RedbRaftStorage>> {
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
    
    RawNode::new(&raft_config, storage, &logger).map_err(|e| {
        BlixardError::Raft {
            operation: "initialize raft node".to_string(),
            source: Box::new(e),
        }
    })
}