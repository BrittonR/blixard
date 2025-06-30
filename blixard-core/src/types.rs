use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use crate::anti_affinity::AntiAffinityRules;

/// Node topology information for multi-datacenter awareness
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeTopology {
    /// Datacenter identifier (e.g., "us-east-1", "eu-west-1")
    pub datacenter: String,
    /// Availability zone within the datacenter (e.g., "zone-a", "zone-b")
    pub zone: String,
    /// Rack identifier within the zone (e.g., "rack-42")
    pub rack: String,
}

impl Default for NodeTopology {
    fn default() -> Self {
        Self {
            datacenter: "default-dc".to_string(),
            zone: "default-zone".to_string(),
            rack: "default-rack".to_string(),
        }
    }
}

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
    #[serde(default)]
    pub topology: NodeTopology, // Datacenter/zone/rack topology information
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
    #[serde(default = "default_priority")]
    pub priority: u32, // Priority level (0-1000, higher is more important)
    #[serde(default = "default_preemptible")]
    pub preemptible: bool, // Whether this VM can be preempted
    #[serde(default)]
    pub locality_preference: LocalityPreference, // Datacenter locality preferences
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
            priority: default_priority(),
            preemptible: default_preemptible(),
            locality_preference: LocalityPreference::default(),
        }
    }
}

impl VmConfig {
    /// Validate the VM configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.priority > 1000 {
            return Err(format!("Priority must be between 0 and 1000, got {}", self.priority));
        }
        if self.name.is_empty() {
            return Err("VM name cannot be empty".to_string());
        }
        if self.vcpus == 0 {
            return Err("VM must have at least 1 vCPU".to_string());
        }
        if self.memory == 0 {
            return Err("VM must have at least 1 MB of memory".to_string());
        }
        Ok(())
    }
}

fn default_tenant() -> String {
    "default".to_string()
}

fn default_priority() -> u32 {
    500 // Default to middle priority
}

fn default_preemptible() -> bool {
    true // Default to preemptible for better resource utilization
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

/// Locality preferences for VM placement across datacenters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalityPreference {
    /// Preferred datacenter for primary placement
    pub preferred_datacenter: Option<String>,
    /// Preferred zone within the datacenter
    pub preferred_zone: Option<String>,
    /// Whether to enforce same-datacenter placement for all instances
    pub enforce_same_datacenter: bool,
    /// Whether to spread across zones for high availability
    pub spread_across_zones: bool,
    /// Maximum network latency tolerance in milliseconds
    pub max_latency_ms: Option<u32>,
    /// List of excluded datacenters
    pub excluded_datacenters: Vec<String>,
}

impl Default for LocalityPreference {
    fn default() -> Self {
        Self {
            preferred_datacenter: None,
            preferred_zone: None,
            enforce_same_datacenter: false,
            spread_across_zones: false,
            max_latency_ms: None,
            excluded_datacenters: Vec::new(),
        }
    }
}
