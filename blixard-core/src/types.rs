use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use crate::anti_affinity::AntiAffinityRules;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: u64,
    pub data_dir: String,
    pub bind_addr: SocketAddr,
    pub join_addr: Option<String>,
    pub use_tailscale: bool,
    pub vm_backend: String, // Backend type: "mock", "microvm", "docker", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_config: Option<crate::transport::config::TransportConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub config_path: String,
    pub vcpus: u32,
    pub memory: u32, // MB
    #[serde(default = "default_tenant")]
    pub tenant_id: String, // Tenant identifier for multi-tenancy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>, // VM IP address for network isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>, // Metadata for Nix images, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_affinity: Option<AntiAffinityRules>, // Anti-affinity rules for placement
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            config_path: String::new(),
            vcpus: 1,
            memory: 1024,
            tenant_id: default_tenant(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
        }
    }
}

fn default_tenant() -> String {
    "default".to_string()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    Uninitialized,
    Initialized,
    JoiningCluster,
    Active,
    LeavingCluster,
    Error,
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
    Migrate { task: VmMigrationTask },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMigrationTask {
    pub vm_name: String,
    pub source_node_id: u64,
    pub target_node_id: u64,
    pub live_migration: bool,
    pub force: bool,
}
