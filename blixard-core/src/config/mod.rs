//! Configuration management system with TOML support and hot-reload
//!
//! This module provides a comprehensive configuration system that:
//! - Loads from TOML files
//! - Supports environment variable overrides
//! - Validates configuration values
//! - Supports hot-reload for non-critical settings
//!
//! # Example Configuration
//!
//! ```toml
//! # Node configuration
//! [node]
//! id = 1
//! bind_address = "127.0.0.1:7001"
//! data_dir = "./data"
//! vm_backend = "microvm"
//! debug = false
//!
//! # Cluster configuration
//! [cluster]
//! bootstrap = false
//! join_address = "127.0.0.1:7002"
//!
//! # Network configuration
//! [network.iroh]
//! max_message_size = 67108864
//! connection_timeout = "10s"
//! keepalive_interval = "10s"
//! home_relay = "https://relay.iroh.network"
//! discovery_port = 0
//!
//! # Discovery configuration
//! [network.discovery]
//! enable_static = true
//! enable_dns = false
//! enable_mdns = true
//! refresh_interval = 30
//! node_stale_timeout = 300
//!
//! # Static seed nodes
//! [[network.discovery.static_nodes.seeds]]
//! node_id = "kyt7jjyhhg5xzlwtqwwgrkwhdtmqag6tg2nqwhz4o4nzqhj3a"
//! addresses = ["127.0.0.1:7002", "192.168.1.100:7002"]
//! name = "seed-node-1"
//!
//! [[network.discovery.static_nodes.seeds]]
//! node_id = "mln8jjyhhg5xzlwtqwwgrkwhdtmqag6tg2nqwhz4o4nzqhj3b"
//! addresses = ["10.0.0.5:7002"]
//! name = "seed-node-2"
//!
//! # DNS discovery
//! [network.discovery.dns]
//! domains = ["_iroh._tcp.blixard.local", "_blixard._tcp.example.com"]
//! dns_server = "8.8.8.8:53"  # Optional, uses system DNS if not specified
//! query_timeout = 5
//!
//! # mDNS discovery
//! [network.discovery.mdns]
//! service_name = "_blixard-iroh._tcp.local"
//! enable_ipv4 = true
//! enable_ipv6 = true
//! ttl = 120
//!
//! # Security configuration
//! [security.tls]
//! enabled = true
//! cert_file = "/path/to/cert.pem"
//! key_file = "/path/to/key.pem"
//! ca_file = "/path/to/ca.pem"
//! require_client_cert = true
//!
//! [security.auth]
//! enabled = true
//! method = "token"
//! token_file = "/path/to/tokens.json"
//! ```

pub mod cluster;
pub mod core;
pub mod discovery;
pub mod network;
pub mod observability;
pub mod p2p;
pub mod security;
pub mod storage;
pub mod vm;

pub use cluster::{ClusterConfig, PeerConfig, RaftConfig, RaftBatchConfig, WorkerConfig, ConnectionPoolConfig};
pub use core::{Config, NodeConfig};
pub use discovery::{DiscoveryConfig, StaticNodesConfig, StaticNodeEntry, DnsDiscoveryConfig, MdnsDiscoveryConfig};
pub use network::{NetworkConfig, IrohTransportConfig, MetricsServerConfig};
pub use observability::{ObservabilityConfig, LoggingConfig, LogRotationConfig, MetricsConfig, TracingConfig};
pub use p2p::{P2pConfig, ImageCacheConfig};
pub use security::{SecurityConfig, TlsConfig, AuthConfig};
pub use storage::{StorageConfig, BackupConfig};
pub use vm::{VmConfig, VmDefaults, SchedulerConfig, ResourceLimits};

// Re-export global management functions
pub use core::{init, get, try_get, subscribe, reload};

// Re-export builder
pub use core::ConfigBuilder;