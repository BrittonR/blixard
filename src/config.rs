use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use directories::ProjectDirs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node configuration
    pub node: NodeConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Cluster configuration
    pub cluster: ClusterConfig,
    
    /// Service management configuration
    pub service: ServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node ID (generated if not specified)
    pub id: Option<String>,
    
    /// Bind address for RPC
    pub bind_addr: String,
    
    /// Advertise address for other nodes
    pub advertise_addr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the database file
    pub db_path: PathBuf,
    
    /// Path to Raft log storage
    pub raft_log_path: PathBuf,
    
    /// Enable write-ahead logging
    pub wal_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Raft heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    
    /// Raft election timeout range in milliseconds
    pub election_timeout_min_ms: u64,
    pub election_timeout_max_ms: u64,
    
    /// Enable Tailscale integration
    pub tailscale_enabled: bool,
    
    /// Tailscale network name
    pub tailscale_network: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Default timeout for service operations in seconds
    pub operation_timeout_secs: u64,
    
    /// Enable automatic service restart
    pub auto_restart: bool,
    
    /// Maximum restart attempts
    pub max_restart_attempts: u32,
}

impl Default for Config {
    fn default() -> Self {
        let project_dirs = ProjectDirs::from("", "", "blixard")
            .expect("Failed to determine project directories");
        
        let data_dir = project_dirs.data_dir();
        
        Self {
            node: NodeConfig {
                id: None,
                bind_addr: "127.0.0.1:9090".to_string(),
                advertise_addr: None,
            },
            storage: StorageConfig {
                db_path: data_dir.join("blixard.db"),
                raft_log_path: data_dir.join("raft-log"),
                wal_enabled: true,
            },
            cluster: ClusterConfig {
                heartbeat_interval_ms: 150,
                election_timeout_min_ms: 300,
                election_timeout_max_ms: 600,
                tailscale_enabled: false,
                tailscale_network: None,
            },
            service: ServiceConfig {
                operation_timeout_secs: 30,
                auto_restart: false,
                max_restart_attempts: 3,
            },
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        if let Some(path) = path {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Try to load from default locations
            let project_dirs = ProjectDirs::from("", "", "blixard");
            
            let paths = vec![
                project_dirs.as_ref().map(|d| d.config_dir().join("config.toml")),
                Some(PathBuf::from("/etc/blixard/config.toml")),
                Some(PathBuf::from("./blixard.toml")),
            ];
            
            for path in paths.into_iter().flatten() {
                if path.exists() {
                    let content = std::fs::read_to_string(&path)?;
                    let config: Config = toml::from_str(&content)?;
                    return Ok(config);
                }
            }
            
            // Return default config if no config file found
            Ok(Self::default())
        }
    }
    
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, content)?;
        Ok(())
    }
}