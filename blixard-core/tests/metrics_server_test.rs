//! Test the metrics server functionality

use blixard_core::{
    config_global,
    config::ConfigBuilder,
    metrics_otel::{init_prometheus, prometheus_metrics, safe_metrics},
    metrics_server,
    node_shared::SharedNodeState,
};
use std::sync::Arc;
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
    if let Ok(metrics) = safe_metrics() {
        metrics.raft_proposals_total.add(5, &[]);
        metrics.grpc_requests_total.add(10, &[]);
        metrics.vm_total.add(3, &[]);
    }

    // Create a SharedNodeState for the metrics server
    let node_config = blixard_core::types::NodeConfig {
        id: 99,
        data_dir: "./metrics-test-data".to_string(),
        bind_addr: "127.0.0.1:9999".parse()?,
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: blixard_core::types::NodeTopology::default(),
    };
    let shared_state = Arc::new(SharedNodeState::new(node_config));
    
    // Start metrics server
    let metrics_addr = "127.0.0.1:0".parse()?; // Use port 0 for auto-assignment
    let handle = metrics_server::spawn_metrics_server(metrics_addr, shared_state);

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

    // Initialize metrics if not already done
    let _ = init_prometheus();

    // Add some metrics
    if let Ok(metrics) = safe_metrics() {
        metrics.raft_proposals_total.add(10, &[]);
        metrics.raft_proposals_failed.add(2, &[]);
    }

    // Get prometheus output
    let output = prometheus_metrics();

    // Verify it contains expected metric names
    assert!(output.contains("raft_proposals_total"));
    assert!(output.contains("raft_proposals_failed"));

    // Verify it's in Prometheus format (contains TYPE and HELP lines)
    assert!(output.contains("# TYPE"));
    assert!(output.contains("# HELP"));
}
