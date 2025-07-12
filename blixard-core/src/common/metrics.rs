//! Standardized metrics recording patterns
//!
//! This module provides utilities for consistent metrics recording
//! across the application.

use std::time::Instant;

#[cfg(feature = "observability")]
use opentelemetry::KeyValue;

#[cfg(feature = "observability")]
mod observability_impl {
    use super::*;
    use opentelemetry::KeyValue;

    /// Trait for recording metrics in a standardized way
    pub trait MetricsRecorder {
        /// Record a counter increment
        fn record_count(&self, metric_name: &str, value: u64, labels: &[KeyValue]);

        /// Record a gauge value
        fn record_gauge(&self, metric_name: &str, value: i64, labels: &[KeyValue]);

        /// Record a histogram value
        fn record_histogram(&self, metric_name: &str, value: f64, labels: &[KeyValue]);

        /// Start a timer for duration recording
        fn start_timer(&self, metric_name: &str, labels: Vec<KeyValue>) -> MetricTimer;
    }
}

#[cfg(not(feature = "observability"))]
mod stub_impl {
    use super::*;

    /// Stub trait for when observability is disabled
    pub trait MetricsRecorder {
        /// Record a counter increment (no-op)
        fn record_count(&self, _metric_name: &str, _value: u64) {}

        /// Record a gauge value (no-op)
        fn record_gauge(&self, _metric_name: &str, _value: i64) {}

        /// Record a histogram value (no-op)
        fn record_histogram(&self, _metric_name: &str, _value: f64) {}

        /// Start a timer for duration recording (no-op)
        fn start_timer(&self, metric_name: &str) -> MetricTimer {
            MetricTimer::new_stub(metric_name.to_string())
        }
    }
}

#[cfg(feature = "observability")]
pub use observability_impl::*;
#[cfg(not(feature = "observability"))]
pub use stub_impl::*;

/// Default implementation using global metrics
pub struct GlobalMetricsRecorder;

#[cfg(feature = "observability")]
impl MetricsRecorder for GlobalMetricsRecorder {
    fn record_count(&self, metric_name: &str, value: u64, labels: &[KeyValue]) {
        if let Some(metrics) = crate::metrics_otel::try_metrics() {
            // This is a simplified version - in practice you'd look up the metric by name
            match metric_name {
                "grpc_requests_total" => metrics.grpc_requests_total.add(value, labels),
                "vm_create_total" => metrics.vm_create_total.add(value, labels),
                "peer_reconnect_attempts" => metrics.peer_reconnect_attempts.add(value, labels),
                _ => tracing::warn!("Unknown metric: {}", metric_name),
            }
        }
    }

    fn record_gauge(&self, metric_name: &str, value: i64, labels: &[KeyValue]) {
        if let Some(metrics) = crate::metrics_otel::try_metrics() {
            match metric_name {
                "vm_total" => metrics.vm_total.add(value, labels),
                "peer_connections_active" => metrics.peer_connections_active.add(value, labels),
                _ => tracing::warn!("Unknown metric: {}", metric_name),
            }
        }
    }

    fn record_histogram(&self, metric_name: &str, value: f64, labels: &[KeyValue]) {
        if let Some(metrics) = crate::metrics_otel::try_metrics() {
            match metric_name {
                "grpc_request_duration" => metrics.grpc_request_duration.record(value, labels),
                "vm_create_duration" => metrics.vm_create_duration.record(value, labels),
                _ => tracing::warn!("Unknown metric: {}", metric_name),
            }
        }
    }

    fn start_timer(&self, metric_name: &str, labels: Vec<KeyValue>) -> MetricTimer {
        MetricTimer::new(metric_name.to_string(), labels)
    }
}

#[cfg(not(feature = "observability"))]
impl MetricsRecorder for GlobalMetricsRecorder {
    fn record_count(&self, _metric_name: &str, _value: u64) {}
    fn record_gauge(&self, _metric_name: &str, _value: i64) {}
    fn record_histogram(&self, _metric_name: &str, _value: f64) {}
    fn start_timer(&self, metric_name: &str) -> MetricTimer {
        MetricTimer::new_stub(metric_name.to_string())
    }
}

