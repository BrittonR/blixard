use anyhow::Result;
use raft::prelude::*;
use raft::{Error as RaftError, GetEntriesContext, Storage as RaftStorageTrait, StorageError};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

#[cfg(feature = "failpoints")]
use fail::fail_point;

use crate::storage::Storage;

/// Persistent Raft storage implementation backed by redb
pub struct RaftStorage {
    storage: Arc<Storage>,

    // In-memory cache for performance
    hard_state: RwLock<HardState>,
    conf_state: RwLock<ConfState>,

    // Track the last compacted index for snapshots
    snapshot_metadata: RwLock<SnapshotMetadata>,
}

impl RaftStorage {
    pub async fn new(storage: Arc<Storage>) -> Result<Self> {
        // Load or initialize Raft state
        let hard_state = match storage.get_raft_state("hard_state")? {
            Some(data) => {
                if data.len() >= 24 {
                    let mut hs = HardState::default();
                    hs.set_term(u64::from_be_bytes(data[0..8].try_into().unwrap()));
                    hs.set_vote(u64::from_be_bytes(data[8..16].try_into().unwrap()));
                    hs.set_commit(u64::from_be_bytes(data[16..24].try_into().unwrap()));
                    hs
                } else {
                    HardState::default()
                }
            }
            None => HardState::default(),
        };

        let conf_state = match storage.get_raft_state("conf_state")? {
            Some(data) => {
                // Manual deserialization of ConfState
                // For simplicity, we'll store it as a JSON string
                if let Ok(json_str) = std::str::from_utf8(&data) {
                    let mut cs = ConfState::default();
                    if let Ok(voters) = serde_json::from_str::<Vec<u64>>(json_str) {
                        cs.set_voters(voters);
                    }
                    cs
                } else {
                    ConfState::default()
                }
            }
            None => ConfState::default(),
        };

        let snapshot_metadata = match storage.get_raft_state("snapshot_metadata")? {
            Some(data) => {
                // Manual deserialization
                if data.len() >= 16 {
                    let mut sm = SnapshotMetadata::default();
                    sm.set_index(u64::from_be_bytes(data[0..8].try_into().unwrap()));
                    sm.set_term(u64::from_be_bytes(data[8..16].try_into().unwrap()));
                    sm
                } else {
                    SnapshotMetadata::default()
                }
            }
            None => SnapshotMetadata::default(),
        };

        Ok(Self {
            storage,
            hard_state: RwLock::new(hard_state),
            conf_state: RwLock::new(conf_state),
            snapshot_metadata: RwLock::new(snapshot_metadata),
        })
    }

    /// Save hard state to persistent storage
    pub async fn save_hard_state(&self, hs: &HardState) -> Result<()> {
        // Manually serialize HardState fields
        let mut data = Vec::new();
        data.extend_from_slice(&hs.term.to_be_bytes());
        data.extend_from_slice(&hs.vote.to_be_bytes());
        data.extend_from_slice(&hs.commit.to_be_bytes());
        self.storage.save_raft_state("hard_state", &data)?;

        // Update in-memory cache
        *self.hard_state.write().unwrap() = hs.clone();

        debug!("Saved hard state: term={}, vote={}", hs.term, hs.vote);
        Ok(())
    }

    /// Save conf state to persistent storage
    pub async fn save_conf_state(&self, cs: &ConfState) -> Result<()> {
        // Store ConfState as JSON for simplicity
        let voters = cs.voters.to_vec();
        let json = serde_json::to_string(&voters)?;
        self.storage
            .save_raft_state("conf_state", json.as_bytes())?;

        // Update in-memory cache
        *self.conf_state.write().unwrap() = cs.clone();

        debug!("Saved conf state: voters={:?}", cs.voters);
        Ok(())
    }

    /// Append entries to the Raft log
    pub async fn append(&self, entries: &[Entry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "failpoints")]
        fail_point!("storage::before_append_entries", |_| Err(anyhow::anyhow!(
            "Injected append failure"
        )));

