//! Configuration management system with TOML support and hot-reload
//!
//! This module provides a comprehensive configuration system that:
//! - Loads from TOML files
//! - Supports environment variable overrides
//! - Validates configuration values
//! - Supports hot-reload for non-critical settings
//!
//! # Example Configuration
//!
//! ```toml
//! # Node configuration
//! [node]
//! id = 1
//! bind_address = "127.0.0.1:7001"
//! data_dir = "./data"
//! vm_backend = "microvm"
//! debug = false
//!
//! # Cluster configuration
//! [cluster]
//! bootstrap = false
//! join_address = "127.0.0.1:7002"
//!
//! # Network configuration
//! [network.iroh]
//! max_message_size = 67108864
//! connection_timeout = "10s"
//! keepalive_interval = "10s"
//! home_relay = "https://relay.iroh.network"
//! discovery_port = 0
//!
//! # Discovery configuration
//! [network.discovery]
//! enable_static = true
//! enable_dns = false
//! enable_mdns = true
//! refresh_interval = 30
//! node_stale_timeout = 300
//!
//! # Static seed nodes
//! [[network.discovery.static_nodes.seeds]]
//! node_id = "kyt7jjyhhg5xzlwtqwwgrkwhdtmqag6tg2nqwhz4o4nzqhj3a"
//! addresses = ["127.0.0.1:7002", "192.168.1.100:7002"]
//! name = "seed-node-1"
//!
//! [[network.discovery.static_nodes.seeds]]
//! node_id = "mln8jjyhhg5xzlwtqwwgrkwhdtmqag6tg2nqwhz4o4nzqhj3b"
//! addresses = ["10.0.0.5:7002"]
//! name = "seed-node-2"
//!
//! # DNS discovery
//! [network.discovery.dns]
//! domains = ["_iroh._tcp.blixard.local", "_blixard._tcp.example.com"]
//! dns_server = "8.8.8.8:53"  # Optional, uses system DNS if not specified
//! query_timeout = 5
//!
//! # mDNS discovery
//! [network.discovery.mdns]
//! service_name = "_blixard-iroh._tcp.local"
//! enable_ipv4 = true
//! enable_ipv6 = true
//! ttl = 120
//!
//! # Security configuration
//! [security.tls]
//! enabled = true
//! cert_file = "/path/to/cert.pem"
//! key_file = "/path/to/key.pem"
//! ca_file = "/path/to/ca.pem"
//! require_client_cert = true
//!
//! [security.auth]
//! enabled = true
//! method = "token"
//! token_file = "/path/to/tokens.json"
//! ```

use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;

/// Global configuration instance
static CONFIG: once_cell::sync::OnceCell<Arc<RwLock<Config>>> = once_cell::sync::OnceCell::new();

/// Configuration watcher for hot-reload
static CONFIG_WATCHER: once_cell::sync::OnceCell<watch::Sender<Arc<Config>>> =
    once_cell::sync::OnceCell::new();

/// Complete configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Node configuration
    pub node: NodeConfig,

    /// Cluster configuration
    pub cluster: ClusterConfig,

    /// Storage configuration
    pub storage: StorageConfig,

    /// Network configuration
    pub network: NetworkConfig,

    /// VM management configuration
    pub vm: VmConfig,

    /// Observability configuration
    pub observability: ObservabilityConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// P2P configuration
    pub p2p: P2pConfig,

    /// Transport configuration
    pub transport: Option<crate::transport::config::TransportConfig>,
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// Node ID (can be overridden by CLI)
    pub id: Option<u64>,

    /// Bind address for Iroh P2P node
    pub bind_address: String,

    /// Data directory for storage
    pub data_dir: PathBuf,

    /// VM backend type
    pub vm_backend: String,

    /// Enable debug mode
    pub debug: bool,
}

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    /// Bootstrap as single-node cluster
    pub bootstrap: bool,

    /// Join address for existing cluster
    pub join_address: Option<String>,

    /// Raft configuration
    pub raft: RaftConfig,

    /// Peer connection settings
    pub peer: PeerConfig,

    /// Worker defaults
    pub worker: WorkerConfig,
}

