//! Configuration provider abstractions
//!
//! This module provides trait-based abstractions for configuration access,
//! eliminating global state dependencies and enabling better testing.

use async_trait::async_trait;
use std::sync::Arc;
use crate::{
    error::BlixardResult,
    config_v2::Config,
};

/// Abstraction for configuration access
#[async_trait]
pub trait ConfigProvider: Send + Sync {
    /// Get current configuration
    async fn get(&self) -> BlixardResult<Arc<Config>>;
    
    /// Get a specific configuration value by path
    async fn get_value(&self, path: &str) -> BlixardResult<Option<String>>;
    
    /// Reload configuration from source
    async fn reload(&self) -> BlixardResult<()>;
    
    /// Subscribe to configuration changes
    async fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<Config>>;
}

// Production implementation using global config

/// Production config provider using global configuration
pub struct GlobalConfigProvider;

impl GlobalConfigProvider {
    /// Create new instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for GlobalConfigProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigProvider for GlobalConfigProvider {
    async fn get(&self) -> BlixardResult<Arc<Config>> {
        crate::config_v2::try_get()
    }
    
    async fn get_value(&self, path: &str) -> BlixardResult<Option<String>> {
        let config = self.get().await?;
        
        // Simple path-based value extraction
        // In a real implementation, this would use a proper path parser
        let value = match path {
            "node.id" => Some(config.node.id.to_string()),
            "node.bind_address" => Some(config.node.bind_address.clone()),
            "node.data_dir" => Some(config.node.data_dir.clone()),
            "cluster.name" => Some(config.cluster.name.clone()),
            "cluster.peer.max_connections" => Some(config.cluster.peer.max_connections.to_string()),
            _ => None,
        };
        
        Ok(value)
    }
    
    async fn reload(&self) -> BlixardResult<()> {
        // Global config doesn't support reload without a path
        Err(crate::error::BlixardError::NotImplemented {
            feature: "Configuration reload requires file path".to_string()
        })
    }
    
    async fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<Config>> {
        crate::config_v2::subscribe()
    }
}

// Mock implementation for testing

use tokio::sync::{RwLock, watch};

/// Mock config provider for testing
pub struct MockConfigProvider {
    config: Arc<RwLock<Arc<Config>>>,
    sender: Arc<RwLock<watch::Sender<Arc<Config>>>>,
    receiver: watch::Receiver<Arc<Config>>,
}

impl MockConfigProvider {
    /// Create new mock provider with default config
    pub fn new() -> Self {
        let config = Arc::new(Config::default());
        let (tx, rx) = watch::channel(config.clone());
        
        Self {
            config: Arc::new(RwLock::new(config)),
            sender: Arc::new(RwLock::new(tx)),
            receiver: rx,
        }
    }
    
    /// Create with specific config
    pub fn with_config(config: Config) -> Self {
        let config = Arc::new(config);
        let (tx, rx) = watch::channel(config.clone());
        
        Self {
            config: Arc::new(RwLock::new(config)),
            sender: Arc::new(RwLock::new(tx)),
            receiver: rx,
        }
    }
    
    /// Update configuration
    pub async fn set_config(&self, config: Config) {
        let config = Arc::new(config);
        *self.config.write().await = config.clone();
        let _ = self.sender.read().await.send(config);
    }
    
    /// Update a specific value
    pub async fn set_value(&self, path: &str, value: String) -> BlixardResult<()> {
        let mut config = (*self.config.read().await).as_ref().clone();
        
        // Simple path-based value setting
        match path {
            "node.id" => config.node.id = value.parse().unwrap_or(0),
            "node.bind_address" => config.node.bind_address = value,
            "node.data_dir" => config.node.data_dir = value,
            "cluster.name" => config.cluster.name = value,
            "cluster.peer.max_connections" => {
                config.cluster.peer.max_connections = value.parse().unwrap_or(100)
            }
            _ => return Err(crate::error::BlixardError::ConfigError(
                format!("Unknown config path: {}", path)
            )),
        }
        
        self.set_config(config).await;
        Ok(())
    }
}

impl Default for MockConfigProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigProvider for MockConfigProvider {
    async fn get(&self) -> BlixardResult<Arc<Config>> {
        Ok(self.config.read().await.clone())
    }
    
    async fn get_value(&self, path: &str) -> BlixardResult<Option<String>> {
        let config = self.get().await?;
        
        let value = match path {
            "node.id" => Some(config.node.id.to_string()),
            "node.bind_address" => Some(config.node.bind_address.clone()),
            "node.data_dir" => Some(config.node.data_dir.clone()),
            "cluster.name" => Some(config.cluster.name.clone()),
            "cluster.peer.max_connections" => Some(config.cluster.peer.max_connections.to_string()),
            _ => None,
        };
        
        Ok(value)
    }
    
    async fn reload(&self) -> BlixardResult<()> {
        // No-op for mock - just trigger a change notification
        let config = self.config.read().await.clone();
        let _ = self.sender.read().await.send(config);
        Ok(())
    }
    
    async fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<Config>> {
        self.receiver.clone()
    }
}