        for entry in entries {
            // Serialize the full entry with term and data
            let mut serialized = Vec::new();
            serialized.extend_from_slice(&entry.term.to_be_bytes());
            serialized.extend_from_slice(&entry.data);

            #[cfg(feature = "failpoints")]
            fail_point!("storage::append_single_entry");

            self.storage.append_raft_log(entry.index, &serialized)?;
        }

        debug!("Appended {} entries to Raft log", entries.len());
        Ok(())
    }

    /// Apply a snapshot
    pub async fn apply_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        let meta = snapshot.get_metadata();
        let data = snapshot.get_data();

        // Save snapshot data
        self.storage.save_raft_state("snapshot_data", data)?;

        // Update snapshot metadata
        let mut metadata_bytes = Vec::new();
        metadata_bytes.extend_from_slice(&meta.index.to_be_bytes());
        metadata_bytes.extend_from_slice(&meta.term.to_be_bytes());
        self.storage
            .save_raft_state("snapshot_metadata", &metadata_bytes)?;

        // Update in-memory cache
        *self.snapshot_metadata.write().unwrap() = meta.clone();

        // Update conf state from snapshot
        if meta.get_conf_state() != &ConfState::default() {
            self.save_conf_state(meta.get_conf_state()).await?;
        }

        // Compact log up to snapshot index
        self.storage.truncate_raft_log(meta.index)?;

        info!("Applied snapshot at index {}", meta.index);
        Ok(())
    }

    /// Create a snapshot of current state
    pub async fn create_snapshot(&self, index: u64, cs: ConfState, data: Vec<u8>) -> Result<()> {
        let mut metadata = SnapshotMetadata::default();
        metadata.set_index(index);
        metadata.set_term(RaftStorageTrait::term(self, index).unwrap_or(0));
        metadata.set_conf_state(cs);

        // Save snapshot
        self.storage.save_raft_state("snapshot_data", &data)?;

        let mut metadata_bytes = Vec::new();
        metadata_bytes.extend_from_slice(&metadata.index.to_be_bytes());
        metadata_bytes.extend_from_slice(&metadata.term.to_be_bytes());
        self.storage
            .save_raft_state("snapshot_metadata", &metadata_bytes)?;

        // Update in-memory cache
        *self.snapshot_metadata.write().unwrap() = metadata;

        // Compact log
        self.compact(index).await?;

        Ok(())
    }

    /// Compact the log up to the given index
    async fn compact(&self, compact_index: u64) -> Result<()> {
        if compact_index > 0 {
            // Remove all entries before compact_index
            self.storage.truncate_raft_log(compact_index - 1)?;
        }
        Ok(())
    }
}

