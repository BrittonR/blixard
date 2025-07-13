//! Observability configuration (logging, metrics, tracing)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// Logging configuration
    pub logging: LoggingConfig,

    /// Metrics configuration
    pub metrics: MetricsConfig,

    /// Tracing configuration
    pub tracing: TracingConfig,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,

    /// Log format (pretty, json)
    pub format: String,

    /// Enable timestamps
    pub timestamps: bool,

    /// Log file path (if any)
    pub file: Option<PathBuf>,

    /// Log rotation
    pub rotation: LogRotationConfig,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogRotationConfig {
    /// Enable rotation
    pub enabled: bool,

    /// Maximum file size in MB
    pub max_size_mb: u64,

    /// Maximum number of files
    pub max_files: usize,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metric prefix
    pub prefix: String,

    /// Include runtime metrics
    pub runtime_metrics: bool,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
    /// Enable tracing
    pub enabled: bool,

    /// OTLP endpoint
    pub otlp_endpoint: Option<String>,

    /// Service name
    pub service_name: String,

    /// Sampling ratio (0.0 - 1.0)
    pub sampling_ratio: f64,

    /// Include span events
    pub span_events: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            timestamps: true,
            file: None,
            rotation: LogRotationConfig::default(),
        }
    }
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_size_mb: 100,
            max_files: 10,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "blixard".to_string(),
            runtime_metrics: true,
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: None,
            service_name: "blixard".to_string(),
            sampling_ratio: 1.0,
            span_events: true,
        }
    }
}