/// Raft configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RaftConfig {
    /// Election timeout range in milliseconds
    pub election_tick: u64,

    /// Heartbeat interval in milliseconds
    pub heartbeat_tick: u64,

    /// Maximum size of uncommitted entries
    pub max_uncommitted_size: u64,

    /// Maximum inflight messages
    pub max_inflight_msgs: usize,

    /// Log compaction threshold
    pub compaction_threshold: u64,

    /// Snapshot interval
    pub snapshot_interval: u64,

    /// Configuration change timeout
    #[serde(with = "humantime_serde")]
    pub conf_change_timeout: Duration,

    /// Proposal timeout
    #[serde(with = "humantime_serde")]
    pub proposal_timeout: Duration,

    /// Batch processing configuration
    pub batch_processing: RaftBatchConfig,

    /// Maximum restart attempts
    pub max_restarts: u32,

    /// Restart base delay
    #[serde(with = "humantime_serde")]
    pub restart_delay: Duration,
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionPoolConfig {
    /// Maximum connections per peer
    pub max_connections_per_peer: usize,

    /// Maximum number of peers
    pub max_peers: usize,

    /// Idle timeout in seconds
    pub idle_timeout_secs: u64,

    /// Maximum connection lifetime in seconds
    pub max_lifetime_secs: u64,

    /// Cleanup interval in seconds
    pub cleanup_interval_secs: u64,
}

/// Peer connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PeerConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,

    /// Connection pool size per peer
    pub pool_size: usize,

    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connection_timeout: Duration,

    /// Reconnection delay
    #[serde(with = "humantime_serde")]
    pub reconnect_delay: Duration,

    /// Health check interval
    #[serde(with = "humantime_serde")]
    pub health_check_interval: Duration,

    /// Failure threshold before marking unhealthy
    pub failure_threshold: u32,

    /// Maximum buffered messages per peer
    pub max_buffered_messages: usize,

    /// Enable connection pooling
    pub enable_pooling: bool,

    /// Connection pool configuration
    pub connection_pool: ConnectionPoolConfig,
}

/// Worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerConfig {
    /// Default CPU cores
    pub default_cpu_cores: u32,

    /// Default memory in MB
    pub default_memory_mb: u64,

    /// Default disk in GB
    pub default_disk_gb: u64,

    /// Resource update interval
    #[serde(with = "humantime_serde")]
    pub resource_update_interval: Duration,

    /// Health check timeout
    #[serde(with = "humantime_serde")]
    pub health_check_timeout: Duration,
}

/// Raft batch processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RaftBatchConfig {
    /// Whether batch processing is enabled
    pub enabled: bool,

    /// Maximum number of proposals in a single batch
    pub max_batch_size: usize,

    /// Maximum time to wait before flushing a batch (ms)
    pub batch_timeout_ms: u64,

    /// Maximum bytes in a single batch
    pub max_batch_bytes: usize,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Database file name
    pub db_name: String,

    /// Cache size in MB
    pub cache_size_mb: u64,

    /// Enable compression
    pub compression: bool,

    /// Sync mode (none, normal, full)
    pub sync_mode: String,

    /// Backup configuration
    pub backup: BackupConfig,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    /// Enable automatic backups
    pub enabled: bool,

    /// Backup directory
    pub backup_dir: PathBuf,

    /// Backup interval
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Maximum backups to retain
    pub max_backups: usize,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Iroh transport settings
    pub iroh: IrohTransportConfig,

    /// HTTP metrics server settings
    pub metrics: MetricsServerConfig,

    /// Node discovery configuration
    pub discovery: DiscoveryConfig,
}

/// Iroh transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IrohTransportConfig {
    /// Maximum message size
    pub max_message_size: usize,

    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connection_timeout: Duration,

    /// Keep-alive interval
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,

    /// Home relay URL
    pub home_relay: String,

    /// Discovery port (0 for random)
    pub discovery_port: u16,
}

/// Metrics server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsServerConfig {
    /// Enable metrics server
    pub enabled: bool,

    /// Port offset from main port
    pub port_offset: u16,

    /// Metrics path
    pub path: String,
}

