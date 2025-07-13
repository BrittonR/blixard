//! Cluster and Raft configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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