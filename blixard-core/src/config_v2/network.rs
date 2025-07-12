//! Network and transport configuration

use super::DiscoveryConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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