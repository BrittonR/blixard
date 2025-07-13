use crate::anti_affinity::AntiAffinityRules;
use crate::vm_health_types::VmHealthCheckConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;

/// Unique identifier for a VM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VmId(pub Uuid);

impl VmId {
    /// Create a new random VM ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a VM ID from a string (deterministic based on input)
    pub fn from_string(s: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        let hash = hasher.finish();

        // Create a deterministic UUID v4 from the hash
        let bytes = [
            (hash >> 56) as u8,
            (hash >> 48) as u8,
            (hash >> 40) as u8,
            (hash >> 32) as u8,
            (hash >> 24) as u8,
            (hash >> 16) as u8,
            0x40 | ((hash >> 12) & 0x0f) as u8, // Version 4
            (hash >> 8) as u8,
            0x80 | ((hash >> 4) & 0x3f) as u8, // Variant bits
            hash as u8,
            (hash >> 32) as u8,
            (hash >> 24) as u8,
            (hash >> 16) as u8,
            (hash >> 8) as u8,
            hash as u8,
            (hash >> 56) as u8,
        ];

        Self(Uuid::from_bytes(bytes))
    }
}

impl std::fmt::Display for VmId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// From/Into conversions for VmId
impl From<uuid::Uuid> for VmId {
    fn from(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }
}

impl From<VmId> for uuid::Uuid {
    fn from(vm_id: VmId) -> Self {
        vm_id.0
    }
}

impl From<VmId> for String {
    fn from(vm_id: VmId) -> Self {
        vm_id.0.to_string()
    }
}

impl std::str::FromStr for VmId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}

impl TryFrom<String> for VmId {
    type Error = uuid::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for VmId {
    type Error = uuid::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

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
    pub ip_address: Option<String>, // VM IP address for network isolation
    pub metadata: Option<std::collections::HashMap<String, String>>, // Metadata for Nix images, etc.
    pub anti_affinity: Option<AntiAffinityRules>, // Anti-affinity rules for placement
    #[serde(default = "default_priority")]
    pub priority: u32, // Priority level (0-1000, higher is more important)
    #[serde(default = "default_preemptible")]
    pub preemptible: bool, // Whether this VM can be preempted
    #[serde(default)]
    pub locality_preference: LocalityPreference, // Datacenter locality preferences
    pub health_check_config: Option<VmHealthCheckConfig>, // Health check configuration
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
            health_check_config: None,
        }
    }
}

impl VmConfig {
    /// Validate the VM configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.priority > 1000 {
            return Err(format!(
                "Priority must be between 0 and 1000, got {}",
                self.priority
            ));
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

// From/Into conversions for VmStatus
impl std::fmt::Display for VmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            VmStatus::Creating => "creating",
            VmStatus::Starting => "starting",
            VmStatus::Running => "running",
            VmStatus::Stopping => "stopping",
            VmStatus::Stopped => "stopped",
            VmStatus::Failed => "failed",
        };
        write!(f, "{}", status_str)
    }
}

impl From<VmStatus> for String {
    fn from(status: VmStatus) -> Self {
        status.to_string()
    }
}

impl std::str::FromStr for VmStatus {
    type Err = crate::error::BlixardError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "creating" => Ok(VmStatus::Creating),
            "starting" => Ok(VmStatus::Starting),
            "running" => Ok(VmStatus::Running),
            "stopping" => Ok(VmStatus::Stopping),
            "stopped" => Ok(VmStatus::Stopped),
            "failed" => Ok(VmStatus::Failed),
            _ => Err(crate::error::BlixardError::InvalidInput {
                field: "vm_status".to_string(),
                message: format!("Invalid VM status '{}'. Valid values: creating, starting, running, stopping, stopped, failed", s),
            }),
        }
    }
}

