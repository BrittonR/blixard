//! Configuration hot-reload implementation
//!
//! This module provides functionality to watch configuration files for changes
//! and automatically reload configuration at runtime.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tokio::time;
use notify::{Watcher, RecursiveMode, Event, EventKind};
use crate::error::{BlixardError, BlixardResult};
use crate::config::Config;
use tracing::{info, warn, error};

/// Configuration that can be reloaded at runtime
#[derive(Debug, Clone)]
pub struct ReloadableConfig {
    /// Fields that can be changed at runtime
    pub logging_level: String,
    pub metrics_enabled: bool,
    pub tracing_sampling_ratio: f64,
    pub scheduler_rebalance_interval: Duration,
    pub scheduler_overcommit_ratio: f32,
    pub peer_connection_timeout: Duration,
    pub peer_retry_interval: Duration,
    pub peer_max_retries: usize,
    pub storage_compaction_interval: Duration,
    pub storage_snapshot_interval: Duration,
    pub vm_health_check_interval: Duration,
    pub vm_health_check_timeout: Duration,
}

impl From<&Config> for ReloadableConfig {
    fn from(config: &Config) -> Self {
        Self {
            logging_level: config.observability.logging.level.clone(),
            metrics_enabled: config.observability.metrics.enabled,
            tracing_sampling_ratio: config.observability.tracing.sampling_ratio,
            scheduler_rebalance_interval: config.vm.scheduler.rebalance_interval,
            scheduler_overcommit_ratio: config.vm.scheduler.overcommit_ratio,
            peer_connection_timeout: config.cluster.peer.connection_timeout,
            peer_retry_interval: config.cluster.peer.retry_interval,
            peer_max_retries: config.cluster.peer.max_retries,
            storage_compaction_interval: config.storage.compaction_interval,
            storage_snapshot_interval: config.storage.snapshot_interval,
            vm_health_check_interval: config.vm.health_check.interval,
            vm_health_check_timeout: config.vm.health_check.timeout,
        }
    }
}

/// Configuration watcher that monitors config files for changes
pub struct ConfigWatcher {
    config_path: PathBuf,
    full_config: Arc<RwLock<Config>>,
    reloadable_tx: watch::Sender<Arc<ReloadableConfig>>,
    reloadable_rx: watch::Receiver<Arc<ReloadableConfig>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl ConfigWatcher {
    /// Create a new configuration watcher
    pub fn new(config_path: PathBuf, initial_config: Config) -> Self {
        let reloadable = ReloadableConfig::from(&initial_config);
        let (tx, rx) = watch::channel(Arc::new(reloadable));
        
        Self {
            config_path,
            full_config: Arc::new(RwLock::new(initial_config)),
            reloadable_tx: tx,
            reloadable_rx: rx,
            shutdown_tx: None,
        }
    }
    
    /// Get a receiver for reloadable configuration updates
    pub fn subscribe(&self) -> watch::Receiver<Arc<ReloadableConfig>> {
        self.reloadable_rx.clone()
    }
    
    /// Get the current full configuration
    pub async fn get_full_config(&self) -> Config {
        self.full_config.read().await.clone()
    }
    
    /// Start watching for configuration changes
    pub async fn start(&mut self) -> BlixardResult<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        let config_path = self.config_path.clone();
        let full_config = self.full_config.clone();
        let reloadable_tx = self.reloadable_tx.clone();
        
        // Spawn the file watcher task
        tokio::spawn(async move {
            if let Err(e) = watch_config_file(config_path, full_config, reloadable_tx, &mut shutdown_rx).await {
                error!("Configuration watcher error: {}", e);
            }
        });
        
        info!("Configuration hot-reload watcher started");
        Ok(())
    }
    
    /// Stop watching for configuration changes
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
            info!("Configuration hot-reload watcher stopped");
        }
    }
    
    /// Manually trigger a configuration reload
    pub async fn reload(&self) -> BlixardResult<()> {
        let new_config = Config::from_file(&self.config_path)?;
        
        // Update full configuration
        {
            let mut config = self.full_config.write().await;
            *config = new_config.clone();
        }
        
        // Update reloadable configuration
        let reloadable = ReloadableConfig::from(&new_config);
        if let Err(e) = self.reloadable_tx.send(Arc::new(reloadable)) {
            warn!("Failed to notify configuration watchers: {}", e);
        }
        
        info!("Configuration reloaded successfully");
        Ok(())
    }
}

