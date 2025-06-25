//! Observability configuration for tracing and metrics
//!
//! This module provides utilities for setting up comprehensive observability
//! including structured logging, distributed tracing, and metrics collection.

use tracing::{Level, Span};
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Registry,
};
use std::time::Instant;

/// Initialize tracing with environment-based configuration
pub fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            // Default log levels
            EnvFilter::new("blixard_core=debug,raft=info,redb=info,tower=info,h2=info")
        });

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    Registry::default()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

/// Create a span for a Raft operation
#[inline]
pub fn raft_span(operation: &str, node_id: u64) -> Span {
    tracing::debug_span!(
        "raft",
        operation = %operation,
        node_id = %node_id,
        term = tracing::field::Empty,
        index = tracing::field::Empty,
    )
}

/// Create a span for a storage operation
#[inline]
pub fn storage_span(operation: &str, table: &str) -> Span {
    tracing::debug_span!(
        "storage",
        operation = %operation,
        table = %table,
        duration_ms = tracing::field::Empty,
    )
}

/// Create a span for a gRPC request
#[inline]
pub fn grpc_span(method: &str, peer: Option<u64>) -> Span {
    tracing::info_span!(
        "grpc",
        method = %method,
        peer = peer,
        duration_ms = tracing::field::Empty,
        status = tracing::field::Empty,
    )
}

/// Create a span for VM operations
#[inline]
pub fn vm_span(operation: &str, vm_name: &str) -> Span {
    tracing::info_span!(
        "vm",
        operation = %operation,
        vm_name = %vm_name,
        duration_ms = tracing::field::Empty,
        status = tracing::field::Empty,
    )
}

/// Record the duration of an operation in the current span
pub fn record_duration(start: Instant) {
    let duration = start.elapsed();
    Span::current().record("duration_ms", duration.as_millis() as u64);
}

/// Log levels for different components
pub mod log_levels {
    use tracing::Level;
    
    pub const RAFT_DEBUG: Level = Level::DEBUG;
    pub const RAFT_INFO: Level = Level::INFO;
    pub const STORAGE: Level = Level::DEBUG;
    pub const GRPC: Level = Level::INFO;
    pub const VM: Level = Level::INFO;
    pub const PEER: Level = Level::DEBUG;
    pub const METRICS: Level = Level::TRACE;
}

/// Macros for structured logging with metrics
#[macro_export]
macro_rules! log_and_metric {
    ($level:expr, $metric:expr, $($arg:tt)*) => {{
        tracing::event!($level, $($arg)*);
        $crate::metrics::global().increment_counter($metric);
    }};
}

#[macro_export]
macro_rules! log_error_and_metric {
    ($metric:expr, $($arg:tt)*) => {{
        tracing::error!($($arg)*);
        $crate::metrics::global().increment_counter($metric);
    }};
}

/// Example instrumented function
#[tracing::instrument(level = "debug", skip(data), fields(data_len = data.len()))]
pub async fn example_instrumented_function(operation: &str, data: &[u8]) -> Result<(), String> {
    let start = Instant::now();
    
    // Simulate some work
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Record metrics
    record_duration(start);
    
    if data.is_empty() {
        Err("Data is empty".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn test_instrumented_function() {
        let result = example_instrumented_function("test", b"hello").await;
        assert!(result.is_ok());
        
        // Check that logs were generated
        assert!(logs_contain("example_instrumented_function"));
        assert!(logs_contain("operation=\"test\""));
        assert!(logs_contain("data_len=5"));
    }

    #[test]
    fn test_span_creation() {
        let _guard = raft_span("propose", 1).entered();
        tracing::info!("Test message in raft span");
    }
}