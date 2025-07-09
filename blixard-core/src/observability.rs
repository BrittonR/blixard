//! Unified observability module for metrics and tracing
//!
//! This module provides a single interface for initializing and managing
//! observability features including OpenTelemetry metrics and tracing.

use crate::config_v2::ObservabilityConfig;
use crate::error::BlixardResult;
use tracing::info;

/// Unified observability manager
pub struct ObservabilityManager {
    /// Configuration
    config: ObservabilityConfig,

    /// Whether metrics are enabled
    metrics_enabled: bool,

    /// Whether tracing is enabled
    tracing_enabled: bool,
}

impl ObservabilityManager {
    /// Create a new observability manager
    pub async fn new(config: ObservabilityConfig) -> BlixardResult<Self> {
        info!("Initializing observability manager");

        // Check if metrics are enabled
        let metrics_enabled = config.metrics.enabled;
        if metrics_enabled {
            info!("Metrics enabled with prefix: {}", config.metrics.prefix);
        } else {
            info!("Metrics disabled");
        }

        // Check if tracing is enabled
        let tracing_enabled = config.tracing.enabled;
        if tracing_enabled {
            info!("Tracing enabled");
        } else {
            info!("Tracing disabled");
        }

        Ok(Self {
            config,
            metrics_enabled,
            tracing_enabled,
        })
    }

    /// Check if metrics are enabled
    pub fn metrics_enabled(&self) -> bool {
        self.metrics_enabled
    }

    /// Check if tracing is enabled
    pub fn tracing_enabled(&self) -> bool {
        self.tracing_enabled
    }

    /// Shutdown observability services
    pub async fn shutdown(&self) -> BlixardResult<()> {
        info!("Shutting down observability services");

        // Metrics and tracing shutdown would be handled globally
        // For now, this is a no-op

        Ok(())
    }
}

/// Create a default observability configuration for development
pub fn default_dev_observability_config() -> ObservabilityConfig {
    ObservabilityConfig {
        logging: crate::config_v2::LoggingConfig {
            level: "info".to_string(),
            format: "pretty".to_string(),
            timestamps: true,
            file: None,
            rotation: crate::config_v2::LogRotationConfig {
                enabled: false,
                max_size_mb: 100,
                max_files: 5,
            },
        },
        metrics: crate::config_v2::MetricsConfig {
            enabled: false,
            prefix: "blixard".to_string(),
            runtime_metrics: false,
        },
        tracing: crate::config_v2::TracingConfig {
            enabled: false,
            otlp_endpoint: None,
            service_name: "blixard".to_string(),
            sampling_ratio: 0.1,
            span_events: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_observability_manager_creation() {
        let config = default_dev_observability_config();
        let manager = ObservabilityManager::new(config).await.unwrap();

        // Should work with disabled metrics and tracing
        assert!(!manager.metrics_enabled());
        assert!(!manager.tracing_enabled());
    }
}