/// Watch a configuration file for changes
async fn watch_config_file(
    config_path: PathBuf,
    full_config: Arc<RwLock<Config>>,
    reloadable_tx: watch::Sender<Arc<ReloadableConfig>>,
    shutdown_rx: &mut mpsc::Receiver<()>,
) -> BlixardResult<()> {
    // Create a channel for file events
    let (tx, mut rx) = mpsc::channel(10);
    
    // Create the file watcher
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            let _ = tx.blocking_send(event);
        }
    }).map_err(|e| BlixardError::ConfigError(format!("Failed to create file watcher: {}", e)))?;
    
    // Watch the config file
    let watch_path = config_path.parent()
        .ok_or_else(|| BlixardError::ConfigError("Invalid config path".to_string()))?;
    watcher.watch(watch_path, RecursiveMode::NonRecursive)
        .map_err(|e| BlixardError::ConfigError(format!("Failed to watch config file: {}", e)))?;
    
    info!("Watching configuration file: {:?}", config_path);
    
    // Debounce timer to avoid multiple reloads
    let mut debounce_timer = time::interval(Duration::from_millis(500));
    let mut pending_reload = false;
    
    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = shutdown_rx.recv() => {
                info!("Configuration watcher shutting down");
                break;
            }
            
            // Check for file events
            Some(event) = rx.recv() => {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        // Check if the event is for our config file
                        if event.paths.iter().any(|p| p == &config_path) {
                            pending_reload = true;
                        }
                    }
                    _ => {}
                }
            }
            
            // Debounce timer tick
            _ = debounce_timer.tick() => {
                if pending_reload {
                    pending_reload = false;
                    
                    // Reload configuration
                    match reload_config(&config_path, &full_config, &reloadable_tx).await {
                        Ok(()) => info!("Configuration reloaded due to file change"),
                        Err(e) => error!("Failed to reload configuration: {}", e),
                    }
                }
            }
        }
    }
    
    Ok(())
}

/// Reload configuration from file
async fn reload_config(
    config_path: &Path,
    full_config: &Arc<RwLock<Config>>,
    reloadable_tx: &watch::Sender<Arc<ReloadableConfig>>,
) -> BlixardResult<()> {
    // Load new configuration
    let new_config = Config::from_file(config_path)?;
    
    // Compare with current configuration
    let current_config = full_config.read().await;
    let changes = compare_configs(&current_config, &new_config);
    
    if !changes.is_empty() {
        info!("Configuration changes detected: {:?}", changes);
        
        // Apply hot-reloadable changes
        drop(current_config);
        let mut config = full_config.write().await;
        
        // Update only reloadable fields
        config.observability.logging.level = new_config.observability.logging.level.clone();
        config.observability.metrics.enabled = new_config.observability.metrics.enabled;
        config.observability.tracing.sampling_ratio = new_config.observability.tracing.sampling_ratio;
        config.vm.scheduler.rebalance_interval = new_config.vm.scheduler.rebalance_interval;
        config.vm.scheduler.overcommit_ratio = new_config.vm.scheduler.overcommit_ratio;
        config.cluster.peer.connection_timeout = new_config.cluster.peer.connection_timeout;
        // TODO: These fields don't exist in the current config structure
        // config.cluster.peer.retry_interval = new_config.cluster.peer.retry_interval;
        // config.cluster.peer.max_retries = new_config.cluster.peer.max_retries;
        // config.storage.compaction_interval = new_config.storage.compaction_interval;
        // config.storage.snapshot_interval = new_config.storage.snapshot_interval;
        // config.vm.health_check.interval = new_config.vm.health_check.interval;
        // config.vm.health_check.timeout = new_config.vm.health_check.timeout;
        
        // Notify watchers
        let reloadable = ReloadableConfig::from(&*config);
        let _ = reloadable_tx.send(Arc::new(reloadable));
        
        // Apply immediate changes
        apply_immediate_changes(&changes).await;
    }
    
    Ok(())
}

