//! Default configuration values for Blixard
//!
//! This module centralizes all default values to make them easy to find and modify.

use std::time::Duration;

// Raft consensus defaults
pub const DEFAULT_RAFT_ELECTION_TICK: u32 = 10;
pub const DEFAULT_RAFT_HEARTBEAT_TICK: u32 = 3;
pub const DEFAULT_RAFT_MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB
pub const DEFAULT_RAFT_MAX_INFLIGHT_MSGS: usize = 256;
pub const DEFAULT_RAFT_TICK_INTERVAL_MS: u64 = 100;
pub const DEFAULT_RAFT_SNAPSHOT_THRESHOLD: u64 = 1000;
pub const DEFAULT_RAFT_SNAPSHOT_CATCHUP_ENTRIES: u64 = 500;

// Network defaults
pub const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:7000";
pub const DEFAULT_MAX_RPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB
pub const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 10;
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_MAX_CONNECTIONS: usize = 1000;

// VM configuration defaults
pub const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 30;
pub const DEFAULT_VM_STARTUP_TIMEOUT_SECS: u64 = 300; // 5 minutes
pub const DEFAULT_VM_SHUTDOWN_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_VM_RESTART_DELAY_SECS: u64 = 5;
pub const DEFAULT_MAX_VM_RESTART_ATTEMPTS: u32 = 3;
pub const DEFAULT_VM_CONSOLE_SOCKET_PREFIX: &str = "/tmp/blixard-vm-";

// Storage defaults
pub const DEFAULT_DATA_DIR: &str = "./data";
pub const DEFAULT_MAX_DB_SIZE: usize = 10 * 1024 * 1024 * 1024; // 10GB
pub const DEFAULT_COMPACTION_INTERVAL_SECS: u64 = 3600; // 1 hour
pub const DEFAULT_CACHE_SIZE: usize = 100 * 1024 * 1024; // 100MB
pub const DEFAULT_WAL_BUFFER_SIZE: usize = 4 * 1024 * 1024; // 4MB

// Monitoring defaults
pub const DEFAULT_METRICS_PORT: u16 = 9090;
pub const DEFAULT_HEALTH_CHECK_PORT: u16 = 8080;
pub const DEFAULT_LOG_LEVEL: &str = "info";
pub const DEFAULT_METRICS_INTERVAL_SECS: u64 = 10;
pub const DEFAULT_TRACE_SAMPLE_RATE: f64 = 0.01; // 1%

// Batch processing defaults
pub const DEFAULT_MAX_BATCH_SIZE: usize = 100;
pub const DEFAULT_BATCH_TIMEOUT_MS: u64 = 10;
pub const DEFAULT_MAX_BATCH_BYTES: usize = 1024 * 1024; // 1MB
pub const DEFAULT_BATCH_QUEUE_SIZE: usize = 1000;

// Retry defaults
pub const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 3;
pub const DEFAULT_RETRY_BASE_DELAY_MS: u64 = 100;
pub const DEFAULT_RETRY_MAX_DELAY_SECS: u64 = 30;
pub const DEFAULT_RETRY_MULTIPLIER: f64 = 2.0;

// Resource pool defaults
pub const DEFAULT_POOL_MIN_SIZE: usize = 0;
pub const DEFAULT_POOL_MAX_SIZE: usize = 10;
pub const DEFAULT_POOL_ACQUIRE_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_POOL_IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

// Helper functions for Duration creation
pub const fn duration_ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

pub const fn duration_secs(secs: u64) -> Duration {
    Duration::from_secs(secs)
}
