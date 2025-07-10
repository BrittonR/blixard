//! Debug-related state management

use crate::tui::types::debug::{RaftDebugInfo, DebugMetrics, DebugLogEntry};
use crate::tui::types::p2p::{P2pPeer, P2pImage, P2pTransfer};
use std::sync::Arc;

#[derive(Debug)]
pub struct DebugState {
    // Raft debugging
    pub raft_debug_info: Option<RaftDebugInfo>,
    
    // Debug metrics
    pub debug_metrics: DebugMetrics,
    
    // Debug logs
    pub debug_log_entries: Vec<DebugLogEntry>,
    pub max_debug_entries: usize,
    
    // P2P state
    pub p2p_enabled: bool,
    pub p2p_node_id: String,
    pub p2p_peer_count: usize,
    pub p2p_shared_images: usize,
    pub p2p_peers: Vec<P2pPeer>,
    pub p2p_images: Vec<P2pImage>,
    pub p2p_transfers: Vec<P2pTransfer>,
    pub p2p_store: Option<Arc<tokio::sync::Mutex<blixard_core::p2p_image_store::P2pImageStore>>>,
    pub p2p_manager: Option<Arc<blixard_core::p2p_manager::P2pManager>>,
}

impl DebugState {
    pub fn new() -> Self {
        Self {
            raft_debug_info: None,
            debug_metrics: DebugMetrics {
                messages_sent: 0,
                messages_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
                errors_count: 0,
                warnings_count: 0,
                raft_proposals: 0,
                raft_commits: 0,
                snapshot_count: 0,
                last_snapshot_size: 0,
            },
            debug_log_entries: Vec::new(),
            max_debug_entries: 1000,
            p2p_enabled: false,
            p2p_node_id: String::new(),
            p2p_peer_count: 0,
            p2p_shared_images: 0,
            p2p_peers: Vec::new(),
            p2p_images: Vec::new(),
            p2p_transfers: Vec::new(),
            p2p_store: None,
            p2p_manager: None,
        }
    }

    pub fn add_debug_log(&mut self, entry: DebugLogEntry) {
        self.debug_log_entries.push(entry);
        if self.debug_log_entries.len() > self.max_debug_entries {
            self.debug_log_entries.remove(0);
        }
    }

    pub fn update_raft_debug_info(&mut self, info: RaftDebugInfo) {
        self.raft_debug_info = Some(info);
    }

    pub fn update_debug_metrics<F>(&mut self, update_fn: F)
    where
        F: FnOnce(&mut DebugMetrics),
    {
        update_fn(&mut self.debug_metrics);
    }

    pub fn reset_debug_metrics(&mut self) {
        self.debug_metrics = DebugMetrics {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors_count: 0,
            warnings_count: 0,
            raft_proposals: 0,
            raft_commits: 0,
            snapshot_count: 0,
            last_snapshot_size: 0,
        };
    }
}