//! Debug-related types for the TUI

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RaftDebugInfo {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub log_length: u64,
    pub commit_index: u64,
    pub last_applied: u64,
    pub state: RaftNodeState,
    pub last_heartbeat: Option<Instant>,
    pub election_timeout: Duration,
    pub entries_since_snapshot: u64,
    pub snapshot_metadata: Option<SnapshotMetadata>,
    pub peer_states: Vec<PeerDebugInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RaftNodeState {
    Follower,
    Candidate,
    Leader,
    PreCandidate,
}

#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub size_bytes: u64,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct PeerDebugInfo {
    pub id: u64,
    pub next_index: u64,
    pub match_index: u64,
    pub state: PeerState,
    pub last_activity: Option<Instant>,
    pub is_reachable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeerState {
    Probe,
    Replicate,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct DebugMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors_count: u64,
    pub warnings_count: u64,
    pub raft_proposals: u64,
    pub raft_commits: u64,
    pub snapshot_count: u64,
    pub last_snapshot_size: u64,
}

#[derive(Debug, Clone)]
pub struct DebugLogEntry {
    pub timestamp: Instant,
    pub level: DebugLevel,
    pub component: String,
    pub message: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebugLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}