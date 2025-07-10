//! Comprehensive configuration module for Blixard
//!
//! This module provides a structured configuration system with sensible defaults,
//! environment variable support, and runtime validation.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use crate::error::{BlixardError, BlixardResult};

pub mod defaults;
pub mod raft;
pub mod network;
pub mod vm;
pub mod storage;
pub mod monitoring;
pub mod batch;

pub use defaults::*;
pub use raft::RaftConfig;
pub use network::NetworkConfig;
pub use vm::VmConfig;
pub use storage::StorageConfig;
pub use monitoring::MonitoringConfig;
pub use batch::BatchConfig;

/// Root configuration structure for Blixard
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BlixardConfig {
    /// Raft consensus configuration
    pub raft: RaftConfig,
    
    /// Network and transport configuration
    pub network: NetworkConfig,
    
    /// Virtual machine configuration
    pub vm: VmConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Monitoring and observability configuration
    pub monitoring: MonitoringConfig,
    
    /// Batch processing configuration
    pub batch: BatchConfig,
    
    /// Node identification
    pub node_id: u64,
    
    /// Cluster name
    pub cluster_name: String,
}

impl Default for BlixardConfig {
    fn default() -> Self {
        Self {
            raft: RaftConfig::default(),
            network: NetworkConfig::default(),
            vm: VmConfig::default(),
            storage: StorageConfig::default(),
            monitoring: MonitoringConfig::default(),
            batch: BatchConfig::default(),
            node_id: 0,
            cluster_name: "default".to_string(),
        }
    }
}

impl BlixardConfig {
    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Load configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();
        
        // Node configuration
        if let Ok(id) = std::env::var("BLIXARD_NODE_ID") {
            config.node_id = id.parse().map_err(|_| BlixardError::ConfigError(
                "Invalid BLIXARD_NODE_ID".to_string()
            ))?;
        }
        
        if let Ok(name) = std::env::var("BLIXARD_CLUSTER_NAME") {
            config.cluster_name = name;
        }
        
        // Load sub-configurations from environment
        config.raft = RaftConfig::from_env()?;
        config.network = NetworkConfig::from_env()?;
        config.vm = VmConfig::from_env()?;
        config.storage = StorageConfig::from_env()?;
        config.monitoring = MonitoringConfig::from_env()?;
        config.batch = BatchConfig::from_env()?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate node ID
        if self.node_id == 0 {
            return Err(BlixardError::ConfigError(
                "node_id must be non-zero".to_string()
            ));
        }
        
        // Validate sub-configurations
        self.raft.validate()?;
        self.network.validate()?;
        self.vm.validate()?;
        self.storage.validate()?;
        self.monitoring.validate()?;
        self.batch.validate()?;
        
        Ok(())
    }
    
    /// Create a test configuration with minimal settings
    pub fn test() -> Self {
        let mut config = Self::default();
        config.node_id = 1;
        config.cluster_name = "test".to_string();
        config.storage.data_dir = PathBuf::from("/tmp/blixard-test");
        config.network.bind_address = "127.0.0.1:0".to_string();
        config
    }
}

/// Builder for BlixardConfig
pub struct BlixardConfigBuilder {
    config: BlixardConfig,
}

impl BlixardConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: BlixardConfig::default(),
        }
    }
    
    pub fn node_id(mut self, id: u64) -> Self {
        self.config.node_id = id;
        self
    }
    
    pub fn cluster_name(mut self, name: impl Into<String>) -> Self {
        self.config.cluster_name = name.into();
        self
    }
    
    pub fn raft(mut self, raft: RaftConfig) -> Self {
        self.config.raft = raft;
        self
    }
    
    pub fn network(mut self, network: NetworkConfig) -> Self {
        self.config.network = network;
        self
    }
    
    pub fn vm(mut self, vm: VmConfig) -> Self {
        self.config.vm = vm;
        self
    }
    
    pub fn storage(mut self, storage: StorageConfig) -> Self {
        self.config.storage = storage;
        self
    }
    
    pub fn monitoring(mut self, monitoring: MonitoringConfig) -> Self {
        self.config.monitoring = monitoring;
        self
    }
    
    pub fn batch(mut self, batch: BatchConfig) -> Self {
        self.config.batch = batch;
        self
    }
    
    pub fn build(self) -> BlixardResult<BlixardConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for BlixardConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for duration parsing
pub(crate) fn parse_duration_from_env(key: &str, default: Duration) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(default)
}

pub(crate) fn parse_duration_secs_from_env(key: &str, default: Duration) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config_validation() {
        let mut config = BlixardConfig::default();
        config.node_id = 1; // Set required field
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_builder() {
        let config = BlixardConfigBuilder::new()
            .node_id(42)
            .cluster_name("test-cluster")
            .build()
            .unwrap();
        
        assert_eq!(config.node_id, 42);
        assert_eq!(config.cluster_name, "test-cluster");
    }
    
    #[test]
    fn test_invalid_config() {
        let config = BlixardConfig::default(); // node_id is 0
        assert!(config.validate().is_err());
    }
}