/// Discovery configuration for finding Iroh nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Enable static discovery from configuration
    pub enable_static: bool,

    /// Enable DNS-based discovery
    pub enable_dns: bool,

    /// Enable mDNS for local network discovery
    pub enable_mdns: bool,

    /// How often to refresh discovery information (in seconds)
    pub refresh_interval: u64,

    /// Maximum age before considering a node stale (in seconds)
    pub node_stale_timeout: u64,

    /// Static nodes configuration
    pub static_nodes: StaticNodesConfig,

    /// DNS discovery settings
    pub dns: DnsDiscoveryConfig,

    /// mDNS discovery settings
    pub mdns: MdnsDiscoveryConfig,
}

/// Static nodes configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StaticNodesConfig {
    /// List of static seed nodes
    pub seeds: Vec<StaticNodeEntry>,
}

/// A static node entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticNodeEntry {
    /// Node ID (base32-encoded)
    pub node_id: String,

    /// Network addresses where the node can be reached
    pub addresses: Vec<String>,

    /// Optional human-readable name
    pub name: Option<String>,
}

/// DNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DnsDiscoveryConfig {
    /// DNS domains to query for SRV records
    pub domains: Vec<String>,

    /// DNS server to use (empty for system default)
    pub dns_server: Option<String>,

    /// Query timeout in seconds
    pub query_timeout: u64,
}

/// mDNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MdnsDiscoveryConfig {
    /// Service name to advertise and discover
    pub service_name: String,

    /// Enable IPv4 multicast
    pub enable_ipv4: bool,

    /// Enable IPv6 multicast
    pub enable_ipv6: bool,

    /// TTL for mDNS packets
    pub ttl: u32,
}

/// VM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmConfig {
    /// Default VM settings
    pub defaults: VmDefaults,

    /// Scheduler configuration
    pub scheduler: SchedulerConfig,

    /// Resource limits
    pub limits: ResourceLimits,
}

/// Default VM settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmDefaults {
    /// Default vCPUs
    pub vcpus: u32,

    /// Default memory in MB
    pub memory_mb: u32,

    /// Default disk in GB
    pub disk_gb: u32,

    /// Default hypervisor
    pub hypervisor: String,
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    /// Default placement strategy
    pub default_strategy: String,

    /// Enable anti-affinity
    pub enable_anti_affinity: bool,

    /// Resource overcommit ratio
    pub overcommit_ratio: f32,

    /// Rebalance interval
    #[serde(with = "humantime_serde")]
    pub rebalance_interval: Duration,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceLimits {
    /// Maximum VMs per node
    pub max_vms_per_node: usize,

    /// Maximum total vCPUs
    pub max_total_vcpus: u32,

    /// Maximum total memory in MB
    pub max_total_memory_mb: u64,

    /// Maximum total disk in GB
    pub max_total_disk_gb: u64,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// Logging configuration
    pub logging: LoggingConfig,

    /// Metrics configuration
    pub metrics: MetricsConfig,

    /// Tracing configuration
    pub tracing: TracingConfig,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,

    /// Log format (pretty, json)
    pub format: String,

    /// Enable timestamps
    pub timestamps: bool,

    /// Log file path (if any)
    pub file: Option<PathBuf>,

    /// Log rotation
    pub rotation: LogRotationConfig,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogRotationConfig {
    /// Enable rotation
    pub enabled: bool,

    /// Maximum file size in MB
    pub max_size_mb: u64,

    /// Maximum number of files
    pub max_files: usize,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metric prefix
    pub prefix: String,

    /// Include runtime metrics
    pub runtime_metrics: bool,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
    /// Enable tracing
    pub enabled: bool,

    /// OTLP endpoint
    pub otlp_endpoint: Option<String>,

    /// Service name
    pub service_name: String,

    /// Sampling ratio (0.0 - 1.0)
    pub sampling_ratio: f64,

    /// Include span events
    pub span_events: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// TLS configuration
    pub tls: TlsConfig,

    /// Authentication configuration
    pub auth: AuthConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,

    /// Certificate file
    pub cert_file: Option<PathBuf>,

    /// Key file
    pub key_file: Option<PathBuf>,

    /// CA certificate file
    pub ca_file: Option<PathBuf>,

    /// Require client certificates
    pub require_client_cert: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,

    /// Authentication method (token, mtls)
    pub method: String,

    /// Token file (for token auth)
    pub token_file: Option<PathBuf>,
}

/// P2P configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct P2pConfig {
    /// Enable P2P features
    pub enabled: bool,

    /// Enable VM image sharing via P2P
    pub enable_image_sharing: bool,

    /// Enable log aggregation via P2P
    pub enable_log_aggregation: bool,

    /// P2P port (in addition to gRPC port)
    pub port: u16,

    /// Relay servers for NAT traversal
    pub relay_servers: Vec<String>,

    /// Image cache configuration
    pub image_cache: ImageCacheConfig,
}

