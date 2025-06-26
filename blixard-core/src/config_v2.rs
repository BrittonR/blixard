//! Configuration management system with TOML support and hot-reload
//!
//! This module provides a comprehensive configuration system that:
//! - Loads from TOML files
//! - Supports environment variable overrides
//! - Validates configuration values
//! - Supports hot-reload for non-critical settings

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use crate::error::{BlixardError, BlixardResult};

/// Global configuration instance
static CONFIG: once_cell::sync::OnceCell<Arc<RwLock<Config>>> = once_cell::sync::OnceCell::new();

/// Configuration watcher for hot-reload
static CONFIG_WATCHER: once_cell::sync::OnceCell<watch::Sender<Arc<Config>>> = once_cell::sync::OnceCell::new();

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
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// Node ID (can be overridden by CLI)
    pub id: Option<u64>,
    
    /// Bind address for gRPC server
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
    
    /// Maximum restart attempts
    pub max_restarts: u32,
    
    /// Restart base delay
    #[serde(with = "humantime_serde")]
    pub restart_delay: Duration,
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
    /// gRPC server settings
    pub grpc: GrpcConfig,
    
    /// HTTP metrics server settings
    pub metrics: MetricsServerConfig,
}

/// gRPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcConfig {
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Keep-alive interval
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,
    
    /// Keep-alive timeout
    #[serde(with = "humantime_serde")]
    pub keepalive_timeout: Duration,
    
    /// Enable TCP nodelay
    pub tcp_nodelay: bool,
    
    /// Number of worker threads
    pub worker_threads: Option<usize>,
}

/// Metrics server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsServerConfig {
    /// Enable metrics server
    pub enabled: bool,
    
    /// Port offset from gRPC port
    pub port_offset: u16,
    
    /// Metrics path
    pub path: String,
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
            grpc: GrpcConfig::default(),
            metrics: MetricsServerConfig::default(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            max_message_size: 64 * 1024 * 1024, // 64MB
            keepalive_interval: Duration::from_secs(10),
            keepalive_timeout: Duration::from_secs(10),
            tcp_nodelay: true,
            worker_threads: None,
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
    }
    
    /// Validate configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate bind address
        if self.node.bind_address.is_empty() {
            return Err(BlixardError::ConfigError("Bind address cannot be empty".to_string()));
        }
        
        // Validate data directory
        if self.node.data_dir.as_os_str().is_empty() {
            return Err(BlixardError::ConfigError("Data directory cannot be empty".to_string()));
        }
        
        // Validate Raft configuration
        if self.cluster.raft.heartbeat_tick >= self.cluster.raft.election_tick {
            return Err(BlixardError::ConfigError(
                "Heartbeat tick must be less than election tick".to_string()
            ));
        }
        
        // Validate resource limits
        if self.vm.scheduler.overcommit_ratio < 1.0 {
            return Err(BlixardError::ConfigError(
                "Overcommit ratio must be >= 1.0".to_string()
            ));
        }
        
        // Validate log level
        match self.observability.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {},
            _ => return Err(BlixardError::ConfigError(
                format!("Invalid log level: {}", self.observability.logging.level)
            )),
        }
        
        Ok(())
    }
    
    /// Check if a setting is hot-reloadable
    pub fn is_hot_reloadable(key: &str) -> bool {
        match key {
            // Hot-reloadable settings
            "observability.logging.level" |
            "observability.logging.format" |
            "cluster.peer.health_check_interval" |
            "cluster.peer.failure_threshold" |
            "cluster.worker.resource_update_interval" |
            "vm.scheduler.default_strategy" |
            "vm.scheduler.overcommit_ratio" |
            "network.grpc.keepalive_interval" |
            "network.grpc.keepalive_timeout" => true,
            
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
    CONFIG_WATCHER.set(tx).map_err(|_| {
        BlixardError::ConfigError("Configuration already initialized".to_string())
    })?;
    
    // Set global config
    CONFIG.set(Arc::new(RwLock::new((*config_arc).clone()))).map_err(|_| {
        BlixardError::ConfigError("Configuration already initialized".to_string())
    })?;
    
    Ok(())
}

/// Get the current configuration
pub fn get() -> Arc<Config> {
    if let Some(config) = CONFIG.get() {
        Arc::new(config.read().unwrap().clone())
    } else {
        // Fallback to default if not initialized
        Arc::new(Config::default())
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
        let mut config_write = config.write().unwrap();
        
        // Apply only hot-reloadable changes
        // This is a simplified version - in production, you'd want more granular updates
        config_write.observability = new_config.observability.clone();
        config_write.cluster.peer.health_check_interval = new_config.cluster.peer.health_check_interval;
        config_write.cluster.peer.failure_threshold = new_config.cluster.peer.failure_threshold;
        
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
    
    pub fn build(self) -> BlixardResult<Config> {
        self.config.validate()?;
        Ok(self.config)
    }
}