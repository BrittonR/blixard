use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub id: u64,
    pub data_dir: String,
    pub bind_addr: SocketAddr,
    pub join_addr: Option<SocketAddr>,
    pub use_tailscale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory: u32, // MB
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    Creating,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmState {
    pub name: String,
    pub config: VmConfig,
    pub status: VmStatus,
    pub node_id: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmCommand {
    Create { config: VmConfig, node_id: u64 },
    Start { name: String },
    Stop { name: String },
    Delete { name: String },
    UpdateStatus { name: String, status: VmStatus },
}