/// P2P image cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageCacheConfig {
    /// Maximum cache size in GB
    pub max_size_gb: u64,

    /// Cache eviction policy (lru, lfu, fifo)
    pub eviction_policy: String,

    /// Prefetch popular images
    pub enable_prefetch: bool,
}

// Default implementations
impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            cluster: ClusterConfig::default(),
            storage: StorageConfig::default(),
            network: NetworkConfig::default(),
            vm: VmConfig::default(),
            observability: ObservabilityConfig::default(),
            security: SecurityConfig::default(),
            p2p: P2pConfig::default(),
            transport: Some(crate::transport::config::TransportConfig::default()),
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            id: None,
            bind_address: "127.0.0.1:7001".to_string(),
            data_dir: PathBuf::from("./data"),
            vm_backend: "mock".to_string(),
            debug: false,
        }
    }
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            bootstrap: false,
            join_address: None,
            raft: RaftConfig::default(),
            peer: PeerConfig::default(),
            worker: WorkerConfig::default(),
        }
    }
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            election_tick: 10,
            heartbeat_tick: 2,
            max_uncommitted_size: 1 << 30, // 1GB
            max_inflight_msgs: 256,
            compaction_threshold: 1000,
            snapshot_interval: 10000,
            conf_change_timeout: Duration::from_secs(5),
            proposal_timeout: Duration::from_secs(10),
            batch_processing: RaftBatchConfig::default(),
            max_restarts: 5,
            restart_delay: Duration::from_secs(1),
        }
    }
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            pool_size: 10,
            connection_timeout: Duration::from_secs(5),
            reconnect_delay: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(10),
            failure_threshold: 5,
            max_buffered_messages: 100,
            enable_pooling: true,
            connection_pool: ConnectionPoolConfig::default(),
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_peer: 5,
            max_peers: 100,
            idle_timeout_secs: 300,    // 5 minutes
            max_lifetime_secs: 3600,   // 1 hour
            cleanup_interval_secs: 60, // 1 minute
        }
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            default_cpu_cores: 4,
            default_memory_mb: 8192,
            default_disk_gb: 100,
            resource_update_interval: Duration::from_secs(30),
            health_check_timeout: Duration::from_secs(5),
        }
    }
}

impl Default for RaftBatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 100,
            batch_timeout_ms: 10,
            max_batch_bytes: 1024 * 1024, // 1MB
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_name: "blixard.db".to_string(),
            cache_size_mb: 512,
            compression: true,
            sync_mode: "normal".to_string(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backup_dir: PathBuf::from("./backups"),
            interval: Duration::from_secs(3600),
            max_backups: 7,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            iroh: IrohTransportConfig::default(),
            metrics: MetricsServerConfig::default(),
            discovery: DiscoveryConfig::default(),
        }
    }
}

impl Default for IrohTransportConfig {
    fn default() -> Self {
        Self {
            max_message_size: 64 * 1024 * 1024, // 64MB
            connection_timeout: Duration::from_secs(10),
            keepalive_interval: Duration::from_secs(10),
            home_relay: "https://relay.iroh.network".to_string(),
            discovery_port: 0,
        }
    }
}

