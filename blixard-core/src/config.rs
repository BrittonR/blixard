//! Configuration constants and environment variables for Blixard
//!
//! This module centralizes all configurable constants, making them
//! easy to override via environment variables for different deployments.

use std::env;

/// Parse an environment variable as a typed value with a default fallback
fn env_var_or_default<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

/// Peer connection configuration
pub struct PeerConfig {
    /// Maximum number of concurrent peer connections
    pub max_connections: usize,
    /// Number of consecutive failures before marking peer as unhealthy
    pub failure_threshold: u32,
    /// Maximum number of messages to buffer per peer
    pub max_buffered_messages: usize,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Reconnection base delay in milliseconds
    pub reconnect_delay_ms: u64,
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            max_connections: env_var_or_default("BLIXARD_MAX_CONNECTIONS", 100),
            failure_threshold: env_var_or_default("BLIXARD_FAILURE_THRESHOLD", 5),
            max_buffered_messages: env_var_or_default("BLIXARD_MAX_BUFFERED_MESSAGES", 100),
            connection_timeout_ms: env_var_or_default("BLIXARD_CONNECTION_TIMEOUT_MS", 5000),
            reconnect_delay_ms: env_var_or_default("BLIXARD_RECONNECT_DELAY_MS", 1000),
        }
    }
}

/// Raft manager configuration
pub struct RaftConfig {
    /// Maximum number of Raft manager restart attempts
    pub max_restarts: u32,
    /// Base delay between restarts in milliseconds
    pub restart_delay_ms: u64,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            max_restarts: env_var_or_default("BLIXARD_RAFT_MAX_RESTARTS", 5),
            restart_delay_ms: env_var_or_default("BLIXARD_RAFT_RESTART_DELAY_MS", 1000),
        }
    }
}

/// Resource estimation configuration
pub struct ResourceConfig {
    /// Default memory allocation in MB when detection fails
    pub default_memory_mb: u64,
    /// Default disk space in GB when detection fails
    pub default_disk_gb: u64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            default_memory_mb: env_var_or_default("BLIXARD_DEFAULT_MEMORY_MB", 8192),
            default_disk_gb: env_var_or_default("BLIXARD_DEFAULT_DISK_GB", 100),
        }
    }
}

/// Task execution configuration
pub struct TaskConfig {
    /// Default task timeout in seconds
    pub default_timeout_secs: u64,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: env_var_or_default("BLIXARD_TASK_TIMEOUT_SECS", 300),
        }
    }
}

/// Worker capabilities when joining a cluster
pub struct WorkerDefaults {
    /// Default CPU cores for joining workers
    pub cpu_cores: u32,
    /// Default memory in MB for joining workers
    pub memory_mb: u64,
    /// Default disk in GB for joining workers
    pub disk_gb: u64,
}

impl Default for WorkerDefaults {
    fn default() -> Self {
        Self {
            cpu_cores: env_var_or_default("BLIXARD_DEFAULT_CPU_CORES", 4),
            memory_mb: env_var_or_default("BLIXARD_DEFAULT_WORKER_MEMORY_MB", 8192),
            disk_gb: env_var_or_default("BLIXARD_DEFAULT_WORKER_DISK_GB", 100),
        }
    }
}

/// Raft consensus timeouts and intervals
pub struct RaftTimeouts {
    /// Configuration change timeout in seconds
    pub conf_change_timeout_secs: u64,
    /// Proposal timeout in seconds
    pub proposal_timeout_secs: u64,
    /// Join request delay after initialization in milliseconds
    pub join_request_delay_ms: u64,
}

impl Default for RaftTimeouts {
    fn default() -> Self {
        Self {
            conf_change_timeout_secs: env_var_or_default("BLIXARD_CONF_CHANGE_TIMEOUT_SECS", 5),
            proposal_timeout_secs: env_var_or_default("BLIXARD_PROPOSAL_TIMEOUT_SECS", 10),
            join_request_delay_ms: env_var_or_default("BLIXARD_JOIN_REQUEST_DELAY_MS", 100),
        }
    }
}

/// Global configuration instance
pub struct Config {
    pub peer: PeerConfig,
    pub raft: RaftConfig,
    pub resource: ResourceConfig,
    pub task: TaskConfig,
    pub worker: WorkerDefaults,
    pub timeouts: RaftTimeouts,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            peer: PeerConfig::default(),
            raft: RaftConfig::default(),
            resource: ResourceConfig::default(),
            task: TaskConfig::default(),
            worker: WorkerDefaults::default(),
            timeouts: RaftTimeouts::default(),
        }
    }
}

/// Get the global configuration instance
pub fn get() -> Config {
    Config::default()
}

/// Initialize configuration by checking environment variables
/// This can be called early in main() to validate configuration
pub fn init() {
    // Simply create a config to trigger environment variable parsing
    let _ = get();
    
    // Log configuration sources for debugging
    if env::var("RUST_LOG").is_ok() || env::var("BLIXARD_LOG").is_ok() {
        tracing::debug!("Configuration initialized from environment");
    }
}