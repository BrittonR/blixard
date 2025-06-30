//! Unified observability module for metrics and tracing
//!
//! This module provides a single interface for initializing and managing
//! observability features including OpenTelemetry metrics and tracing.

use crate::error::BlixardResult;
use crate::config_v2::ObservabilityConfig;
use crate::metrics_otel::Metrics;
use std::sync::Arc;
use tracing::info;

/// Unified observability manager
#[derive(Debug)]
pub struct ObservabilityManager {
    /// Configuration
    config: ObservabilityConfig,
    
    /// Metrics instance
    metrics: Option<Arc<Metrics>>,
    
    /// Whether tracing is enabled
    tracing_enabled: bool,
}

impl ObservabilityManager {
    /// Create a new observability manager
    pub async fn new(config: ObservabilityConfig) -> BlixardResult<Self> {
        info!("Initializing observability manager");
        
        // Initialize metrics if enabled
        let metrics = if config.metrics.enabled {
            // For now, we just create a global Metrics instance
            // In production, this would be configured with the OTLP exporter
            info!("Metrics enabled");
            Some(Arc::new(Metrics::global()))
        } else {
            info!("Metrics disabled");
            None
        };
        
        // Initialize tracing if enabled
        let tracing_enabled = if config.tracing.enabled {
            // Tracing is initialized globally via tracing_otel
            info!("Tracing enabled");
            true
        } else {
            info!("Tracing disabled");
            false
        };
        
        Ok(Self {
            config,
            metrics,
            tracing_enabled,
        })
    }
    
    /// Get the metrics instance
    pub fn metrics(&self) -> Option<Arc<Metrics>> {
        self.metrics.clone()
    }
    
    /// Check if metrics are enabled
    pub fn metrics_enabled(&self) -> bool {
        self.metrics.is_some()
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
        metrics: crate::config_v2::MetricsConfig {
            enabled: false,
            endpoint: None,
            export_interval_secs: 60,
            service_name: "blixard".to_string(),
            environment: "dev".to_string(),
            exemplars_enabled: false,
        },
        tracing: crate::config_v2::TracingConfig {
            enabled: false,
            endpoint: None,
            sampling_rate: 0.1,
            service_name: "blixard".to_string(),
            environment: "dev".to_string(),
            propagation_format: "w3c".to_string(),
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