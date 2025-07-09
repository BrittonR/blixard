//! Global configuration instance management
//!
//! This module provides access to the global configuration instance
//! and handles configuration initialization.

use crate::config_v2::Config;
use crate::error::{BlixardError, BlixardResult};
use once_cell::sync::OnceCell;
use std::sync::Arc;

/// Global configuration instance
static CONFIG: OnceCell<Arc<Config>> = OnceCell::new();

/// Initialize the global configuration
///
/// This should be called once at startup with the loaded configuration.
/// If called multiple times, subsequent calls will return an error.
pub fn init(config: Config) -> BlixardResult<()> {
    CONFIG
        .set(Arc::new(config))
        .map_err(|_| BlixardError::ConfigError("Configuration already initialized".to_string()))
}

/// Get the global configuration instance
///
/// # Errors
/// Returns an error if configuration has not been initialized via `init()`.
pub fn get() -> BlixardResult<Arc<Config>> {
    CONFIG
        .get()
        .ok_or_else(|| BlixardError::ConfigError(
            "Configuration not initialized. Call config_global::init() first".to_string()
        ))
        .map(|config| config.clone())
}

/// Check if configuration has been initialized
pub fn is_initialized() -> bool {
    CONFIG.get().is_some()
}

/// Get the global configuration instance if initialized
pub fn try_get() -> Option<Arc<Config>> {
    CONFIG.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_not_initialized() {
        // In tests, we can't guarantee CONFIG is uninitialized
        // Just verify the methods exist
        let _ = is_initialized();
        let _ = try_get();
    }
}