impl TryFrom<String> for VmStatus {
    type Error = crate::error::BlixardError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for VmStatus {
    type Error = crate::error::BlixardError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

// Protocol compatibility conversions
impl From<VmStatus> for i32 {
    fn from(status: VmStatus) -> Self {
        match status {
            VmStatus::Creating => 1,
            VmStatus::Starting => 2,
            VmStatus::Running => 3,
            VmStatus::Stopping => 4,
            VmStatus::Stopped => 5,
            VmStatus::Failed => 6,
        }
    }
}

impl TryFrom<i32> for VmStatus {
    type Error = crate::error::BlixardError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(VmStatus::Creating),
            2 => Ok(VmStatus::Starting),
            3 => Ok(VmStatus::Running),
            4 => Ok(VmStatus::Stopping),
            5 => Ok(VmStatus::Stopped),
            6 => Ok(VmStatus::Failed),
            _ => Err(crate::error::BlixardError::InvalidInput {
                field: "vm_status".to_string(),
                message: format!("Invalid VM status code {}. Valid codes: 1-6", value),
            }),
        }
    }
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

// From/Into conversions for NodeState
impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state_str = match self {
            NodeState::Uninitialized => "uninitialized",
            NodeState::Initialized => "initialized",
            NodeState::JoiningCluster => "joining_cluster",
            NodeState::Active => "active",
            NodeState::LeavingCluster => "leaving_cluster",
            NodeState::Error => "error",
        };
        write!(f, "{}", state_str)
    }
}

impl From<NodeState> for String {
    fn from(state: NodeState) -> Self {
        state.to_string()
    }
}

impl std::str::FromStr for NodeState {
    type Err = crate::error::BlixardError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "uninitialized" => Ok(NodeState::Uninitialized),
            "initialized" => Ok(NodeState::Initialized),
            "joining_cluster" | "joining-cluster" => Ok(NodeState::JoiningCluster),
            "active" => Ok(NodeState::Active),
            "leaving_cluster" | "leaving-cluster" => Ok(NodeState::LeavingCluster),
            "error" => Ok(NodeState::Error),
            _ => Err(crate::error::BlixardError::InvalidInput {
                field: "node_state".to_string(),
                message: format!("Invalid node state '{}'. Valid values: uninitialized, initialized, joining_cluster, active, leaving_cluster, error", s),
            }),
        }
    }
}

impl TryFrom<String> for NodeState {
    type Error = crate::error::BlixardError;

    fn try_from(s: String) -> Result<Self, <Self as TryFrom<String>>::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for NodeState {
    type Error = crate::error::BlixardError;

    fn try_from(s: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        s.parse()
    }
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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


impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            id: 1,
            data_dir: "./data".to_string(),
            bind_addr: "127.0.0.1:7001".parse().expect("Invalid default bind address"),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: NodeTopology::default(),
        }
    }
}

// NodeId type with conversions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<NodeId> for u64 {
    fn from(node_id: NodeId) -> Self {
        node_id.0
    }
}

impl From<NodeId> for String {
    fn from(node_id: NodeId) -> Self {
        node_id.0.to_string()
    }
}

impl std::str::FromStr for NodeId {
    type Err = crate::error::BlixardError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>()
            .map(Self)
            .map_err(|e| crate::error::BlixardError::InvalidInput {
                field: "node_id".to_string(),
                message: format!("Invalid node ID '{}': {}", s, e),
            })
    }
}

impl TryFrom<String> for NodeId {
    type Error = crate::error::BlixardError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for NodeId {
    type Error = crate::error::BlixardError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

// SocketAddr parsing helpers
pub trait SocketAddrExt {
    fn try_from_string(s: String) -> Result<SocketAddr, crate::error::BlixardError>;
    fn try_from_str(s: &str) -> Result<SocketAddr, crate::error::BlixardError>;
}

impl SocketAddrExt for SocketAddr {
    fn try_from_string(s: String) -> Result<SocketAddr, crate::error::BlixardError> {
        s.parse().map_err(|e| crate::error::BlixardError::InvalidInput {
            field: "socket_address".to_string(),
            message: format!("Invalid socket address '{}': {}", s, e),
        })
    }

    fn try_from_str(s: &str) -> Result<SocketAddr, crate::error::BlixardError> {
        s.parse().map_err(|e| crate::error::BlixardError::InvalidInput {
            field: "socket_address".to_string(),
            message: format!("Invalid socket address '{}': {}", s, e),
        })
    }
}

// Metadata conversion helpers
pub mod metadata {
    use std::collections::HashMap;

    /// Convert a vector of string pairs to a HashMap
    pub fn from_string_pairs(pairs: Vec<(String, String)>) -> HashMap<String, String> {
        pairs.into_iter().collect()
    }

    /// Convert a vector of str pairs to a HashMap with owned strings
    pub fn from_str_pairs(pairs: Vec<(&str, &str)>) -> HashMap<String, String> {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// Convert key-value pairs from any iterator to HashMap
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> HashMap<String, String>
    where
        K: Into<String>,
        V: Into<String>,
    {
        pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect()
    }
}

// Option to Result helpers
pub trait OptionExt<T> {
    fn ok_or_invalid_input(self, field: &str, message: &str) -> Result<T, crate::error::BlixardError>;
    fn ok_or_not_found(self, resource: &str) -> Result<T, crate::error::BlixardError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_invalid_input(self, field: &str, message: &str) -> Result<T, crate::error::BlixardError> {
        self.ok_or_else(|| crate::error::BlixardError::InvalidInput {
            field: field.to_string(),
            message: message.to_string(),
        })
    }

    fn ok_or_not_found(self, resource: &str) -> Result<T, crate::error::BlixardError> {
        self.ok_or_else(|| crate::error::BlixardError::NotFound {
            resource: resource.to_string(),
        })
    }
}

/// Hypervisor types supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hypervisor {
    CloudHypervisor,
    Firecracker,
    Qemu,
}

impl std::fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hypervisor::CloudHypervisor => write!(f, "cloud-hypervisor"),
            Hypervisor::Firecracker => write!(f, "firecracker"),
            Hypervisor::Qemu => write!(f, "qemu"),
        }
    }
}

