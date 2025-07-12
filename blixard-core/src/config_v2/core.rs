//! Core configuration structures and global management

use crate::common::file_io::read_config_file;
use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;

use super::{
    ClusterConfig, NetworkConfig, ObservabilityConfig, P2pConfig, SecurityConfig, StorageConfig,
    VmConfig,
};

/// Global configuration instance
static CONFIG: once_cell::sync::OnceCell<Arc<RwLock<Config>>> = once_cell::sync::OnceCell::new();

/// Configuration watcher for hot-reload
static CONFIG_WATCHER: once_cell::sync::OnceCell<watch::Sender<Arc<Config>>> =
    once_cell::sync::OnceCell::new();

/// Complete configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Node configuration
    pub node: NodeConfig,

    /// Cluster configuration
    pub cluster: ClusterConfig,

    /// Storage configuration
    pub storage: StorageConfig,

    /// Network configuration
    pub network: NetworkConfig,

    /// VM management configuration
    pub vm: VmConfig,

    /// Observability configuration
    pub observability: ObservabilityConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// P2P configuration
    pub p2p: P2pConfig,

    /// Transport configuration
    pub transport: Option<crate::transport::config::TransportConfig>,
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// Node ID (can be overridden by CLI)
    pub id: Option<u64>,

    /// Bind address for Iroh P2P node
    pub bind_address: String,

    /// Data directory for storage
    pub data_dir: PathBuf,

    /// VM backend type
    pub vm_backend: String,

    /// Enable debug mode
    pub debug: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            cluster: ClusterConfig::default(),
            storage: StorageConfig::default(),
            network: NetworkConfig::default(),
            vm: VmConfig::default(),
            observability: ObservabilityConfig::default(),
            security: SecurityConfig::default(),
            p2p: P2pConfig::default(),
            transport: Some(crate::transport::config::TransportConfig::default()),
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            id: None,
            bind_address: "127.0.0.1:7001".to_string(),
            data_dir: PathBuf::from("./data"),
            vm_backend: "mock".to_string(),
            debug: false,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> BlixardResult<Self> {
        // Use blocking read since this is called from sync context
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| BlixardError::ConfigError("No tokio runtime available".to_string()))?;

        let config = runtime.block_on(async { Self::from_file_async(path).await })?;

        Ok(config)
    }

    /// Load configuration from a TOML file (async version)
    pub async fn from_file_async<P: AsRef<Path>>(path: P) -> BlixardResult<Self> {
        let mut config: Config = read_config_file(path, "blixard").await?;

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        // Node configuration
        if let Ok(id) = std::env::var("BLIXARD_NODE_ID") {
            if let Ok(id) = id.parse() {
                self.node.id = Some(id);
            }
        }
        if let Ok(addr) = std::env::var("BLIXARD_BIND_ADDRESS") {
            self.node.bind_address = addr;
        }
        if let Ok(dir) = std::env::var("BLIXARD_DATA_DIR") {
            self.node.data_dir = PathBuf::from(dir);
        }

        // Cluster configuration
        if let Ok(addr) = std::env::var("BLIXARD_JOIN_ADDRESS") {
            self.cluster.join_address = Some(addr);
        }

        // Observability
        if let Ok(level) = std::env::var("BLIXARD_LOG_LEVEL") {
            self.observability.logging.level = level;
        }
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            self.observability.tracing.enabled = true;
            self.observability.tracing.otlp_endpoint = Some(endpoint);
        }

        // Security configuration
        if let Ok(enabled) = std::env::var("BLIXARD_TLS_ENABLED") {
            if let Ok(enabled) = enabled.parse() {
                self.security.tls.enabled = enabled;
            }
        }
        if let Ok(cert_file) = std::env::var("BLIXARD_TLS_CERT_FILE") {
            self.security.tls.cert_file = Some(PathBuf::from(cert_file));
        }
        if let Ok(key_file) = std::env::var("BLIXARD_TLS_KEY_FILE") {
            self.security.tls.key_file = Some(PathBuf::from(key_file));
        }
        if let Ok(ca_file) = std::env::var("BLIXARD_TLS_CA_FILE") {
            self.security.tls.ca_file = Some(PathBuf::from(ca_file));
        }
        if let Ok(require) = std::env::var("BLIXARD_TLS_REQUIRE_CLIENT_CERT") {
            if let Ok(require) = require.parse() {
                self.security.tls.require_client_cert = require;
            }
        }

        if let Ok(enabled) = std::env::var("BLIXARD_AUTH_ENABLED") {
            if let Ok(enabled) = enabled.parse() {
                self.security.auth.enabled = enabled;
            }
        }
        if let Ok(method) = std::env::var("BLIXARD_AUTH_METHOD") {
            self.security.auth.method = method;
        }
        if let Ok(token_file) = std::env::var("BLIXARD_AUTH_TOKEN_FILE") {
            self.security.auth.token_file = Some(PathBuf::from(token_file));
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate bind address
        if self.node.bind_address.is_empty() {
            return Err(BlixardError::ConfigError(
                "Bind address cannot be empty".to_string(),
            ));
        }

        // Validate data directory
        if self.node.data_dir.as_os_str().is_empty() {
            return Err(BlixardError::ConfigError(
                "Data directory cannot be empty".to_string(),
            ));
        }

        // Validate Raft configuration
        if self.cluster.raft.heartbeat_tick >= self.cluster.raft.election_tick {
            return Err(BlixardError::ConfigError(
                "Heartbeat tick must be less than election tick".to_string(),
            ));
        }

        // Validate resource limits
        if self.vm.scheduler.overcommit_ratio < 1.0 {
            return Err(BlixardError::ConfigError(
                "Overcommit ratio must be >= 1.0".to_string(),
            ));
        }

        // Validate log level
        match self.observability.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            _ => {
                return Err(BlixardError::ConfigError(format!(
                    "Invalid log level: {}",
                    self.observability.logging.level
                )))
            }
        }

        // Validate security configuration
        if self.security.tls.enabled {
            // If TLS is enabled, certificate and key files should be specified
            if self.security.tls.cert_file.is_none() || self.security.tls.key_file.is_none() {
                return Err(BlixardError::ConfigError(
                    "TLS enabled but cert_file or key_file not specified".to_string(),
                ));
            }
        }

        // Validate authentication method
        if self.security.auth.enabled {
            match self.security.auth.method.as_str() {
                "token" | "mtls" => {}
                _ => {
                    return Err(BlixardError::ConfigError(format!(
                        "Invalid authentication method: {}",
                        self.security.auth.method
                    )))
                }
            }

            // If using token auth, warn if no token file is specified
            if self.security.auth.method == "token" && self.security.auth.token_file.is_none() {
                // This is not an error since tokens can be generated dynamically,
                // but in production you'd typically want a token file
            }
        }

        // Validate discovery configuration
        if self.network.discovery.refresh_interval == 0 {
            return Err(BlixardError::ConfigError(
                "Discovery refresh interval must be greater than 0".to_string(),
            ));
        }

        if self.network.discovery.node_stale_timeout == 0 {
            return Err(BlixardError::ConfigError(
                "Discovery node stale timeout must be greater than 0".to_string(),
            ));
        }

        // Validate static node entries
        for (i, node) in self.network.discovery.static_nodes.seeds.iter().enumerate() {
            if node.node_id.is_empty() {
                return Err(BlixardError::ConfigError(format!(
                    "Static node {} has empty node_id",
                    i
                )));
            }
            if node.addresses.is_empty() {
                return Err(BlixardError::ConfigError(format!(
                    "Static node {} has no addresses",
                    i
                )));
            }
        }

        // Validate DNS configuration
        if self.network.discovery.enable_dns && self.network.discovery.dns.domains.is_empty() {
            return Err(BlixardError::ConfigError(
                "DNS discovery enabled but no domains configured".to_string(),
            ));
        }

        // Validate mDNS configuration
        if self.network.discovery.enable_mdns && self.network.discovery.mdns.service_name.is_empty()
        {
            return Err(BlixardError::ConfigError(
                "mDNS discovery enabled but service name is empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if a setting is hot-reloadable
    pub fn is_hot_reloadable(key: &str) -> bool {
        match key {
            // Hot-reloadable settings
            "observability.logging.level"
            | "observability.logging.format"
            | "cluster.peer.health_check_interval"
            | "cluster.peer.failure_threshold"
            | "cluster.worker.resource_update_interval"
            | "vm.scheduler.default_strategy"
            | "vm.scheduler.overcommit_ratio"
            | "network.iroh.keepalive_interval"
            | "network.iroh.connection_timeout"
            | "network.discovery.refresh_interval"
            | "network.discovery.node_stale_timeout"
            | "network.discovery.enable_static"
            | "network.discovery.enable_dns"
            | "network.discovery.enable_mdns" => true,

            // Non-reloadable settings (require restart)
            _ => false,
        }
    }
}

/// Initialize the global configuration
pub fn init<P: AsRef<Path>>(config_path: P) -> BlixardResult<()> {
    let config = Config::from_file(config_path)?;
    let config_arc = Arc::new(config);

    // Initialize watcher channel
    let (tx, _rx) = watch::channel(config_arc.clone());
    CONFIG_WATCHER
        .set(tx)
        .map_err(|_| BlixardError::ConfigError("Configuration already initialized".to_string()))?;

    // Set global config
    CONFIG
        .set(Arc::new(RwLock::new((*config_arc).clone())))
        .map_err(|_| BlixardError::ConfigError("Configuration already initialized".to_string()))?;

    Ok(())
}

/// Get the current configuration
pub fn get() -> Arc<Config> {
    if let Some(config) = CONFIG.get() {
        match config.read() {
            Ok(guard) => Arc::new(guard.clone()),
            Err(_) => {
                // Lock is poisoned, fallback to default
                tracing::error!("Configuration lock is poisoned, returning default config");
                Arc::new(Config::default())
            }
        }
    } else {
        // Fallback to default if not initialized
        Arc::new(Config::default())
    }
}

/// Get the current configuration, returning an error if lock is poisoned
pub fn try_get() -> BlixardResult<Arc<Config>> {
    if let Some(config) = CONFIG.get() {
        config
            .read()
            .map(|guard| Arc::new(guard.clone()))
            .map_err(|_| {
                BlixardError::ConfigError(
                    "Failed to read configuration (lock poisoned)".to_string(),
                )
            })
    } else {
        Err(BlixardError::ConfigError(
            "Configuration not initialized".to_string(),
        ))
    }
}

/// Subscribe to configuration changes
pub fn subscribe() -> watch::Receiver<Arc<Config>> {
    if let Some(tx) = CONFIG_WATCHER.get() {
        tx.subscribe()
    } else {
        // Create a dummy channel with default config
        let (_tx, rx) = watch::channel(Arc::new(Config::default()));
        rx
    }
}

/// Reload configuration from file (hot-reload)
pub async fn reload<P: AsRef<Path>>(config_path: P) -> BlixardResult<()> {
    let new_config = Config::from_file(config_path)?;

    // Update global config
    if let Some(config) = CONFIG.get() {
        let mut config_write = config.write().map_err(|_| {
            BlixardError::ConfigError(
                "Failed to acquire write lock on configuration (lock poisoned)".to_string(),
            )
        })?;

        // Apply only hot-reloadable changes
        // This is a simplified version - in production, you'd want more granular updates
        config_write.observability = new_config.observability.clone();
        config_write.cluster.peer.health_check_interval =
            new_config.cluster.peer.health_check_interval;
        config_write.cluster.peer.failure_threshold = new_config.cluster.peer.failure_threshold;
        config_write.cluster.worker.resource_update_interval =
            new_config.cluster.worker.resource_update_interval;
        config_write.vm.scheduler.default_strategy =
            new_config.vm.scheduler.default_strategy.clone();
        config_write.vm.scheduler.overcommit_ratio = new_config.vm.scheduler.overcommit_ratio;
        config_write.network.iroh.keepalive_interval = new_config.network.iroh.keepalive_interval;
        config_write.network.iroh.connection_timeout = new_config.network.iroh.connection_timeout;

        // Apply discovery configuration changes
        config_write.network.discovery.refresh_interval =
            new_config.network.discovery.refresh_interval;
        config_write.network.discovery.node_stale_timeout =
            new_config.network.discovery.node_stale_timeout;
        config_write.network.discovery.enable_static = new_config.network.discovery.enable_static;
        config_write.network.discovery.enable_dns = new_config.network.discovery.enable_dns;
        config_write.network.discovery.enable_mdns = new_config.network.discovery.enable_mdns;

        drop(config_write);

        // Notify watchers
        if let Some(tx) = CONFIG_WATCHER.get() {
            let _ = tx.send(Arc::new(new_config));
        }
    }

    Ok(())
}

/// Configuration builder for programmatic construction
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn node_id(mut self, id: u64) -> Self {
        self.config.node.id = Some(id);
        self
    }

    pub fn bind_address(mut self, addr: impl Into<String>) -> Self {
        self.config.node.bind_address = addr.into();
        self
    }

    pub fn data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.node.data_dir = dir.into();
        self
    }

    pub fn join_address(mut self, addr: impl Into<String>) -> Self {
        self.config.cluster.join_address = Some(addr.into());
        self
    }

    pub fn vm_backend(mut self, backend: impl Into<String>) -> Self {
        self.config.node.vm_backend = backend.into();
        self
    }

    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config.observability.logging.level = level.into();
        self
    }

    /// Security configuration methods
    pub fn security_tls_enabled(mut self, enabled: bool) -> Self {
        self.config.security.tls.enabled = enabled;
        self
    }

    pub fn security_tls_cert(mut self, cert_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.cert_file = Some(cert_file.into());
        self
    }

    pub fn security_tls_key(mut self, key_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.key_file = Some(key_file.into());
        self
    }

    pub fn security_tls_ca(mut self, ca_file: impl Into<PathBuf>) -> Self {
        self.config.security.tls.ca_file = Some(ca_file.into());
        self
    }

    pub fn security_tls_require_client_cert(mut self, require: bool) -> Self {
        self.config.security.tls.require_client_cert = require;
        self
    }

    pub fn security_auth_enabled(mut self, enabled: bool) -> Self {
        self.config.security.auth.enabled = enabled;
        self
    }

    pub fn security_auth_method(mut self, method: impl Into<String>) -> Self {
        self.config.security.auth.method = method.into();
        self
    }

    pub fn security_auth_token_file(mut self, token_file: impl Into<PathBuf>) -> Self {
        self.config.security.auth.token_file = Some(token_file.into());
        self
    }

    /// Discovery configuration methods
    pub fn discovery_enable_static(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_static = enabled;
        self
    }

    pub fn discovery_enable_dns(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_dns = enabled;
        self
    }

    pub fn discovery_enable_mdns(mut self, enabled: bool) -> Self {
        self.config.network.discovery.enable_mdns = enabled;
        self
    }

    pub fn discovery_add_static_node(
        mut self,
        node_id: impl Into<String>,
        addresses: Vec<String>,
        name: Option<String>,
    ) -> Self {
        use super::StaticNodeEntry;
        self.config
            .network
            .discovery
            .static_nodes
            .seeds
            .push(StaticNodeEntry {
                node_id: node_id.into(),
                addresses,
                name,
            });
        self
    }

    pub fn discovery_add_dns_domain(mut self, domain: impl Into<String>) -> Self {
        self.config
            .network
            .discovery
            .dns
            .domains
            .push(domain.into());
        self
    }

    pub fn discovery_mdns_service_name(mut self, service_name: impl Into<String>) -> Self {
        self.config.network.discovery.mdns.service_name = service_name.into();
        self
    }

    pub fn p2p_enabled(mut self, enabled: bool) -> Self {
        self.config.p2p.enabled = enabled;
        self
    }

    pub fn build(self) -> BlixardResult<Config> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}