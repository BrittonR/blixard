//! Timing configuration for simulation tests
//!
//! This module provides centralized timing configuration for all simulation tests,
//! making it easy to adjust timeouts and intervals to reduce flakiness.

use madsim::time::Duration;
use std::env;

/// Get the environment multiplier for timeouts
pub fn env_multiplier() -> u32 {
    if env::var("CI").is_ok() || env::var("GITHUB_ACTIONS").is_ok() {
        3 // 3x slower in CI environments
    } else if let Ok(mult) = env::var("TEST_TIMEOUT_MULTIPLIER") {
        mult.parse().unwrap_or(1)
    } else {
        1 // Normal speed for local development
    }
}

/// Apply environment multiplier to a duration
pub fn scaled(base: Duration) -> Duration {
    base * env_multiplier()
}

/// Timing constants for Raft tests
pub mod raft {
    use super::*;
    
    /// Minimum election timeout (before randomization)
    pub fn election_timeout_min() -> Duration {
        scaled(Duration::from_millis(150))
    }
    
    /// Maximum election timeout (before randomization)
    pub fn election_timeout_max() -> Duration {
        scaled(Duration::from_millis(300))
    }
    
    /// Heartbeat interval
    pub fn heartbeat_interval() -> Duration {
        scaled(Duration::from_millis(50))
    }
    
    /// Time to wait for initial leader election
    pub fn leader_election_timeout() -> Duration {
        scaled(Duration::from_secs(10))
    }
    
    /// Time to wait for leader election in partitioned scenarios
    pub fn partitioned_election_timeout() -> Duration {
        scaled(Duration::from_secs(15))
    }
    
    /// Time to wait for cluster convergence after healing
    pub fn convergence_timeout() -> Duration {
        scaled(Duration::from_secs(20))
    }
    
    /// Interval for checking convergence
    pub fn convergence_check_interval() -> Duration {
        scaled(Duration::from_millis(500))
    }
    
    /// Time to wait after creating a partition
    pub fn partition_stabilization_delay() -> Duration {
        scaled(Duration::from_secs(3))
    }
    
    /// Message processing loop interval
    pub fn message_loop_interval() -> Duration {
        Duration::from_millis(1) // Keep this fast even in CI
    }
    
    /// Tick loop interval
    pub fn tick_interval() -> Duration {
        Duration::from_millis(10) // Keep this fast even in CI
    }
}

/// Timing constants for gRPC tests
pub mod grpc {
    use super::*;
    
    /// Time to wait for gRPC server startup
    pub fn server_startup_timeout() -> Duration {
        scaled(Duration::from_secs(5))
    }
    
    /// Initial delay for connection retry
    pub fn connection_retry_initial_delay() -> Duration {
        Duration::from_millis(100)
    }
    
    /// Maximum delay for connection retry
    pub fn connection_retry_max_delay() -> Duration {
        Duration::from_secs(2)
    }
    
    /// Maximum number of connection attempts
    pub fn connection_max_attempts() -> u32 {
        10 * env_multiplier()
    }
    
    /// Time to wait between service startup
    pub fn service_startup_delay() -> Duration {
        scaled(Duration::from_millis(500))
    }
}

/// Timing constants for cluster tests
pub mod cluster {
    use super::*;
    
    /// Time to wait for database resource cleanup
    pub fn database_cleanup_delay() -> Duration {
        scaled(Duration::from_millis(500))
    }
    
    /// Overall test timeout
    pub fn test_timeout() -> Duration {
        scaled(Duration::from_secs(30))
    }
    
    /// Time to wait for cluster formation
    pub fn formation_timeout() -> Duration {
        scaled(Duration::from_secs(10))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scaling() {
        // In normal environment
        if env::var("CI").is_err() {
            assert_eq!(env_multiplier(), 1);
            assert_eq!(scaled(Duration::from_secs(1)), Duration::from_secs(1));
        }
    }
    
    #[test]
    fn test_raft_timeouts() {
        // Ensure election timeout is greater than heartbeat
        assert!(raft::election_timeout_min() > raft::heartbeat_interval() * 2);
    }
}