/// Timer for recording operation durations
pub struct MetricTimer {
    metric_name: String,
    #[cfg(feature = "observability")]
    labels: Vec<opentelemetry::KeyValue>,
    start: Instant,
}

impl MetricTimer {
    /// Create a new timer (with observability features)
    #[cfg(feature = "observability")]
    pub fn new(metric_name: String, labels: Vec<opentelemetry::KeyValue>) -> Self {
        Self {
            metric_name,
            labels,
            start: Instant::now(),
        }
    }

    /// Create a new timer stub (without observability features)
    #[cfg(not(feature = "observability"))]
    pub fn new_stub(metric_name: String) -> Self {
        Self {
            metric_name,
            start: Instant::now(),
        }
    }

    /// Record the duration and consume the timer
    #[cfg(feature = "observability")]
    pub fn record(self) {
        let duration = self.start.elapsed().as_secs_f64();
        let recorder = GlobalMetricsRecorder;
        recorder.record_histogram(&self.metric_name, duration, &self.labels);
    }

    /// Record the duration and consume the timer (stub version)
    #[cfg(not(feature = "observability"))]
    pub fn record(self) {
        // No-op in stub version
    }
}

impl Drop for MetricTimer {
    fn drop(&mut self) {
        // Auto-record on drop if not explicitly recorded
        let duration = self.start.elapsed().as_secs_f64();
        let recorder = GlobalMetricsRecorder;
        #[cfg(feature = "observability")]
        recorder.record_histogram(&self.metric_name, duration, &self.labels);
        #[cfg(not(feature = "observability"))]
        recorder.record_histogram(&self.metric_name, duration);
    }
}

/// Builder for recording metrics with labels
#[cfg(feature = "observability")]
pub struct MetricBuilder<'a> {
    recorder: &'a dyn MetricsRecorder,
    labels: Vec<KeyValue>,
}

/// Stub builder when observability is disabled
#[cfg(not(feature = "observability"))]
pub struct MetricBuilder<'a> {
    _recorder: &'a dyn MetricsRecorder,
}

#[cfg(feature = "observability")]
impl<'a> MetricBuilder<'a> {
    /// Create a new metric builder
    pub fn new(recorder: &'a dyn MetricsRecorder) -> Self {
        Self {
            recorder,
            labels: Vec::new(),
        }
    }

    /// Add a label
    pub fn label(mut self, key: &str, value: impl ToString) -> Self {
        self.labels
            .push(KeyValue::new(key.to_string(), value.to_string()));
        self
    }

    /// Add method label
    pub fn method(mut self, method: &str) -> Self {
        self.labels
            .push(KeyValue::new("method", method.to_string()));
        self
    }

    /// Add node ID label
    pub fn node_id(mut self, node_id: u64) -> Self {
        self.labels
            .push(KeyValue::new("node_id", node_id.to_string()));
        self
    }

    /// Add error label
    pub fn error(mut self, is_error: bool) -> Self {
        self.labels
            .push(KeyValue::new("error", is_error.to_string()));
        self
    }

    /// Record a counter
    pub fn count(self, metric_name: &str, value: u64) {
        self.recorder.record_count(metric_name, value, &self.labels);
    }

    /// Record a gauge
    pub fn gauge(self, metric_name: &str, value: i64) {
        self.recorder.record_gauge(metric_name, value, &self.labels);
    }

    /// Start a timer
    pub fn timer(self, metric_name: &str) -> MetricTimer {
        self.recorder.start_timer(metric_name, self.labels)
    }
}

#[cfg(not(feature = "observability"))]
impl<'a> MetricBuilder<'a> {
    /// Create a new metric builder (stub)
    pub fn new(recorder: &'a dyn MetricsRecorder) -> Self {
        Self {
            _recorder: recorder,
        }
    }

    /// Add a label (stub)
    pub fn label(self, _key: &str, _value: impl ToString) -> Self {
        self
    }

    /// Add method label (stub)
    pub fn method(self, _method: &str) -> Self {
        self
    }

