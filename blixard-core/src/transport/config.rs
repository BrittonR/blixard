//! Transport configuration types
//!
//! Configuration for Iroh P2P transport.

use serde::{Deserialize, Serialize};

/// Transport configuration (Iroh only)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransportConfig {
    /// Home relay server URL
    #[serde(default = "default_home_relay")]
    pub home_relay: String,

    /// Discovery port (0 for random)
    #[serde(default)]
    pub discovery_port: u16,

    /// Custom ALPN protocols
    #[serde(default)]
    pub alpn_protocols: Vec<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            home_relay: default_home_relay(),
            discovery_port: 0,
            alpn_protocols: vec![],
        }
    }
}

fn default_home_relay() -> String {
    "https://relay.iroh.network".to_string()
}

/// Iroh configuration (kept for compatibility)
pub type IrohConfig = TransportConfig;
