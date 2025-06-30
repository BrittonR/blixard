//! Example of using exemplars for trace-to-metrics correlation

use blixard_core::{
    metrics_otel::{init as init_metrics, get_metrics},
    tracing_otel::init_otlp,
    observability::exemplars::{with_trace_context, integrations::*},
};
use opentelemetry::{global, trace::Tracer};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Initialize metrics
    init_metrics("blixard-exemplar-example")?;
    
    // Initialize tracing with OTLP export
    init_otlp("blixard-exemplar-example", None)?;
    
    info!("Starting exemplar demonstration...");
    
    // Simulate some operations with trace context
    for i in 0..10 {
        // Create a root span for this iteration
        let tracer = global::tracer("example");
        let span = tracer.start(format!("iteration-{}", i));
        let _guard = opentelemetry::Context::current_with_span(span).attach();
        
        // Simulate gRPC request with automatic exemplar recording
        let duration = 0.1 + (i as f64 * 0.05);
        record_grpc_request_with_trace("CreateVm", duration, i % 3 != 0);
        
        // Simulate storage operation
        record_storage_operation_with_trace("vms", "write", duration * 0.5);
        
        // Simulate Raft proposal
        record_raft_proposal_with_trace(duration * 0.8, i % 4 != 0);
        
        // Manual exemplar recording with trace context
        with_trace_context("custom-operation", |ctx| {
            let metrics = get_metrics();
            
            // Increment counter
            metrics.vm_create_total.add(1, &[]);
            
            // Record histogram value
            metrics.vm_create_duration.record(duration, &[]);
            
            info!("Completed iteration {} with trace context", i);
        });
        
        sleep(Duration::from_millis(500)).await;
    }
    
    info!("Exemplar demonstration complete!");
    info!("Metrics with exemplars should now be visible in Prometheus/Grafana");
    info!("Look for the trace_id label in the metric samples");
    
    // Keep running to allow metrics scraping
    sleep(Duration::from_secs(60)).await;
    
    Ok(())
}

/// Example of a service method that uses exemplars
async fn example_service_method() -> Result<(), Box<dyn std::error::Error>> {
    // This would typically be in your gRPC service implementation
    with_trace_context("service.example_method", |ctx| {
        let metrics = get_metrics();
        
        // Start timing
        let start = std::time::Instant::now();
        
        // Do some work...
        std::thread::sleep(Duration::from_millis(50));
        
        // Record metrics with automatic exemplar attachment
        let duration = start.elapsed().as_secs_f64();
        metrics.grpc_request_duration.record(duration, &[]);
        
        // The OTLP exporter will automatically include the trace_id
        // from the current context as an exemplar
        
        Ok(())
    })
}