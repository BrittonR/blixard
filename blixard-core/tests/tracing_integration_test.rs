#![cfg(feature = "test-helpers")]

use blixard_core::{
    test_helpers::TestCluster,
    tracing_otel,
    metrics_otel_v2,
    proto::{
        blixard_service_client::BlixardServiceClient,
        CreateVmRequest,
    },
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_tracing_initialization() {
    // Initialize tracing without exporter
    let result = tracing_otel::init_noop();
    assert!(result.is_ok(), "Tracing initialization should succeed");
    
    // Create a test span
    let span = tracing::span!(tracing::Level::INFO, "test_span");
    let _guard = span.enter();
    
    // Log within the span
    tracing::info!("Test event within span");
    
    // Span should be recorded (even though we're using noop exporter)
    assert!(tracing::Span::current().is_enabled());
}

#[tokio::test]
async fn test_distributed_tracing_propagation() {
    // Initialize metrics and tracing
    let _ = metrics_otel_v2::init_noop();
    let _ = tracing_otel::init_noop();
    
    // Create a test cluster
    let mut cluster = TestCluster::new();
    
    // Start three nodes
    let node1 = cluster.add_node(1, 7201).await.unwrap();
    let _node2 = cluster.add_node(2, 7202).await.unwrap();
    let _node3 = cluster.add_node(3, 7203).await.unwrap();
    
    // Give cluster time to elect a leader
    sleep(Duration::from_secs(2)).await;
    
    // Make a gRPC request to create a VM
    let addr = format!("http://127.0.0.1:{}", 7201);
    let mut client = BlixardServiceClient::connect(addr).await.unwrap();
    
    // Create root span for the test
    let root_span = tracing::span!(
        tracing::Level::INFO,
        "test_vm_create",
        test.name = "distributed_tracing_test"
    );
    let _enter = root_span.enter();
    
    // Make request - trace context should be propagated
    let request = tonic::Request::new(CreateVmRequest {
        name: "test-traced-vm".to_string(),
        image: "test-image".to_string(),
        cpus: 2,
        memory: 1024,
        disk: 10,
    });
    
    let response = client.create_vm(request).await.unwrap();
    assert!(response.into_inner().success);
    
    // The trace should have propagated through:
    // 1. Client -> gRPC server (join_cluster span)
    // 2. gRPC server -> Raft proposal
    // 3. Raft -> Storage operations
    // 4. Raft -> VM backend operations
    
    // In a real test with an actual collector, we would verify the trace
    // For now, we just ensure the code paths execute without errors
}

#[tokio::test]
async fn test_trace_context_injection_extraction() {
    use blixard_core::proto::HealthCheckRequest;
    
    // Create a test request
    let mut request = tonic::Request::new(HealthCheckRequest {});
    
    // Create a span
    let span = tracing::span!(tracing::Level::INFO, "test_injection");
    let _guard = span.enter();
    
    // Inject context into request
    tracing_otel::inject_context(&mut request);
    
    // Extract context from request
    let extracted_context = tracing_otel::extract_context(&request);
    
    // The extracted context should be valid (though it might be empty in noop mode)
    let _guard2 = extracted_context.attach();
    
    // Should not panic
    tracing::info!("Context injection and extraction successful");
}

#[tokio::test]
async fn test_span_attributes() {
    let _ = tracing_otel::init_noop();
    
    // Create different types of spans
    let grpc_span = tracing_otel::grpc_span("test_method", opentelemetry::trace::SpanKind::Server);
    let _guard1 = grpc_span.enter();
    
    // Add attributes
    tracing_otel::add_attributes(&[
        ("test.attribute", &"value"),
        ("test.number", &42),
    ]);
    
    drop(_guard1);
    
    let storage_span = tracing_otel::storage_span("read", "test_table");
    let _guard2 = storage_span.enter();
    
    // Record an error
    let test_error = std::io::Error::new(std::io::ErrorKind::Other, "test error");
    tracing_otel::record_error(&test_error);
    
    drop(_guard2);
    
    let raft_span = tracing_otel::raft_span("propose");
    let _guard3 = raft_span.enter();
    
    tracing::info!("Raft operation in progress");
}

#[tokio::test]
async fn test_metrics_and_tracing_together() {
    // Initialize both metrics and tracing
    let _ = metrics_otel_v2::init_noop();
    let _ = tracing_otel::init_noop();
    
    // Create a span
    let span = tracing::span!(
        tracing::Level::INFO,
        "test_operation",
        operation = "test"
    );
    let _guard = span.enter();
    
    // Record metrics within the span
    let metrics = metrics_otel_v2::metrics();
    metrics.grpc_requests_total.add(1, &[
        metrics_otel_v2::attributes::method("test_method"),
    ]);
    
    // Use timer with span context
    {
        let _timer = metrics_otel_v2::Timer::new(metrics.grpc_request_duration.clone());
        
        // Simulate some work
        sleep(Duration::from_millis(10)).await;
        
        // Timer records duration when dropped
    }
    
    // Both metrics and traces should be recorded
    tracing::info!("Operation completed");
}