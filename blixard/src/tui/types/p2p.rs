//! P2P-related types for the TUI

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct P2pPeer {
    pub node_id: String,
    pub address: String,
    pub status: P2pPeerStatus,
    pub latency_ms: Option<u32>,
    pub bandwidth_mbps: f32,
    pub shared_images: u32,
    pub last_activity: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum P2pPeerStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

impl P2pPeerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            P2pPeerStatus::Connected => "Connected",
            P2pPeerStatus::Connecting => "Connecting",
            P2pPeerStatus::Disconnected => "Disconnected",
            P2pPeerStatus::Error => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct P2pImage {
    pub name: String,
    pub hash: String,
    pub size_bytes: u64,
    pub availability: u32, // number of peers with this image
    pub is_local: bool,
    pub last_accessed: Option<Instant>,
    pub is_cached: bool,
    pub is_downloading: bool,
    pub version: String,
    pub size: u64,
    pub available_peers: u32,
}

#[derive(Debug, Clone)]
pub struct P2pTransfer {
    pub image_name: String,
    pub peer_id: String,
    pub direction: TransferDirection,
    pub progress: f32,
    pub speed_mbps: f32,
    pub started_at: Instant,
    pub estimated_completion: Option<Instant>,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub is_upload: bool,
    pub resource_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkQuality {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Reconnecting,
}