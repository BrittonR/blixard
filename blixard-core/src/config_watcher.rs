//! Configuration file watcher for hot-reload functionality
//!
//! This module provides file system watching capabilities to detect
//! configuration changes and trigger hot-reloads.

use crate::config_v2::{self, Config};
use crate::error::{BlixardError, BlixardResult};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Configuration watcher that monitors file changes
pub struct ConfigWatcher {
    /// Path to the configuration file
    _config_path: PathBuf,

    /// File system watcher
    _watcher: RecommendedWatcher,

    /// Channel for shutdown signal
    shutdown_tx: mpsc::Sender<()>,

    /// Background task handle
    task_handle: Option<JoinHandle<()>>,
}

impl ConfigWatcher {
    /// Create a new configuration watcher
    pub fn new<P: AsRef<Path>>(config_path: P) -> BlixardResult<Self> {
        let config_path = config_path.as_ref().to_path_buf();

        // Create channels
        let (event_tx, mut event_rx) = mpsc::channel(100);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // Create file watcher
        let mut watcher =
            notify::recommended_watcher(move |event: Result<Event, notify::Error>| {
                if let Ok(event) = event {
                    let _ = event_tx.blocking_send(event);
                }
            })
            .map_err(|e| {
                BlixardError::ConfigError(format!("Failed to create file watcher: {}", e))
            })?;

        // Watch the config file
        watcher
            .watch(&config_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                BlixardError::ConfigError(format!("Failed to watch config file: {}", e))
            })?;

        // Also watch the parent directory for file moves/renames
        if let Some(parent) = config_path.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }

        let config_path_clone = config_path.clone();

        // Spawn background task to handle events
        let task_handle = tokio::spawn(async move {
            let mut last_reload = std::time::Instant::now();
            let debounce_duration = Duration::from_millis(500);

            loop {
                tokio::select! {
                    Some(event) = event_rx.recv() => {
                        // Check if this is a relevant event
                        if Self::is_config_change_event(&event, &config_path_clone) {
                            // Debounce rapid changes
                            let now = std::time::Instant::now();
                            if now.duration_since(last_reload) < debounce_duration {
                                continue;
                            }

                            tracing::info!("Configuration file changed, attempting hot-reload");

                            // Attempt to reload configuration
                            match config_v2::reload(&config_path_clone).await {
                                Ok(()) => {
                                    tracing::info!("Configuration hot-reload successful");
                                    last_reload = now;
                                }
                                Err(e) => {
                                    tracing::error!("Configuration hot-reload failed: {}", e);
                                    // Continue watching - don't crash on bad config
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Configuration watcher shutting down");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            _config_path: config_path,
            _watcher: watcher,
            shutdown_tx,
            task_handle: Some(task_handle),
        })
    }

    /// Check if an event is a configuration change
    fn is_config_change_event(event: &Event, config_path: &Path) -> bool {
        // Check if the event is a modification or creation
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {
                // Check if any of the paths match our config file
                event.paths.iter().any(|p| p == config_path)
            }
            _ => false,
        }
    }

    /// Stop watching for configuration changes
    pub async fn stop(&mut self) {
        let _ = self.shutdown_tx.send(()).await;

        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
    }
}

/// Configuration update handler
pub trait ConfigUpdateHandler: Send + Sync + 'static {
    /// Called when configuration is updated
    fn on_config_update(&self, old_config: &Config, new_config: &Config);
}

/// Advanced configuration watcher with update handlers
pub struct ConfigWatcherWithHandlers {
    /// Base watcher
    watcher: ConfigWatcher,

    /// Configuration update handlers
    handlers: Vec<Arc<dyn ConfigUpdateHandler>>,

    /// Subscription to config updates
    config_rx: tokio::sync::watch::Receiver<Arc<Config>>,

    /// Handler task
    handler_task: Option<JoinHandle<()>>,
}

impl ConfigWatcherWithHandlers {
    /// Create a new watcher with handlers
    pub fn new<P: AsRef<Path>>(config_path: P) -> BlixardResult<Self> {
        let watcher = ConfigWatcher::new(config_path)?;
        let config_rx = config_v2::subscribe();

        Ok(Self {
            watcher,
            handlers: Vec::new(),
            config_rx,
            handler_task: None,
        })
    }

    /// Add an update handler
    pub fn add_handler(&mut self, handler: Arc<dyn ConfigUpdateHandler>) {
        self.handlers.push(handler);
    }

    /// Start watching with handlers
    pub fn start(&mut self) {
        let handlers = self.handlers.clone();
        let mut config_rx = self.config_rx.clone();

        let task = tokio::spawn(async move {
            let mut current_config = (*config_rx.borrow()).clone();

            while config_rx.changed().await.is_ok() {
                let new_config = (*config_rx.borrow()).clone();

                // Call all handlers
                for handler in &handlers {
                    handler.on_config_update(&current_config, &new_config);
                }

                current_config = new_config;
            }
        });

        self.handler_task = Some(task);
    }

    /// Stop the watcher
    pub async fn stop(&mut self) {
        self.watcher.stop().await;

        if let Some(task) = self.handler_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

/// Example handler that logs configuration changes
pub struct LoggingConfigHandler;

impl ConfigUpdateHandler for LoggingConfigHandler {
    fn on_config_update(&self, old_config: &Config, new_config: &Config) {
        // Check what changed
        if old_config.observability.logging.level != new_config.observability.logging.level {
            tracing::info!(
                "Log level changed from {} to {}",
                old_config.observability.logging.level,
                new_config.observability.logging.level
            );

            // Apply the new log level
            // This would update the tracing subscriber filter in a real implementation
        }

        if old_config.cluster.peer.health_check_interval
            != new_config.cluster.peer.health_check_interval
        {
            tracing::info!(
                "Health check interval changed from {:?} to {:?}",
                old_config.cluster.peer.health_check_interval,
                new_config.cluster.peer.health_check_interval
            );
        }

        // Log other important changes...
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_config_watcher_creation() {
        // Create a temporary config file
        let temp_file = NamedTempFile::new()
            .map_err(|e| format!("Failed to create temp file: {}", e))
            .expect("Test setup should succeed");
        let config_content = r#"
[node]
bind_address = "127.0.0.1:7001"
data_dir = "./test_data"
"#;
        fs::write(temp_file.path(), config_content)
            .map_err(|e| format!("Failed to write test config: {}", e))
            .expect("Test setup should succeed");

        // Create watcher
        let watcher = ConfigWatcher::new(temp_file.path());
        assert!(watcher.is_ok());

        // Clean up
        let mut watcher = watcher.expect("Watcher creation should succeed in test");
        watcher.stop().await;
    }
}
