//! Raft consensus configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::error::{BlixardError, BlixardResult};
use super::defaults::*;
use super::{parse_duration_from_env, BatchConfig};

/// Raft consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RaftConfig {
    /// Number of ticks before triggering election
    pub election_tick: u32,
    
    /// Number of ticks between heartbeats
    pub heartbeat_tick: u32,
    
    /// Maximum size of a single Raft message
    pub max_message_size: usize,
    
    /// Maximum number of in-flight messages
    pub max_inflight_msgs: usize,
    
    /// Interval between Raft ticks
    #[serde(with = "humantime_serde")]
    pub tick_interval: Duration,
    
    /// Number of entries before triggering snapshot
    pub snapshot_threshold: u64,
    
    /// Number of entries to retain after snapshot
    pub snapshot_catchup_entries: u64,
    
    /// Enable pre-vote to reduce disruption
    pub pre_vote: bool,
    
    /// Check quorum on proposals
    pub check_quorum: bool,
    
    /// Batch processing configuration
    pub batch_processing: BatchConfig,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            election_tick: DEFAULT_RAFT_ELECTION_TICK,
            heartbeat_tick: DEFAULT_RAFT_HEARTBEAT_TICK,
            max_message_size: DEFAULT_RAFT_MAX_MESSAGE_SIZE,
            max_inflight_msgs: DEFAULT_RAFT_MAX_INFLIGHT_MSGS,
            tick_interval: duration_ms(DEFAULT_RAFT_TICK_INTERVAL_MS),
            snapshot_threshold: DEFAULT_RAFT_SNAPSHOT_THRESHOLD,
            snapshot_catchup_entries: DEFAULT_RAFT_SNAPSHOT_CATCHUP_ENTRIES,
            pre_vote: true,
            check_quorum: true,
            batch_processing: BatchConfig::default(),
        }
    }
}

impl RaftConfig {
    /// Load Raft configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_ELECTION_TICK") {
            config.election_tick = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_ELECTION_TICK".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_HEARTBEAT_TICK") {
            config.heartbeat_tick = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_HEARTBEAT_TICK".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_MAX_MESSAGE_SIZE") {
            config.max_message_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_MAX_MESSAGE_SIZE".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_MAX_INFLIGHT_MSGS") {
            config.max_inflight_msgs = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_MAX_INFLIGHT_MSGS".to_string())
            )?;
        }
        
        config.tick_interval = parse_duration_from_env(
            "BLIXARD_RAFT_TICK_INTERVAL_MS",
            config.tick_interval
        );
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_SNAPSHOT_THRESHOLD") {
            config.snapshot_threshold = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_SNAPSHOT_THRESHOLD".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_PRE_VOTE") {
            config.pre_vote = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_PRE_VOTE".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_RAFT_CHECK_QUORUM") {
            config.check_quorum = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_RAFT_CHECK_QUORUM".to_string())
            )?;
        }
        
        config.batch_processing = BatchConfig::from_env()?;
        
        Ok(config)
    }
    
    /// Validate Raft configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Election tick must be greater than heartbeat tick
        if self.election_tick <= self.heartbeat_tick {
            return Err(BlixardError::ConfigError(
                "election_tick must be greater than heartbeat_tick".to_string()
            ));
        }
        
        // Ensure reasonable message size
        if self.max_message_size > 100 * 1024 * 1024 { // 100MB
            return Err(BlixardError::ConfigError(
                "max_message_size too large (max 100MB)".to_string()
            ));
        }
        
        if self.max_message_size < 1024 { // 1KB
            return Err(BlixardError::ConfigError(
                "max_message_size too small (min 1KB)".to_string()
            ));
        }
        
        // Validate tick interval
        if self.tick_interval < Duration::from_millis(10) {
            return Err(BlixardError::ConfigError(
                "tick_interval too small (min 10ms)".to_string()
            ));
        }
        
        if self.tick_interval > Duration::from_secs(10) {
            return Err(BlixardError::ConfigError(
                "tick_interval too large (max 10s)".to_string()
            ));
        }
        
        // Validate snapshot configuration
        if self.snapshot_threshold < 100 {
            return Err(BlixardError::ConfigError(
                "snapshot_threshold too small (min 100)".to_string()
            ));
        }
        
        if self.snapshot_catchup_entries >= self.snapshot_threshold {
            return Err(BlixardError::ConfigError(
                "snapshot_catchup_entries must be less than snapshot_threshold".to_string()
            ));
        }
        
        self.batch_processing.validate()?;
        
        Ok(())
    }
    
    /// Convert to raft-rs Config
    pub fn to_raft_config(&self, id: u64) -> raft::Config {
        let mut cfg = raft::Config {
            id,
            election_tick: self.election_tick as usize,
            heartbeat_tick: self.heartbeat_tick as usize,
            max_size_per_msg: self.max_message_size as u64,
            max_inflight_msgs: self.max_inflight_msgs,
            pre_vote: self.pre_vote,
            check_quorum: self.check_quorum,
            ..Default::default()
        };
        
        cfg.validate().expect("Invalid Raft configuration");
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_raft_config_is_valid() {
        let config = RaftConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_election_tick() {
        let mut config = RaftConfig::default();
        config.election_tick = 2;
        config.heartbeat_tick = 3;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_raft_config_conversion() {
        let config = RaftConfig::default();
        let raft_cfg = config.to_raft_config(1);
        assert_eq!(raft_cfg.id, 1);
        assert_eq!(raft_cfg.election_tick, config.election_tick as usize);
    }
}