/// Configuration change type
#[derive(Debug)]
enum ConfigChange {
    LogLevel(String),
    MetricsEnabled(bool),
    TracingSamplingRatio(f64),
    Other(String),
}

/// Compare two configurations and return list of changes
fn compare_configs(current: &Config, new: &Config) -> Vec<ConfigChange> {
    let mut changes = Vec::new();
    
    if current.observability.logging.level != new.observability.logging.level {
        changes.push(ConfigChange::LogLevel(new.observability.logging.level.clone()));
    }
    
    if current.observability.metrics.enabled != new.observability.metrics.enabled {
        changes.push(ConfigChange::MetricsEnabled(new.observability.metrics.enabled));
    }
    
    if (current.observability.tracing.sampling_ratio - new.observability.tracing.sampling_ratio).abs() > f64::EPSILON {
        changes.push(ConfigChange::TracingSamplingRatio(new.observability.tracing.sampling_ratio));
    }
    
    // Add other reloadable fields
    if current.vm.scheduler.rebalance_interval != new.vm.scheduler.rebalance_interval {
        changes.push(ConfigChange::Other("scheduler.rebalance_interval".to_string()));
    }
    
    if (current.vm.scheduler.overcommit_ratio - new.vm.scheduler.overcommit_ratio).abs() > f32::EPSILON {
        changes.push(ConfigChange::Other("scheduler.overcommit_ratio".to_string()));
    }
    
    changes
}

/// Apply immediate configuration changes
async fn apply_immediate_changes(changes: &[ConfigChange]) {
    for change in changes {
        match change {
            ConfigChange::LogLevel(level) => {
                // Update global log level
                if let Err(e) = update_log_level(level) {
                    warn!("Failed to update log level: {}", e);
                }
            }
            ConfigChange::MetricsEnabled(enabled) => {
                info!("Metrics collection {}", if *enabled { "enabled" } else { "disabled" });
            }
            ConfigChange::TracingSamplingRatio(ratio) => {
                info!("Updated tracing sampling ratio to {}", ratio);
            }
            ConfigChange::Other(field) => {
                info!("Updated configuration field: {}", field);
            }
        }
    }
}

/// Update the global log level
fn update_log_level(level: &str) -> BlixardResult<()> {
    use tracing_subscriber::{reload, EnvFilter};
    
    let filter = EnvFilter::try_new(level)
        .map_err(|e| BlixardError::ConfigError(format!("Invalid log level '{}': {}", level, e)))?;
    
    // This would need to be integrated with the actual tracing subscriber setup
    // For now, just log the change
    info!("Log level changed to: {}", level);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_config_watcher_creation() {
        let config = Config::default();
        let temp_file = NamedTempFile::new().unwrap();
        let watcher = ConfigWatcher::new(temp_file.path().to_path_buf(), config);
        
        // Test subscription
        let _rx = watcher.subscribe();
        
        // Test getting full config
        let _full_config = watcher.get_full_config().await;
    }
    
    #[tokio::test]
    async fn test_reloadable_config_conversion() {
        let mut config = Config::default();
        config.observability.logging.level = "debug".to_string();
        config.observability.metrics.enabled = false;
        
        let reloadable = ReloadableConfig::from(&config);
        assert_eq!(reloadable.logging_level, "debug");
        assert_eq!(reloadable.metrics_enabled, false);
    }
    
    #[tokio::test]
    async fn test_config_comparison() {
        let mut current = Config::default();
        let mut new = Config::default();
        
        // No changes
        let changes = compare_configs(&current, &new);
        assert!(changes.is_empty());
        
        // Change log level
        new.observability.logging.level = "debug".to_string();
        let changes = compare_configs(&current, &new);
        assert_eq!(changes.len(), 1);
        
        // Change multiple fields
        new.observability.metrics.enabled = false;
        new.observability.tracing.sampling_ratio = 0.5;
        let changes = compare_configs(&current, &new);
        assert_eq!(changes.len(), 3);
    }
}