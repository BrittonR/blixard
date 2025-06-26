//! Test the metrics server functionality

use blixard_core::{
    config_v2::ConfigBuilder,
    config_global,
    metrics_otel_v2::{init_prometheus, metrics},
    metrics_server,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_metrics_server_basic() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize configuration if not already done
    if !config_global::is_initialized() {
        let config = ConfigBuilder::new()
            .node_id(99)
            .bind_address("127.0.0.1:9999")
            .data_dir("./metrics-test-data")
            .build()?;
        config_global::init(config)?;
    }
    
    // Initialize metrics (ignore error if already initialized)
    let _ = init_prometheus();
    
    // Add some test metrics
    metrics().raft_proposals_total.add(5, &[]);
    metrics().grpc_requests_total.add(10, &[]);
    metrics().vm_total.add(3, &[]);
    
    // Start metrics server
    let metrics_addr = "127.0.0.1:0".parse()?; // Use port 0 for auto-assignment
    let handle = metrics_server::spawn_metrics_server(metrics_addr);
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // We can't easily test the actual HTTP endpoint in unit tests
    // because we don't know the assigned port, but we can verify
    // that the server starts without error
    assert!(!handle.is_finished());
    
    // Abort the server
    handle.abort();
    
    Ok(())
}

#[test]
fn test_prometheus_metrics_format() {
    use blixard_core::metrics_otel_v2::prometheus_metrics;
    
    // Initialize metrics if not already done
    let _ = init_prometheus();
    
    // Add some metrics
    metrics().raft_proposals_total.add(10, &[]);
    metrics().raft_proposals_failed.add(2, &[]);
    
    // Get prometheus output
    let output = prometheus_metrics();
    
    // Verify it contains expected metric names
    assert!(output.contains("raft_proposals_total"));
    assert!(output.contains("raft_proposals_failed"));
    
    // Verify it's in Prometheus format (contains TYPE and HELP lines)
    assert!(output.contains("# TYPE"));
    assert!(output.contains("# HELP"));
}