impl RaftStorageTrait for RaftStorage {
    fn initial_state(&self) -> raft::Result<RaftState> {
        let hard_state = self.hard_state.read().unwrap().clone();
        let conf_state = self.conf_state.read().unwrap().clone();

        Ok(RaftState {
            hard_state,
            conf_state,
        })
    }

    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
        _ctx: GetEntriesContext,
    ) -> raft::Result<Vec<Entry>> {
        let max_size = max_size.into();

        // Check bounds
        let snapshot_index = self.snapshot_metadata.read().unwrap().index;
        if low <= snapshot_index {
            return Err(RaftError::Store(StorageError::Compacted));
        }

        // Fetch entries from storage
        let entries = self.storage.get_raft_logs(low, high - 1);

        match entries {
            Ok(logs) => {
                let mut entries = Vec::new();
                let mut size = 0u64;

                for (index, data) in logs {
                    // Extract term and actual data
                    let (term, entry_data) = if data.len() >= 8 {
                        let term = u64::from_be_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                        ]);
                        (term, data[8..].to_vec())
                    } else {
                        // Invalid entry, skip
                        warn!("Invalid entry at index {}: too short", index);
                        continue;
                    };

                    let entry = Entry {
                        context: vec![],
                        data: entry_data.into(),
                        index,
                        sync_log: false,
                        term,
                        entry_type: EntryType::EntryNormal.into(),
                    };

                    // Approximate size of the entry
                    size += 8 + 8 + entry.data.len() as u64; // index + term + data

                    if let Some(max) = max_size {
                        if size > max && !entries.is_empty() {
                            break;
                        }
                    }

                    entries.push(entry);
                }

                Ok(entries)
            }
            Err(e) => {
                warn!("Failed to fetch entries: {}", e);
                Err(RaftError::Store(StorageError::Unavailable))
            }
        }
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        // Check if index is compacted
        let snapshot_index = self.snapshot_metadata.read().unwrap().index;
        if idx == snapshot_index {
            return Ok(self.snapshot_metadata.read().unwrap().term);
        }

        // Fetch from storage
        let entry = self.storage.get_raft_log(idx);

        match entry {
            Ok(Some(data)) => {
                // Parse entry to get term
                // For now, store term at beginning of entry data
                if data.len() >= 8 {
                    let term = u64::from_be_bytes([
                        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                    ]);
                    Ok(term)
                } else {
                    Err(RaftError::Store(StorageError::Other(
                        "Invalid entry format".into(),
                    )))
                }
            }
            Ok(None) => Err(RaftError::Store(StorageError::Compacted)),
            Err(e) => {
                warn!("Failed to fetch term for index {}: {}", idx, e);
                Err(RaftError::Store(StorageError::Unavailable))
            }
        }
    }

    fn first_index(&self) -> raft::Result<u64> {
        let snapshot_index = self.snapshot_metadata.read().unwrap().index;
        Ok(snapshot_index + 1)
    }

    fn last_index(&self) -> raft::Result<u64> {
        // Get the highest index from storage
        let logs = self.storage.get_raft_logs(0, u64::MAX);

        match logs {
            Ok(entries) => {
                if let Some((index, _)) = entries.last() {
                    Ok(*index)
                } else {
                    // No entries, return snapshot index
                    Ok(self.snapshot_metadata.read().unwrap().index)
                }
            }
            Err(e) => {
                warn!("Failed to get last index: {}", e);
                Err(RaftError::Store(StorageError::Unavailable))
            }
        }
    }

    fn snapshot(&self, request_index: u64, _to: u64) -> raft::Result<Snapshot> {
        let metadata = self.snapshot_metadata.read().unwrap().clone();

        if metadata.index < request_index {
            return Err(RaftError::Store(
                StorageError::SnapshotTemporarilyUnavailable,
            ));
        }

        // Load snapshot data
        let data = self.storage.get_raft_state("snapshot_data");

        match data {
            Ok(Some(data)) => {
                let mut snapshot = Snapshot::default();
                snapshot.set_metadata(metadata);
                snapshot.set_data(data.into());
                Ok(snapshot)
            }
            Ok(None) => {
                // No snapshot data yet
                let mut snapshot = Snapshot::default();
                snapshot.set_metadata(metadata);
                Ok(snapshot)
            }
            Err(e) => {
                warn!("Failed to load snapshot: {}", e);
                Err(RaftError::Store(StorageError::Other(e.to_string().into())))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_raft_storage_basic() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path().join("test.db")).unwrap());
        let raft_storage = RaftStorage::new(storage).await.unwrap();

        // Test initial state
        let state = raft_storage.initial_state().unwrap();
        assert_eq!(state.hard_state, HardState::default());
        assert_eq!(state.conf_state, ConfState::default());

        // Test saving hard state
        let mut hs = HardState::default();
        hs.set_term(1);
        hs.set_vote(2);
        raft_storage.save_hard_state(&hs).await.unwrap();

        let state = raft_storage.initial_state().unwrap();
        assert_eq!(state.hard_state.term, 1);
        assert_eq!(state.hard_state.vote, 2);
    }
}
