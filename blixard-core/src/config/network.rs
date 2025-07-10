//! Network and transport configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::error::{BlixardError, BlixardResult};
use super::defaults::*;
use super::parse_duration_secs_from_env;

/// Network and transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Address to bind the server to
    pub bind_address: String,
    
    /// Address to advertise to other nodes (if different from bind)
    pub advertise_address: Option<String>,
    
    /// Maximum RPC message size
    pub max_rpc_message_size: usize,
    
    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connection_timeout: Duration,
    
    /// Keepalive interval for connections
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,
    
    /// Request timeout
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,
    
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    
    /// Enable TCP nodelay
    pub tcp_nodelay: bool,
    
    /// Enable SO_REUSEADDR
    pub reuse_address: bool,
    
    /// Iroh P2P specific settings
    pub iroh: IrohConfig,
}

/// Iroh P2P specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IrohConfig {
    /// Enable Iroh P2P transport
    pub enabled: bool,
    
    /// Maximum frame size for Iroh messages
    pub max_frame_size: usize,
    
    /// Number of worker threads for Iroh
    pub worker_threads: Option<usize>,
    
    /// Enable QUIC transport
    pub enable_quic: bool,
    
    /// Enable relay connections
    pub enable_relay: bool,
    
    /// Relay server URLs
    pub relay_urls: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: DEFAULT_BIND_ADDRESS.to_string(),
            advertise_address: None,
            max_rpc_message_size: DEFAULT_MAX_RPC_MESSAGE_SIZE,
            connection_timeout: duration_secs(DEFAULT_CONNECTION_TIMEOUT_SECS),
            keepalive_interval: duration_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS),
            request_timeout: duration_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            max_connections: DEFAULT_MAX_CONNECTIONS,
            tcp_nodelay: true,
            reuse_address: true,
            iroh: IrohConfig::default(),
        }
    }
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_frame_size: DEFAULT_MAX_RPC_MESSAGE_SIZE,
            worker_threads: None, // Use default from runtime
            enable_quic: true,
            enable_relay: true,
            relay_urls: vec![
                "https://relay.iroh.network/".to_string(),
            ],
        }
    }
}

impl NetworkConfig {
    /// Load network configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();
        
        if let Ok(addr) = std::env::var("BLIXARD_BIND_ADDRESS") {
            config.bind_address = addr;
        }
        
        if let Ok(addr) = std::env::var("BLIXARD_ADVERTISE_ADDRESS") {
            config.advertise_address = Some(addr);
        }
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_RPC_MESSAGE_SIZE") {
            config.max_rpc_message_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_RPC_MESSAGE_SIZE".to_string())
            )?;
        }
        
        config.connection_timeout = parse_duration_secs_from_env(
            "BLIXARD_CONNECTION_TIMEOUT_SECS",
            config.connection_timeout
        );
        
        config.keepalive_interval = parse_duration_secs_from_env(
            "BLIXARD_KEEPALIVE_INTERVAL_SECS",
            config.keepalive_interval
        );
        
        config.request_timeout = parse_duration_secs_from_env(
            "BLIXARD_REQUEST_TIMEOUT_SECS",
            config.request_timeout
        );
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_CONNECTIONS") {
            config.max_connections = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_CONNECTIONS".to_string())
            )?;
        }
        
        // Iroh configuration
        if let Ok(val) = std::env::var("BLIXARD_IROH_ENABLED") {
            config.iroh.enabled = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_IROH_ENABLED".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_IROH_WORKER_THREADS") {
            config.iroh.worker_threads = Some(val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_IROH_WORKER_THREADS".to_string())
            )?);
        }
        
        Ok(config)
    }
    
    /// Validate network configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate bind address format
        if self.bind_address.is_empty() {
            return Err(BlixardError::ConfigError(
                "bind_address cannot be empty".to_string()
            ));
        }
        
        // Validate message size
        if self.max_rpc_message_size < 1024 { // 1KB
            return Err(BlixardError::ConfigError(
                "max_rpc_message_size too small (min 1KB)".to_string()
            ));
        }
        
        if self.max_rpc_message_size > 1024 * 1024 * 1024 { // 1GB
            return Err(BlixardError::ConfigError(
                "max_rpc_message_size too large (max 1GB)".to_string()
            ));
        }
        
        // Validate timeouts
        if self.connection_timeout < Duration::from_secs(1) {
            return Err(BlixardError::ConfigError(
                "connection_timeout too small (min 1s)".to_string()
            ));
        }
        
        if self.keepalive_interval < Duration::from_secs(1) {
            return Err(BlixardError::ConfigError(
                "keepalive_interval too small (min 1s)".to_string()
            ));
        }
        
        // Validate max connections
        if self.max_connections < 10 {
            return Err(BlixardError::ConfigError(
                "max_connections too small (min 10)".to_string()
            ));
        }
        
        // Validate Iroh config
        if self.iroh.enabled {
            if let Some(threads) = self.iroh.worker_threads {
                if threads == 0 || threads > 256 {
                    return Err(BlixardError::ConfigError(
                        "iroh.worker_threads must be between 1 and 256".to_string()
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get the effective advertise address
    pub fn advertise_address(&self) -> &str {
        self.advertise_address
            .as_deref()
            .unwrap_or(&self.bind_address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_network_config_is_valid() {
        let config = NetworkConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_advertise_address_fallback() {
        let config = NetworkConfig::default();
        assert_eq!(config.advertise_address(), config.bind_address);
        
        let mut config = NetworkConfig::default();
        config.advertise_address = Some("10.0.0.1:7000".to_string());
        assert_eq!(config.advertise_address(), "10.0.0.1:7000");
    }
}