impl Default for MetricsServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port_offset: 1000,
            path: "/metrics".to_string(),
        }
    }
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            defaults: VmDefaults::default(),
            scheduler: SchedulerConfig::default(),
            limits: ResourceLimits::default(),
        }
    }
}

impl Default for VmDefaults {
    fn default() -> Self {
        Self {
            vcpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            hypervisor: "qemu".to_string(),
        }
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            default_strategy: "most_available".to_string(),
            enable_anti_affinity: true,
            overcommit_ratio: 1.2,
            rebalance_interval: Duration::from_secs(300),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_vms_per_node: 100,
            max_total_vcpus: 512,
            max_total_memory_mb: 131072, // 128GB
            max_total_disk_gb: 4096,     // 4TB
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            timestamps: true,
            file: None,
            rotation: LogRotationConfig::default(),
        }
    }
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_size_mb: 100,
            max_files: 10,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "blixard".to_string(),
            runtime_metrics: true,
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: None,
            service_name: "blixard".to_string(),
            sampling_ratio: 1.0,
            span_events: true,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tls: TlsConfig::default(),
            auth: AuthConfig::default(),
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: "token".to_string(),
            token_file: None,
        }
    }
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enable_image_sharing: true,
            enable_log_aggregation: false,
            port: 7002,
            relay_servers: vec![],
            image_cache: ImageCacheConfig::default(),
        }
    }
}

impl Default for ImageCacheConfig {
    fn default() -> Self {
        Self {
            max_size_gb: 10,
            eviction_policy: "lru".to_string(),
            enable_prefetch: false,
        }
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_static: true,
            enable_dns: false,
            enable_mdns: true,
            refresh_interval: 30,
            node_stale_timeout: 300, // 5 minutes
            static_nodes: StaticNodesConfig::default(),
            dns: DnsDiscoveryConfig::default(),
            mdns: MdnsDiscoveryConfig::default(),
        }
    }
}

impl Default for StaticNodesConfig {
    fn default() -> Self {
        Self { seeds: Vec::new() }
    }
}

impl Default for DnsDiscoveryConfig {
    fn default() -> Self {
        Self {
            domains: vec!["_iroh._tcp.blixard.local".to_string()],
            dns_server: None,
            query_timeout: 5,
        }
    }
}

impl Default for MdnsDiscoveryConfig {
    fn default() -> Self {
        Self {
            service_name: "_blixard-iroh._tcp.local".to_string(),
            enable_ipv4: true,
            enable_ipv6: true,
            ttl: 120,
        }
    }
}

