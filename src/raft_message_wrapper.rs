use raft::prelude::*;
use serde::{Serialize, Deserialize};

/// Wrapper for raft Message to add serde support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMessageWrapper {
    pub msg_type: i32,  // raft uses i32 for MessageType
    pub to: u64,
    pub from: u64,
    pub term: u64,
    pub log_term: u64,
    pub index: u64,
    pub entries: Vec<EntryWrapper>,
    pub commit: u64,
    pub snapshot: Option<SnapshotWrapper>,
    pub reject: bool,
    pub reject_hint: u64,
    pub context: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryWrapper {
    pub entry_type: i32,  // raft uses i32 for EntryType
    pub term: u64,
    pub index: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotWrapper {
    pub data: Vec<u8>,
    pub metadata: SnapshotMetadataWrapper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadataWrapper {
    pub conf_state: Option<ConfStateWrapper>,
    pub index: u64,
    pub term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfStateWrapper {
    pub voters: Vec<u64>,
    pub learners: Vec<u64>,
}

impl From<Message> for RaftMessageWrapper {
    fn from(msg: Message) -> Self {
        RaftMessageWrapper {
            msg_type: msg.msg_type,
            to: msg.to,
            from: msg.from,
            term: msg.term,
            log_term: msg.log_term,
            index: msg.index,
            entries: msg.entries.into_iter().map(EntryWrapper::from).collect(),
            commit: msg.commit,
            snapshot: msg.snapshot.map(SnapshotWrapper::from),
            reject: msg.reject,
            reject_hint: msg.reject_hint,
            context: msg.context,
        }
    }
}

impl From<RaftMessageWrapper> for Message {
    fn from(wrapper: RaftMessageWrapper) -> Self {
        let mut msg = Message::default();
        msg.msg_type = wrapper.msg_type;
        msg.to = wrapper.to;
        msg.from = wrapper.from;
        msg.term = wrapper.term;
        msg.log_term = wrapper.log_term;
        msg.index = wrapper.index;
        msg.entries = wrapper.entries.into_iter().map(Entry::from).collect();
        msg.commit = wrapper.commit;
        msg.snapshot = wrapper.snapshot.map(Snapshot::from);
        msg.reject = wrapper.reject;
        msg.reject_hint = wrapper.reject_hint;
        msg.context = wrapper.context;
        msg
    }
}

impl From<Entry> for EntryWrapper {
    fn from(entry: Entry) -> Self {
        EntryWrapper {
            entry_type: entry.entry_type,
            term: entry.term,
            index: entry.index,
            data: entry.data,
        }
    }
}

impl From<EntryWrapper> for Entry {
    fn from(wrapper: EntryWrapper) -> Self {
        let mut entry = Entry::default();
        entry.entry_type = wrapper.entry_type;
        entry.term = wrapper.term;
        entry.index = wrapper.index;
        entry.data = wrapper.data;
        entry
    }
}

impl From<Snapshot> for SnapshotWrapper {
    fn from(snapshot: Snapshot) -> Self {
        SnapshotWrapper {
            data: snapshot.data,
            metadata: snapshot.metadata.map(SnapshotMetadataWrapper::from).unwrap_or_else(|| SnapshotMetadataWrapper {
                conf_state: None,
                index: 0,
                term: 0,
            }),
        }
    }
}

impl From<SnapshotWrapper> for Snapshot {
    fn from(wrapper: SnapshotWrapper) -> Self {
        let mut snapshot = Snapshot::default();
        snapshot.data = wrapper.data;
        snapshot.metadata = Some(wrapper.metadata.into());
        snapshot
    }
}

impl From<SnapshotMetadata> for SnapshotMetadataWrapper {
    fn from(metadata: SnapshotMetadata) -> Self {
        SnapshotMetadataWrapper {
            conf_state: metadata.conf_state.map(ConfStateWrapper::from),
            index: metadata.index,
            term: metadata.term,
        }
    }
}

impl From<SnapshotMetadataWrapper> for SnapshotMetadata {
    fn from(wrapper: SnapshotMetadataWrapper) -> Self {
        let mut metadata = SnapshotMetadata::default();
        metadata.conf_state = wrapper.conf_state.map(ConfState::from);
        metadata.index = wrapper.index;
        metadata.term = wrapper.term;
        metadata
    }
}

impl From<ConfState> for ConfStateWrapper {
    fn from(conf_state: ConfState) -> Self {
        ConfStateWrapper {
            voters: conf_state.voters,
            learners: conf_state.learners,
        }
    }
}

impl From<ConfStateWrapper> for ConfState {
    fn from(wrapper: ConfStateWrapper) -> Self {
        let mut conf_state = ConfState::default();
        conf_state.voters = wrapper.voters;
        conf_state.learners = wrapper.learners;
        conf_state
    }
}