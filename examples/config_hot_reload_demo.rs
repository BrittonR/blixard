//! Example demonstrating configuration hot-reload functionality

use blixard_core::config::Config;
use blixard_core::config_watcher::ConfigWatcher;
use std::path::Path;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Path to configuration file
    let config_path = Path::new("config/development.toml");
    
    // Load initial configuration
    let initial_config = Config::from_file(config_path)?;
    tracing::info!("Initial config loaded - log level: {}", initial_config.observability.logging.level);
    
    // Create config watcher with change handler
    let watcher = ConfigWatcher::new(
        config_path,
        Box::new(|config: Arc<Config>| {
            tracing::info!("Configuration changed!");
            tracing::info!("  New log level: {}", config.observability.logging.level);
            tracing::info!("  New peer health check interval: {:?}", config.cluster.peer.health_check_interval);
            tracing::info!("  New scheduler strategy: {}", config.vm.scheduler.default_strategy);
            
            // Apply hot-reloadable changes
            if Config::is_hot_reloadable("observability.logging.level") {
                tracing::info!("  → Log level is hot-reloadable, would apply change now");
            }
            
            // Show non-reloadable settings
            tracing::info!("  Bind address: {} (requires restart)", config.node.bind_address);
        }),
    )?;
    
    tracing::info!("Config watcher started. Try editing config/development.toml");
    tracing::info!("Hot-reloadable settings:");
    tracing::info!("  - observability.logging.level");
    tracing::info!("  - cluster.peer.health_check_interval");
    tracing::info!("  - vm.scheduler.default_strategy");
    tracing::info!("");
    tracing::info!("Press Ctrl+C to exit");
    
    // Keep the program running
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}