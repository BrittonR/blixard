//! P2P configuration

use serde::{Deserialize, Serialize};

/// P2P configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct P2pConfig {
    /// Enable P2P features
    pub enabled: bool,

    /// Enable VM image sharing via P2P
    pub enable_image_sharing: bool,

    /// Enable log aggregation via P2P
    pub enable_log_aggregation: bool,

    /// P2P port (in addition to gRPC port)
    pub port: u16,

    /// Relay servers for NAT traversal
    pub relay_servers: Vec<String>,

    /// Image cache configuration
    pub image_cache: ImageCacheConfig,
}

/// P2P image cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageCacheConfig {
    /// Maximum cache size in GB
    pub max_size_gb: u64,

    /// Cache eviction policy (lru, lfu, fifo)
    pub eviction_policy: String,

    /// Prefetch popular images
    pub enable_prefetch: bool,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enable_image_sharing: true,
            enable_log_aggregation: false,
            port: 7002,
            relay_servers: vec![],
            image_cache: ImageCacheConfig::default(),
        }
    }
}

impl Default for ImageCacheConfig {
    fn default() -> Self {
        Self {
            max_size_gb: 10,
            eviction_policy: "lru".to_string(),
            enable_prefetch: false,
        }
    }
}