    /// Add node ID label (stub)
    pub fn node_id(self, _node_id: u64) -> Self {
        self
    }

    /// Add error label (stub)
    pub fn error(self, _is_error: bool) -> Self {
        self
    }

    /// Record a counter (stub)
    pub fn count(self, _metric_name: &str, _value: u64) {
        // No-op when observability is disabled
    }

    /// Record a gauge (stub)
    pub fn gauge(self, _metric_name: &str, _value: i64) {
        // No-op when observability is disabled
    }

    /// Start a timer (stub)
    pub fn timer(self, metric_name: &str) -> MetricTimer {
        MetricTimer::new_stub(metric_name.to_string())
    }
}

/// Convenience function to create a metric builder
pub fn record_metric<'a>(recorder: &'a dyn MetricsRecorder) -> MetricBuilder<'a> {
    MetricBuilder::new(recorder)
}

/// Standard metric recording patterns for common operations
/// Record a gRPC request
pub fn record_grpc_request(method: &str, node_id: u64, success: bool, duration_secs: f64) {
    let recorder = GlobalMetricsRecorder;

    record_metric(&recorder)
        .method(method)
        .node_id(node_id)
        .count("grpc_requests_total", 1);

    if !success {
        record_metric(&recorder)
            .method(method)
            .error(true)
            .count("grpc_requests_failed", 1);
    }

    record_metric(&recorder)
        .method(method)
        .node_id(node_id)
        .gauge("grpc_request_duration", (duration_secs * 1000.0) as i64);
}

/// Record a VM operation
pub fn record_vm_operation(operation: &str, success: bool, duration_secs: Option<f64>) {
    let recorder = GlobalMetricsRecorder;

    let metric_name = format!("vm_{}_total", operation);
    record_metric(&recorder)
        .label("operation", operation)
        .count(&metric_name, 1);

    if !success {
        let failed_metric = format!("vm_{}_failed", operation);
        record_metric(&recorder)
            .label("operation", operation)
            .count(&failed_metric, 1);
    }

    if let Some(duration) = duration_secs {
        let duration_metric = format!("vm_{}_duration", operation);
        record_metric(&recorder)
            .label("operation", operation)
            .gauge(&duration_metric, (duration * 1000.0) as i64);
    }
}

/// Scoped timer that records on drop
pub struct ScopedTimer<'a> {
    operation: &'a str,
    node_id: Option<u64>,
    start: Instant,
}

impl<'a> ScopedTimer<'a> {
    /// Create a new scoped timer
    pub fn new(operation: &'a str) -> Self {
        Self {
            operation,
            node_id: None,
            start: Instant::now(),
        }
    }

    /// Set node ID for the timer
    pub fn with_node_id(mut self, node_id: u64) -> Self {
        self.node_id = Some(node_id);
        self
    }
}

impl<'a> Drop for ScopedTimer<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        record_grpc_request(self.operation, self.node_id.unwrap_or(0), true, duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMetricsRecorder {
        counts: std::sync::Mutex<Vec<(String, u64)>>,
    }

    impl MockMetricsRecorder {
        fn new() -> Self {
            Self {
                counts: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl MetricsRecorder for MockMetricsRecorder {
        fn record_count(&self, metric_name: &str, value: u64, _labels: &[KeyValue]) {
            self.counts
                .lock()
                .unwrap()
                .push((metric_name.to_string(), value));
        }

        fn record_gauge(&self, _metric_name: &str, _value: i64, _labels: &[KeyValue]) {}
        fn record_histogram(&self, _metric_name: &str, _value: f64, _labels: &[KeyValue]) {}
        fn start_timer(&self, metric_name: &str, labels: Vec<KeyValue>) -> MetricTimer {
            MetricTimer::new(metric_name.to_string(), labels)
        }
    }

    #[test]
    fn test_metric_builder() {
        let recorder = MockMetricsRecorder::new();

        record_metric(&recorder)
            .method("test_method")
            .node_id(42)
            .count("test_metric", 5);

        let counts = recorder.counts.lock().unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].0, "test_metric");
        assert_eq!(counts[0].1, 5);
    }
}
