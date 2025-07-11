//! OpenTelemetry-based distributed tracing
//!
//! This module provides distributed tracing using OpenTelemetry, complementing
//! the metrics module for full observability.

use opentelemetry::{
    global,
    trace::SpanKind,
    Context, KeyValue,
};
use opentelemetry_sdk::trace::{self, Sampler};
use tracing_subscriber::layer::SubscriberExt;

// We'll use the global tracer provider instead of storing a specific tracer

/// Initialize tracing with OTLP exporter
pub fn init_otlp(
    service_name: &str,
    otlp_endpoint: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use opentelemetry_otlp::WithExportConfig;
    
    // Configure OTLP exporter
    let mut exporter = opentelemetry_otlp::new_exporter()
        .tonic();
    
    if let Some(endpoint) = otlp_endpoint {
        exporter = exporter.with_endpoint(endpoint);
    }
    
    // Build the trace pipeline
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    KeyValue::new("service.name", service_name.to_string()),
                    KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                ]))
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    
    // Store tracer - use global tracer instead
    // We don't actually need to store it since we'll use global::tracer()
    
    // Create OpenTelemetry layer for tracing-subscriber
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    
    // Get existing subscriber or create new one
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer);
    
    // Set as global subscriber
    tracing::subscriber::set_global_default(subscriber)?;
    
    Ok(())
}

/// Initialize tracing without exporter (for testing)
pub fn init_noop() -> Result<(), Box<dyn std::error::Error>> {
    
    // Just use basic tracing without OpenTelemetry export
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer());
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    Ok(())
}

/// Shutdown tracing and flush any pending spans
pub fn shutdown() {
    opentelemetry::global::shutdown_tracer_provider();
}

/// Extract trace context from gRPC metadata
pub fn extract_context<T>(request: &tonic::Request<T>) -> Context {
    use opentelemetry::propagation::Extractor;
    
    struct MetadataExtractor<'a>(&'a tonic::metadata::MetadataMap);
    
    impl<'a> Extractor for MetadataExtractor<'a> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|v| v.to_str().ok())
        }
        
        fn keys(&self) -> Vec<&str> {
            // Convert metadata keys to strings
            Vec::new() // Simplified - we don't need all keys for trace context
        }
    }
    
    let parent_context = global::get_text_map_propagator(|propagator| {
        propagator.extract(&MetadataExtractor(request.metadata()))
    });
    
    parent_context
}

/// Inject trace context into gRPC metadata
pub fn inject_context<T>(request: &mut tonic::Request<T>) {
    use opentelemetry::propagation::Injector;
    
    struct MetadataInjector<'a>(&'a mut tonic::metadata::MetadataMap);
    
    impl<'a> Injector for MetadataInjector<'a> {
        fn set(&mut self, key: &str, value: String) {
            if let Ok(key) = tonic::metadata::MetadataKey::<tonic::metadata::Ascii>::from_bytes(key.as_bytes()) {
                if let Ok(val) = tonic::metadata::AsciiMetadataValue::try_from(&value) {
                    self.0.insert(key, val);
                }
            }
        }
    }
    
    let context = Context::current();
    
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut MetadataInjector(request.metadata_mut()))
    });
}

/// Helper to create a span for a gRPC method
pub fn grpc_span(method: &str, kind: SpanKind) -> tracing::Span {
    tracing::span!(
        tracing::Level::INFO,
        "grpc",
        otel.name = method,
        otel.kind = ?kind,
        rpc.system = "grpc",
        rpc.method = method,
    )
}

/// Helper to create a span for storage operations
pub fn storage_span(operation: &str, table: &str) -> tracing::Span {
    tracing::span!(
        tracing::Level::INFO,
        "storage",
        otel.name = format!("storage.{}", operation),
        otel.kind = ?SpanKind::Internal,
        db.operation = operation,
        db.table = table,
    )
}

/// Helper to create a span for Raft operations
pub fn raft_span(operation: &str) -> tracing::Span {
    tracing::span!(
        tracing::Level::INFO,
        "raft",
        otel.name = format!("raft.{}", operation),
        otel.kind = ?SpanKind::Internal,
        raft.operation = operation,
    )
}

/// Helper to record an error on the current span
pub fn record_error(error: &dyn std::error::Error) {
    tracing::Span::current().record("otel.status_code", "ERROR");
    tracing::Span::current().record("error", true);
    tracing::Span::current().record("error.message", error.to_string());
}

/// Helper to add attributes to the current span
pub fn add_attributes(attributes: &[(&str, &dyn std::fmt::Display)]) {
    // In the current tracing implementation, we need to log attributes as events
    // because dynamic field recording requires pre-declared fields
    let mut attrs = String::with_capacity(attributes.len() * 20); // Estimate 20 chars per attribute
    for (i, (key, value)) in attributes.iter().enumerate() {
        if i > 0 {
            attrs.push_str(", ");
        }
        attrs.push_str(&format!("{}={}", key, value));
    }
    tracing::debug!(attributes = %attrs, "Span attributes");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tracing_initialization() {
        let result = init_noop();
        assert!(result.is_ok());
        
        // Create a test span
        let span = tracing::span!(tracing::Level::INFO, "test_span");
        let _guard = span.enter();
        
        tracing::info!("Test event within span");
    }
}