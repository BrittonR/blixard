//! Monitoring and observability configuration

use super::defaults::*;
use super::parse_duration_secs_from_env;
use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Monitoring and observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitoringConfig {
    /// Metrics server port
    pub metrics_port: u16,

    /// Health check server port
    pub health_check_port: u16,

    /// Log level
    pub log_level: String,

    /// Metrics collection interval
    #[serde(with = "humantime_serde")]
    pub metrics_interval: Duration,

    /// Enable metrics collection
    pub metrics_enabled: bool,

    /// Enable distributed tracing
    pub tracing_enabled: bool,

    /// Trace sample rate (0.0 to 1.0)
    pub trace_sample_rate: f64,

    /// OpenTelemetry configuration
    pub otel: OtelConfig,

    /// Prometheus configuration
    pub prometheus: PrometheusConfig,

    /// Alerting configuration
    pub alerting: AlertingConfig,
}

/// OpenTelemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelConfig {
    /// OTLP endpoint
    pub endpoint: Option<String>,

    /// Service name
    pub service_name: String,

    /// Service version
    pub service_version: String,

    /// Export timeout
    #[serde(with = "humantime_serde")]
    pub export_timeout: Duration,

    /// Batch size for exports
    pub batch_size: usize,

    /// Enable gRPC transport
    pub use_grpc: bool,

    /// Additional resource attributes
    pub resource_attributes: std::collections::HashMap<String, String>,
}

/// Prometheus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrometheusConfig {
    /// Enable Prometheus metrics
    pub enabled: bool,

    /// Metrics path
    pub metrics_path: String,

    /// Include process metrics
    pub include_process_metrics: bool,

    /// Include runtime metrics
    pub include_runtime_metrics: bool,

    /// Metric buckets for histograms
    pub histogram_buckets: Vec<f64>,
}

/// Alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enabled: bool,

    /// Alert evaluation interval
    #[serde(with = "humantime_serde")]
    pub evaluation_interval: Duration,

    /// Alert manager URL
    pub alertmanager_url: Option<String>,

    /// Webhook URLs for alerts
    pub webhook_urls: Vec<String>,

    /// Email configuration
    pub email: Option<EmailAlertConfig>,
}

/// Email alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAlertConfig {
    /// SMTP server
    pub smtp_server: String,

    /// SMTP port
    pub smtp_port: u16,

    /// From address
    pub from: String,

    /// To addresses
    pub to: Vec<String>,

    /// Use TLS
    pub use_tls: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            metrics_port: DEFAULT_METRICS_PORT,
            health_check_port: DEFAULT_HEALTH_CHECK_PORT,
            log_level: DEFAULT_LOG_LEVEL.to_string(),
            metrics_interval: duration_secs(DEFAULT_METRICS_INTERVAL_SECS),
            metrics_enabled: true,
            tracing_enabled: false,
            trace_sample_rate: DEFAULT_TRACE_SAMPLE_RATE,
            otel: OtelConfig::default(),
            prometheus: PrometheusConfig::default(),
            alerting: AlertingConfig::default(),
        }
    }
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            service_name: "blixard".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            export_timeout: Duration::from_secs(10),
            batch_size: 512,
            use_grpc: true,
            resource_attributes: std::collections::HashMap::new(),
        }
    }
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics_path: "/metrics".to_string(),
            include_process_metrics: true,
            include_runtime_metrics: true,
            histogram_buckets: vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        }
    }
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            evaluation_interval: Duration::from_secs(60),
            alertmanager_url: None,
            webhook_urls: Vec::new(),
            email: None,
        }
    }
}

impl MonitoringConfig {
    /// Load monitoring configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();

        if let Ok(port) = std::env::var("BLIXARD_METRICS_PORT") {
            config.metrics_port = port.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_METRICS_PORT".to_string())
            })?;
        }

        if let Ok(port) = std::env::var("BLIXARD_HEALTH_CHECK_PORT") {
            config.health_check_port = port.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_HEALTH_CHECK_PORT".to_string())
            })?;
        }

        if let Ok(level) = std::env::var("BLIXARD_LOG_LEVEL") {
            config.log_level = level;
        }

        config.metrics_interval =
            parse_duration_secs_from_env("BLIXARD_METRICS_INTERVAL_SECS", config.metrics_interval);

        if let Ok(val) = std::env::var("BLIXARD_METRICS_ENABLED") {
            config.metrics_enabled = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_METRICS_ENABLED".to_string())
            })?;
        }

        if let Ok(val) = std::env::var("BLIXARD_TRACING_ENABLED") {
            config.tracing_enabled = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_TRACING_ENABLED".to_string())
            })?;
        }

        if let Ok(val) = std::env::var("BLIXARD_TRACE_SAMPLE_RATE") {
            config.trace_sample_rate = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_TRACE_SAMPLE_RATE".to_string())
            })?;
        }

        // OTEL configuration
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config.otel.endpoint = Some(endpoint);
        }

        if let Ok(name) = std::env::var("OTEL_SERVICE_NAME") {
            config.otel.service_name = name;
        }

        // Prometheus configuration
        if let Ok(val) = std::env::var("BLIXARD_PROMETHEUS_ENABLED") {
            config.prometheus.enabled = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_PROMETHEUS_ENABLED".to_string())
            })?;
        }

        // Alerting configuration
        if let Ok(val) = std::env::var("BLIXARD_ALERTING_ENABLED") {
            config.alerting.enabled = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_ALERTING_ENABLED".to_string())
            })?;
        }

        if let Ok(url) = std::env::var("BLIXARD_ALERTMANAGER_URL") {
            config.alerting.alertmanager_url = Some(url);
        }

        Ok(config)
    }

    /// Validate monitoring configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate ports
        if self.metrics_port == 0 {
            return Err(BlixardError::ConfigError(
                "metrics_port cannot be 0".to_string(),
            ));
        }

        if self.health_check_port == 0 {
            return Err(BlixardError::ConfigError(
                "health_check_port cannot be 0".to_string(),
            ));
        }

        if self.metrics_port == self.health_check_port {
            return Err(BlixardError::ConfigError(
                "metrics_port and health_check_port must be different".to_string(),
            ));
        }

        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.log_level.to_lowercase().as_str()) {
            return Err(BlixardError::ConfigError(format!(
                "Invalid log_level: {}",
                self.log_level
            )));
        }

        // Validate trace sample rate
        if self.trace_sample_rate < 0.0 || self.trace_sample_rate > 1.0 {
            return Err(BlixardError::ConfigError(
                "trace_sample_rate must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate metrics interval
        if self.metrics_interval < Duration::from_secs(1) {
            return Err(BlixardError::ConfigError(
                "metrics_interval too small (min 1s)".to_string(),
            ));
        }

        // Validate OTEL config
        if self.tracing_enabled && self.otel.endpoint.is_none() {
            return Err(BlixardError::ConfigError(
                "OTEL endpoint required when tracing is enabled".to_string(),
            ));
        }

        // Validate alerting config
        if self.alerting.enabled
            && self.alerting.alertmanager_url.is_none()
            && self.alerting.webhook_urls.is_empty()
            && self.alerting.email.is_none()
        {
            return Err(BlixardError::ConfigError(
                "At least one alert destination must be configured".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_monitoring_config_is_valid() {
        let config = MonitoringConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_ports() {
        let mut config = MonitoringConfig::default();
        config.metrics_port = 0;
        assert!(config.validate().is_err());

        config.metrics_port = 8080;
        config.health_check_port = 8080;
        assert!(config.validate().is_err());
    }
}
