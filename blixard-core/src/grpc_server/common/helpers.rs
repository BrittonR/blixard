//! Helper functions and macros for gRPC services

use crate::{
    metrics_otel::{metrics, Timer, attributes},
    tracing_otel,
};
use tonic::Request;


/// Instrument a gRPC method with tracing and metrics
pub fn instrument_grpc_method<T>(
    request: &Request<T>,
    method_name: &str,
    node_id: u64,
) -> (opentelemetry::Context, Timer) {
    // Extract trace context from incoming request
    let parent_context = tracing_otel::extract_context(request);
    let _guard = parent_context.clone().attach();
    
    // Create span for this operation
    let span = tracing_otel::grpc_span(method_name, opentelemetry::trace::SpanKind::Server);
    let _enter = span.enter();
    
    // Record metrics
    let metrics = metrics();
    let timer = Timer::with_attributes(
        metrics.grpc_request_duration.clone(),
        vec![
            attributes::method(method_name),
            attributes::node_id(node_id),
        ],
    );
    metrics.grpc_requests_total.add(1, &[attributes::method(method_name)]);
    
    (parent_context, timer)
}

/// Record a gRPC error in metrics and tracing
pub fn record_grpc_error(method_name: &str, error: Option<&dyn std::error::Error>) {
    metrics().grpc_requests_failed.add(1, &[
        attributes::method(method_name),
        attributes::error(true),
    ]);
    
    if let Some(err) = error {
        tracing_otel::record_error(err);
        tracing::error!(
            error = true,
            error_message = %err,
            "gRPC method {} failed",
            method_name
        );
    } else {
        tracing::error!(
            error = true,
            "gRPC method {} failed",
            method_name
        );
    }
}

/// Helper macro to instrument gRPC methods with tracing
#[macro_export]
macro_rules! instrument_grpc {
    ($node:expr, $request:expr, $method:expr) => {{
        use $crate::grpc_server::common::helpers::instrument_grpc_method;
        
        let (_context, _timer) = instrument_grpc_method(&$request, $method, $node.get_id());
        $request.into_inner()
    }};
}

/// Helper macro to record gRPC errors in metrics and tracing
#[macro_export]
macro_rules! record_grpc_error {
    ($method:expr) => {
        $crate::grpc_server::common::helpers::record_grpc_error($method, None)
    };
    ($method:expr, $error:expr) => {
        $crate::grpc_server::common::helpers::record_grpc_error($method, Some(&$error))
    };
}