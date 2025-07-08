//! Integration test for OpenTelemetry metrics

use blixard_core::metrics_otel::{self, attributes, metrics, Timer};

#[tokio::test]
async fn test_metrics_recording() {
    // Initialize metrics (or skip if already initialized)
    let _ = metrics_otel::init_prometheus();

    let metrics = metrics();

    // Record some metrics
    metrics
        .raft_proposals_total
        .add(5, &[attributes::node_id(1)]);
    metrics
        .raft_proposals_failed
        .add(2, &[attributes::node_id(1)]);

    // Use timer
    {
        let _timer = Timer::with_attributes(
            metrics.raft_proposal_duration.clone(),
            vec![attributes::node_id(1)],
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Record VM metrics
    metrics.vm_create_total.add(3, &[]);
    metrics.vm_create_failed.add(1, &[]);
    metrics.vm_total.add(2, &[]);
    metrics.vm_running.add(2, &[]);

    // Get Prometheus text output
    let metrics_text = metrics_otel::prometheus_metrics();

    // Verify metrics are present
    assert!(metrics_text.contains("raft_proposals_total"));
    assert!(metrics_text.contains("raft_proposals_failed"));
    assert!(metrics_text.contains("raft_proposal_duration"));
    assert!(metrics_text.contains("vm_create_total"));
    assert!(metrics_text.contains("vm_create_failed"));
    assert!(metrics_text.contains("vm_total"));
    assert!(metrics_text.contains("vm_running"));

    // Verify some values
    assert!(metrics_text.contains("node_id=\"1\""));
}

#[test]
fn test_metrics_attributes() {
    // Test that attribute helper functions work correctly
    let node_attr = attributes::node_id(42);
    assert_eq!(node_attr.key.to_string(), "node.id");
    assert_eq!(
        format!("{:?}", node_attr.value),
        format!("{:?}", opentelemetry::Value::I64(42))
    );

    let vm_attr = attributes::vm_name("test-vm");
    assert_eq!(vm_attr.key.to_string(), "vm.name");

    let table_attr = attributes::table("vm_state");
    assert_eq!(table_attr.key.to_string(), "table");

    let status_attr = attributes::status("success");
    assert_eq!(status_attr.key.to_string(), "status");

    let error_attr = attributes::error(true);
    assert_eq!(error_attr.key.to_string(), "error");
}