impl std::str::FromStr for Hypervisor {
    type Err = crate::error::BlixardError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cloud-hypervisor" | "cloudhypervisor" => Ok(Hypervisor::CloudHypervisor),
            "firecracker" => Ok(Hypervisor::Firecracker),
            "qemu" => Ok(Hypervisor::Qemu),
            _ => Err(crate::error::BlixardError::InvalidInput {
                field: "hypervisor".to_string(),
                message: format!("Invalid hypervisor type: {}. Valid options: cloud-hypervisor, firecracker, qemu", s),
            }),
        }
    }
}

// =============================================================================
// Type Aliases for Common Complex Types
// =============================================================================

/// Binary data for serialization, network protocols, and file operations
/// 
/// Used extensively throughout the codebase for:
/// - Raft message serialization/deserialization
/// - File I/O operations
/// - Network protocol buffers
/// - Cryptographic operations
/// - Image and blob storage
pub type BinaryData = Vec<u8>;

/// String-to-string metadata and configuration maps
/// 
/// Common usage patterns:
/// - VM metadata and labels
/// - Configuration key-value pairs
/// - Audit log context information
/// - Service discovery attributes
pub type Metadata = HashMap<String, String>;

/// Map keyed by node IDs for cluster-wide state tracking
/// 
/// Used for:
/// - Raft next_index and match_index tracking
/// - Node health and status monitoring
/// - Test cluster management
/// - Peer connection state
pub type NodeMap<T> = HashMap<NodeId, T>;

/// Shared concurrent map for name-based resource lookups (VMs, configs, etc.)
/// 
/// Commonly used for:
/// - VM state management
/// - Configuration storage
/// - Resource name resolution
/// - Service registration
pub type SharedResourceMap<T> = Arc<RwLock<HashMap<String, T>>>;

/// Shared concurrent map for node ID-based lookups (peers, node states)
/// 
/// Used for:
/// - Peer connection management
/// - Node state tracking
/// - Cluster membership
/// - Health monitoring
pub type SharedNodeMap<T> = Arc<RwLock<HashMap<NodeId, T>>>;

/// Shared concurrent map for arbitrary key-value mappings
/// 
/// Generic shared map for any concurrent access patterns requiring
/// multiple readers or exclusive writers
pub type SharedMap<K, V> = Arc<RwLock<HashMap<K, V>>>;

/// Synchronized map for mutex-protected concurrent access
/// 
/// Used in contexts requiring exclusive access for all operations:
/// - Raft consensus data structures
/// - Test utilities and simulation state
/// - Byzantine failure testing
pub type SyncMap<K, V> = Arc<Mutex<HashMap<K, V>>>;

// =============================================================================
// Specialized Domain Type Aliases
// =============================================================================

/// Raft log entry index for consensus operations
/// 
/// Represents the position of an entry in the Raft log, used for:
/// - Log replication tracking
/// - Snapshot coordination
/// - Recovery operations
pub type LogIndex = u64;

/// Raft term number for leader election
/// 
/// Monotonically increasing term identifier used for:
/// - Leader election cycles
/// - Log entry versioning
/// - Split-brain prevention
pub type Term = u64;
