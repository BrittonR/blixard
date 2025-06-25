//! Tests for observability and tracing

use blixard_core::observability::{self, raft_span, storage_span, grpc_span, vm_span, record_duration};
use std::time::Instant;
use tracing::{info, debug};
use tracing_test::traced_test;

/// Test span creation
#[traced_test]
#[test]
fn test_span_creation() {
    // Raft span
    {
        let _span = raft_span("propose", 1).entered();
        info!("Test raft operation");
    }
    assert!(logs_contain("raft"));
    assert!(logs_contain("operation=propose"));
    assert!(logs_contain("node_id=1"));
    
    // Storage span
    {
        let _span = storage_span("write", "vm_state").entered();
        debug!("Test storage operation");
    }
    assert!(logs_contain("storage"));
    assert!(logs_contain("operation=write"));
    assert!(logs_contain("table=vm_state"));
    
    // gRPC span
    {
        let _span = grpc_span("SubmitTask", Some(2)).entered();
        info!("Test gRPC call");
    }
    assert!(logs_contain("grpc"));
    assert!(logs_contain("method=SubmitTask"));
    assert!(logs_contain("peer=2"));
    
    // VM span
    {
        let _span = vm_span("create", "test-vm").entered();
        info!("Test VM operation");
    }
    assert!(logs_contain("vm"));
    assert!(logs_contain("operation=create"));
    assert!(logs_contain("vm_name=test-vm"));
}

/// Test duration recording
#[traced_test]
#[tokio::test]
async fn test_duration_recording() {
    let span = tracing::info_span!("test_operation", duration_ms = tracing::field::Empty);
    let _guard = span.enter();
    
    let start = Instant::now();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    record_duration(start);
    
    info!("Operation completed");
    
    // The span should have recorded the duration
    assert!(logs_contain("test_operation"));
    assert!(logs_contain("duration_ms="));
}

/// Test instrumented function
#[traced_test]
#[tokio::test]
async fn test_instrumented_function() {
    let result = observability::example_instrumented_function("test_op", b"hello world").await;
    assert!(result.is_ok());
    
    // Check logs
    assert!(logs_contain("example_instrumented_function"));
    assert!(logs_contain("operation=\"test_op\""));
    assert!(logs_contain("data_len=11"));
    assert!(logs_contain("duration_ms="));
    
    // Test with empty data
    let result = observability::example_instrumented_function("empty_op", b"").await;
    assert!(result.is_err());
}

/// Test nested spans
#[traced_test]
#[tokio::test]
async fn test_nested_spans() {
    let outer_span = grpc_span("CreateVm", None);
    let _outer = outer_span.enter();
    
    info!("Starting VM creation");
    
    {
        let inner_span = vm_span("allocate", "new-vm");
        let _inner = inner_span.enter();
        
        info!("Allocating resources");
        
        {
            let storage_span = storage_span("write", "vm_state");
            let _storage = storage_span.enter();
            
            debug!("Writing to storage");
        }
    }
    
    info!("VM creation complete");
    
    // All spans should be in the logs
    assert!(logs_contain("grpc"));
    assert!(logs_contain("vm"));
    assert!(logs_contain("storage"));
}

/// Test log levels
#[traced_test]
#[test]
fn test_log_levels() {
    use blixard_core::observability::log_levels;
    
    tracing::event!(log_levels::RAFT_DEBUG, "Raft debug message");
    tracing::event!(log_levels::RAFT_INFO, "Raft info message");
    tracing::event!(log_levels::STORAGE, "Storage message");
    tracing::event!(log_levels::GRPC, "gRPC message");
    tracing::event!(log_levels::VM, "VM message");
    tracing::event!(log_levels::PEER, "Peer message");
    tracing::event!(log_levels::METRICS, "Metrics message");
    
    // Different levels should produce different output
    assert!(logs_contain("Raft debug message"));
    assert!(logs_contain("Raft info message"));
    assert!(logs_contain("Storage message"));
}