//! Network discovery configuration

use serde::{Deserialize, Serialize};

/// Discovery configuration for finding Iroh nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Enable static discovery from configuration
    pub enable_static: bool,

    /// Enable DNS-based discovery
    pub enable_dns: bool,

    /// Enable mDNS for local network discovery
    pub enable_mdns: bool,

    /// How often to refresh discovery information (in seconds)
    pub refresh_interval: u64,

    /// Maximum age before considering a node stale (in seconds)
    pub node_stale_timeout: u64,

    /// Static nodes configuration
    pub static_nodes: StaticNodesConfig,

    /// DNS discovery settings
    pub dns: DnsDiscoveryConfig,

    /// mDNS discovery settings
    pub mdns: MdnsDiscoveryConfig,
}

/// Static nodes configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StaticNodesConfig {
    /// List of static seed nodes
    pub seeds: Vec<StaticNodeEntry>,
}

/// A static node entry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StaticNodeEntry {
    /// Node ID (base32-encoded)
    pub node_id: String,

    /// Network addresses where the node can be reached
    pub addresses: Vec<String>,

    /// Optional human-readable name
    pub name: Option<String>,
}

/// DNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DnsDiscoveryConfig {
    /// DNS domains to query for SRV records
    pub domains: Vec<String>,

    /// DNS server to use (empty for system default)
    pub dns_server: Option<String>,

    /// Query timeout in seconds
    pub query_timeout: u64,
}

/// mDNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MdnsDiscoveryConfig {
    /// Service name to advertise and discover
    pub service_name: String,

    /// Enable IPv4 multicast
    pub enable_ipv4: bool,

    /// Enable IPv6 multicast
    pub enable_ipv6: bool,

    /// TTL for mDNS packets
    pub ttl: u32,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_static: true,
            enable_dns: false,
            enable_mdns: true,
            refresh_interval: 30,
            node_stale_timeout: 300, // 5 minutes
            static_nodes: StaticNodesConfig::default(),
            dns: DnsDiscoveryConfig::default(),
            mdns: MdnsDiscoveryConfig::default(),
        }
    }
}

impl Default for DnsDiscoveryConfig {
    fn default() -> Self {
        Self {
            domains: vec!["_iroh._tcp.blixard.local".to_string()],
            dns_server: None,
            query_timeout: 5,
        }
    }
}

impl Default for MdnsDiscoveryConfig {
    fn default() -> Self {
        Self {
            service_name: "_blixard-iroh._tcp.local".to_string(),
            enable_ipv4: true,
            enable_ipv6: true,
            ttl: 120,
        }
    }
}