// Configuration loading and management

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> BlixardResult<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| BlixardError::ConfigError(format!("Failed to read config file: {}", e)))?;

        let mut config: Config = toml::from_str(&contents)
            .map_err(|e| BlixardError::ConfigError(format!("Failed to parse TOML: {}", e)))?;

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        // Node configuration
        if let Ok(id) = std::env::var("BLIXARD_NODE_ID") {
            if let Ok(id) = id.parse() {
                self.node.id = Some(id);
            }
        }
        if let Ok(addr) = std::env::var("BLIXARD_BIND_ADDRESS") {
            self.node.bind_address = addr;
        }
        if let Ok(dir) = std::env::var("BLIXARD_DATA_DIR") {
            self.node.data_dir = PathBuf::from(dir);
        }

        // Cluster configuration
        if let Ok(addr) = std::env::var("BLIXARD_JOIN_ADDRESS") {
            self.cluster.join_address = Some(addr);
        }

        // Observability
        if let Ok(level) = std::env::var("BLIXARD_LOG_LEVEL") {
            self.observability.logging.level = level;
        }
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            self.observability.tracing.enabled = true;
            self.observability.tracing.otlp_endpoint = Some(endpoint);
        }

        // Security configuration
        if let Ok(enabled) = std::env::var("BLIXARD_TLS_ENABLED") {
            if let Ok(enabled) = enabled.parse() {
                self.security.tls.enabled = enabled;
            }
        }
        if let Ok(cert_file) = std::env::var("BLIXARD_TLS_CERT_FILE") {
            self.security.tls.cert_file = Some(PathBuf::from(cert_file));
        }
        if let Ok(key_file) = std::env::var("BLIXARD_TLS_KEY_FILE") {
            self.security.tls.key_file = Some(PathBuf::from(key_file));
        }
        if let Ok(ca_file) = std::env::var("BLIXARD_TLS_CA_FILE") {
            self.security.tls.ca_file = Some(PathBuf::from(ca_file));
        }
        if let Ok(require) = std::env::var("BLIXARD_TLS_REQUIRE_CLIENT_CERT") {
            if let Ok(require) = require.parse() {
                self.security.tls.require_client_cert = require;
            }
        }

        if let Ok(enabled) = std::env::var("BLIXARD_AUTH_ENABLED") {
            if let Ok(enabled) = enabled.parse() {
                self.security.auth.enabled = enabled;
            }
        }
        if let Ok(method) = std::env::var("BLIXARD_AUTH_METHOD") {
            self.security.auth.method = method;
        }
        if let Ok(token_file) = std::env::var("BLIXARD_AUTH_TOKEN_FILE") {
            self.security.auth.token_file = Some(PathBuf::from(token_file));
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate bind address
        if self.node.bind_address.is_empty() {
            return Err(BlixardError::ConfigError(
                "Bind address cannot be empty".to_string(),
            ));
        }

        // Validate data directory
        if self.node.data_dir.as_os_str().is_empty() {
            return Err(BlixardError::ConfigError(
                "Data directory cannot be empty".to_string(),
            ));
        }

        // Validate Raft configuration
        if self.cluster.raft.heartbeat_tick >= self.cluster.raft.election_tick {
            return Err(BlixardError::ConfigError(
                "Heartbeat tick must be less than election tick".to_string(),
            ));
        }

        // Validate resource limits
        if self.vm.scheduler.overcommit_ratio < 1.0 {
            return Err(BlixardError::ConfigError(
                "Overcommit ratio must be >= 1.0".to_string(),
            ));
        }

        // Validate log level
        match self.observability.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            _ => {
                return Err(BlixardError::ConfigError(format!(
                    "Invalid log level: {}",
                    self.observability.logging.level
                )))
            }
        }

        // Validate security configuration
        if self.security.tls.enabled {
            // If TLS is enabled, certificate and key files should be specified
            if self.security.tls.cert_file.is_none() || self.security.tls.key_file.is_none() {
                return Err(BlixardError::ConfigError(
                    "TLS enabled but cert_file or key_file not specified".to_string(),
                ));
            }
        }

        // Validate authentication method
        if self.security.auth.enabled {
            match self.security.auth.method.as_str() {
                "token" | "mtls" => {}
                _ => {
                    return Err(BlixardError::ConfigError(format!(
                        "Invalid authentication method: {}",
                        self.security.auth.method
                    )))
                }
            }

            // If using token auth, warn if no token file is specified
            if self.security.auth.method == "token" && self.security.auth.token_file.is_none() {
                // This is not an error since tokens can be generated dynamically,
                // but in production you'd typically want a token file
            }
        }

        // Validate discovery configuration
        if self.network.discovery.refresh_interval == 0 {
            return Err(BlixardError::ConfigError(
                "Discovery refresh interval must be greater than 0".to_string(),
            ));
        }

        if self.network.discovery.node_stale_timeout == 0 {
            return Err(BlixardError::ConfigError(
                "Discovery node stale timeout must be greater than 0".to_string(),
            ));
        }

        // Validate static node entries
        for (i, node) in self.network.discovery.static_nodes.seeds.iter().enumerate() {
            if node.node_id.is_empty() {
                return Err(BlixardError::ConfigError(format!(
                    "Static node {} has empty node_id",
                    i
                )));
            }
            if node.addresses.is_empty() {
                return Err(BlixardError::ConfigError(format!(
                    "Static node {} has no addresses",
                    i
                )));
            }
        }

        // Validate DNS configuration
        if self.network.discovery.enable_dns && self.network.discovery.dns.domains.is_empty() {
            return Err(BlixardError::ConfigError(
                "DNS discovery enabled but no domains configured".to_string(),
            ));
        }

        // Validate mDNS configuration
        if self.network.discovery.enable_mdns && self.network.discovery.mdns.service_name.is_empty()
        {
            return Err(BlixardError::ConfigError(
                "mDNS discovery enabled but service name is empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if a setting is hot-reloadable
    pub fn is_hot_reloadable(key: &str) -> bool {
        match key {
            // Hot-reloadable settings
            "observability.logging.level"
            | "observability.logging.format"
            | "cluster.peer.health_check_interval"
            | "cluster.peer.failure_threshold"
            | "cluster.worker.resource_update_interval"
            | "vm.scheduler.default_strategy"
            | "vm.scheduler.overcommit_ratio"
            | "network.iroh.keepalive_interval"
            | "network.iroh.connection_timeout"
            | "network.discovery.refresh_interval"
            | "network.discovery.node_stale_timeout"
            | "network.discovery.enable_static"
            | "network.discovery.enable_dns"
            | "network.discovery.enable_mdns" => true,

            // Non-reloadable settings (require restart)
            _ => false,
        }
    }
}

/// Initialize the global configuration
pub fn init<P: AsRef<Path>>(config_path: P) -> BlixardResult<()> {
    let config = Config::from_file(config_path)?;
    let config_arc = Arc::new(config);

    // Initialize watcher channel
    let (tx, _rx) = watch::channel(config_arc.clone());
    CONFIG_WATCHER
        .set(tx)
        .map_err(|_| BlixardError::ConfigError("Configuration already initialized".to_string()))?;

    // Set global config
    CONFIG
        .set(Arc::new(RwLock::new((*config_arc).clone())))
        .map_err(|_| BlixardError::ConfigError("Configuration already initialized".to_string()))?;

    Ok(())
}

/// Get the current configuration
pub fn get() -> Arc<Config> {
    if let Some(config) = CONFIG.get() {
        match config.read() {
            Ok(guard) => Arc::new(guard.clone()),
            Err(_) => {
                // Lock is poisoned, fallback to default
                tracing::error!("Configuration lock is poisoned, returning default config");
                Arc::new(Config::default())
            }
        }
    } else {
        // Fallback to default if not initialized
        Arc::new(Config::default())
    }
}

/// Get the current configuration, returning an error if lock is poisoned
pub fn try_get() -> BlixardResult<Arc<Config>> {
    if let Some(config) = CONFIG.get() {
        config
            .read()
            .map(|guard| Arc::new(guard.clone()))
            .map_err(|_| {
                BlixardError::ConfigError(
                    "Failed to read configuration (lock poisoned)".to_string(),
                )
            })
    } else {
        Err(BlixardError::ConfigError(
            "Configuration not initialized".to_string(),
        ))
    }
}

/// Subscribe to configuration changes
pub fn subscribe() -> watch::Receiver<Arc<Config>> {
    if let Some(tx) = CONFIG_WATCHER.get() {
        tx.subscribe()
    } else {
        // Create a dummy channel with default config
        let (_tx, rx) = watch::channel(Arc::new(Config::default()));
        rx
    }
}

/// Reload configuration from file (hot-reload)
pub async fn reload<P: AsRef<Path>>(config_path: P) -> BlixardResult<()> {
    let new_config = Config::from_file(config_path)?;

    // Update global config
    if let Some(config) = CONFIG.get() {
        let mut config_write = config.write().map_err(|_| {
            BlixardError::ConfigError(
                "Failed to acquire write lock on configuration (lock poisoned)".to_string(),
            )
        })?;

        // Apply only hot-reloadable changes
        // This is a simplified version - in production, you'd want more granular updates
        config_write.observability = new_config.observability.clone();
        config_write.cluster.peer.health_check_interval =
            new_config.cluster.peer.health_check_interval;
        config_write.cluster.peer.failure_threshold = new_config.cluster.peer.failure_threshold;
        config_write.cluster.worker.resource_update_interval =
            new_config.cluster.worker.resource_update_interval;
        config_write.vm.scheduler.default_strategy =
            new_config.vm.scheduler.default_strategy.clone();
        config_write.vm.scheduler.overcommit_ratio = new_config.vm.scheduler.overcommit_ratio;
        config_write.network.iroh.keepalive_interval = new_config.network.iroh.keepalive_interval;
        config_write.network.iroh.connection_timeout = new_config.network.iroh.connection_timeout;

        // Apply discovery configuration changes
        config_write.network.discovery.refresh_interval =
            new_config.network.discovery.refresh_interval;
        config_write.network.discovery.node_stale_timeout =
            new_config.network.discovery.node_stale_timeout;
        config_write.network.discovery.enable_static = new_config.network.discovery.enable_static;
        config_write.network.discovery.enable_dns = new_config.network.discovery.enable_dns;
        config_write.network.discovery.enable_mdns = new_config.network.discovery.enable_mdns;

        drop(config_write);

        // Notify watchers
        if let Some(tx) = CONFIG_WATCHER.get() {
            let _ = tx.send(Arc::new(new_config));
        }
    }

    Ok(())
}

/// Configuration builder for programmatic construction
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn node_id(mut self, id: u64) -> Self {
        self.config.node.id = Some(id);
        self
    }

    pub fn bind_address(mut self, addr: impl Into<String>) -> Self {
        self.config.node.bind_address = addr.into();
        self
    }

    pub fn data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.node.data_dir = dir.into();
        self
    }

    pub fn join_address(mut self, addr: impl Into<String>) -> Self {
        self.config.cluster.join_address = Some(addr.into());
        self
    }

    pub fn vm_backend(mut self, backend: impl Into<String>) -> Self {
        self.config.node.vm_backend = backend.into();
        self
    }

    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config.observability.logging.level = level.into();
        self
    }

    /// Security configuration methods
    pub fn security_tls_enabled(mut self, enabled: bool) -> Self {
        self.config.security.tls.enabled = enabled;
        self
    }

    pub fn security_tls_cert(mut self, cert_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.cert_file = Some(cert_file.into());
        self
    }

    pub fn security_tls_key(mut self, key_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.key_file = Some(key_file.into());
        self
    }

    pub fn security_tls_ca(mut self, ca_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.ca_file = Some(ca_file.into());
        self
    }

    pub fn security_tls_require_client_cert(mut self, require: bool) -> Self {
        self.config.security.tls.require_client_cert = require;
        self
    }

    pub fn security_auth_enabled(mut self, enabled: bool) -> Self {
        self.config.security.auth.enabled = enabled;
        self
    }

    pub fn security_auth_method(mut self, method: impl Into<String>) -> Self {
        self.config.security.auth.method = method.into();
        self
    }

    pub fn security_auth_token_file(mut self, token_file: impl Into<PathBuf>) -> Self {
        self.config.security.auth.token_file = Some(token_file.into());
        self
    }

    /// Discovery configuration methods
    pub fn discovery_enable_static(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_static = enabled;
        self
    }

    pub fn discovery_enable_dns(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_dns = enabled;
        self
    }

    pub fn discovery_enable_mdns(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_mdns = enabled;
        self
    }

    pub fn discovery_add_static_node(
        mut self,
        node_id: impl Into<String>,
        addresses: Vec<String>,
        name: Option<String>,
    ) -> Self {
        self.config
            .network
            .discovery
            .static_nodes
            .seeds
            .push(StaticNodeEntry {
                node_id: node_id.into(),
                addresses,
                name,
            });
        self
    }

    pub fn discovery_add_dns_domain(mut self, domain: impl Into<String>) -> Self {
        self.config
            .network
            .discovery
            .dns
            .domains
            .push(domain.into());
        self
    }

    pub fn discovery_mdns_service_name(mut self, service_name: impl Into<String>) -> Self {
        self.config.network.discovery.mdns.service_name = service_name.into();
        self
    }

    pub fn p2p_enabled(mut self, enabled: bool) -> Self {
        self.config.p2p.enabled = enabled;
        self
    }

    pub fn build(self) -> BlixardResult<Config> {
        self.config.validate()?;
        Ok(self.config)
    }
}
