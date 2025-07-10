//! Batch processing configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::error::{BlixardError, BlixardResult};
use super::defaults::*;
use super::parse_duration_from_env;

/// Batch processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BatchConfig {
    /// Maximum number of items in a batch
    pub max_batch_size: usize,
    
    /// Timeout before processing incomplete batch
    #[serde(with = "humantime_serde")]
    pub batch_timeout: Duration,
    
    /// Maximum batch size in bytes
    pub max_batch_bytes: usize,
    
    /// Queue size for pending items
    pub queue_size: usize,
    
    /// Enable adaptive batching
    pub adaptive: bool,
    
    /// Minimum batch size for adaptive batching
    pub adaptive_min_size: usize,
    
    /// Target latency for adaptive batching
    #[serde(with = "humantime_serde")]
    pub adaptive_target_latency: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
            batch_timeout: duration_ms(DEFAULT_BATCH_TIMEOUT_MS),
            max_batch_bytes: DEFAULT_MAX_BATCH_BYTES,
            queue_size: DEFAULT_BATCH_QUEUE_SIZE,
            adaptive: false,
            adaptive_min_size: 10,
            adaptive_target_latency: Duration::from_millis(50),
        }
    }
}

impl BatchConfig {
    /// Load batch configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_BATCH_SIZE") {
            config.max_batch_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_BATCH_SIZE".to_string())
            )?;
        }
        
        config.batch_timeout = parse_duration_from_env(
            "BLIXARD_BATCH_TIMEOUT_MS",
            config.batch_timeout
        );
        
        if let Ok(val) = std::env::var("BLIXARD_MAX_BATCH_BYTES") {
            config.max_batch_bytes = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_MAX_BATCH_BYTES".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_BATCH_QUEUE_SIZE") {
            config.queue_size = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_BATCH_QUEUE_SIZE".to_string())
            )?;
        }
        
        if let Ok(val) = std::env::var("BLIXARD_BATCH_ADAPTIVE") {
            config.adaptive = val.parse().map_err(|_| 
                BlixardError::ConfigError("Invalid BLIXARD_BATCH_ADAPTIVE".to_string())
            )?;
        }
        
        Ok(config)
    }
    
    /// Validate batch configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate batch size
        if self.max_batch_size == 0 {
            return Err(BlixardError::ConfigError(
                "max_batch_size must be at least 1".to_string()
            ));
        }
        
        if self.max_batch_size > 10000 {
            return Err(BlixardError::ConfigError(
                "max_batch_size too large (max 10000)".to_string()
            ));
        }
        
        // Validate batch timeout
        if self.batch_timeout < Duration::from_millis(1) {
            return Err(BlixardError::ConfigError(
                "batch_timeout too small (min 1ms)".to_string()
            ));
        }
        
        if self.batch_timeout > Duration::from_secs(60) {
            return Err(BlixardError::ConfigError(
                "batch_timeout too large (max 60s)".to_string()
            ));
        }
        
        // Validate batch bytes
        if self.max_batch_bytes < 1024 { // 1KB
            return Err(BlixardError::ConfigError(
                "max_batch_bytes too small (min 1KB)".to_string()
            ));
        }
        
        if self.max_batch_bytes > 100 * 1024 * 1024 { // 100MB
            return Err(BlixardError::ConfigError(
                "max_batch_bytes too large (max 100MB)".to_string()
            ));
        }
        
        // Validate queue size
        if self.queue_size < self.max_batch_size {
            return Err(BlixardError::ConfigError(
                "queue_size must be at least max_batch_size".to_string()
            ));
        }
        
        // Validate adaptive batching
        if self.adaptive {
            if self.adaptive_min_size == 0 || self.adaptive_min_size > self.max_batch_size {
                return Err(BlixardError::ConfigError(
                    "adaptive_min_size must be between 1 and max_batch_size".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Get effective batch size based on adaptive settings
    pub fn effective_batch_size(&self, current_latency: Duration) -> usize {
        if !self.adaptive {
            return self.max_batch_size;
        }
        
        // Simple adaptive algorithm: adjust batch size based on latency
        if current_latency > self.adaptive_target_latency {
            // Reduce batch size if latency is too high
            std::cmp::max(
                self.adaptive_min_size,
                self.max_batch_size * 3 / 4
            )
        } else if current_latency < self.adaptive_target_latency / 2 {
            // Increase batch size if latency is low
            self.max_batch_size
        } else {
            // Keep current size if within target range
            self.max_batch_size * 7 / 8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_batch_config_is_valid() {
        let config = BatchConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_batch_size() {
        let mut config = BatchConfig::default();
        config.max_batch_size = 0;
        assert!(config.validate().is_err());
        
        config.max_batch_size = 20000;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_adaptive_batch_size() {
        let mut config = BatchConfig::default();
        config.adaptive = true;
        config.max_batch_size = 100;
        config.adaptive_min_size = 10;
        
        // High latency should reduce batch size
        let size = config.effective_batch_size(Duration::from_millis(100));
        assert!(size < config.max_batch_size);
        assert!(size >= config.adaptive_min_size);
        
        // Low latency should maintain max batch size
        let size = config.effective_batch_size(Duration::from_millis(10));
        assert_eq!(size, config.max_batch_size);
    }
}