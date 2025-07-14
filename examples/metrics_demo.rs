//! Example demonstrating the Prometheus metrics endpoint

use blixard_core::{
    config_global,
    config::ConfigBuilder,
    metrics_otel::{init_prometheus, safe_metrics},
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Create configuration with metrics enabled
    let config = ConfigBuilder::new()
        .node_id(1)
        .bind_address("127.0.0.1:7001")
        .data_dir("./metrics-demo-data")
        .vm_backend("mock")
        .build()?;

    // Initialize global config
    config_global::init(config)?;

    // Initialize metrics with Prometheus exporter
    init_prometheus()?;
    tracing::info!("Metrics initialized with Prometheus exporter");

    // Generate metrics (in production, would be exported via HTTP server)
    tracing::info!("Metrics will be available at http://127.0.0.1:8001/metrics");

    // Simulate some metric updates
    tracing::info!("Generating sample metrics...");

    // Simulate Raft operations
    for i in 0..10 {
        let metrics = safe_metrics()?;
        metrics.raft_proposals_total.add(1, &[]);
        if i % 3 == 0 {
            metrics.raft_proposals_failed.add(1, &[]);
        }
        metrics.raft_committed_entries.add(1, &[]);
        metrics.raft_applied_entries.add(1, &[]);

        // Record proposal duration
        let duration = 0.001 * (i as f64 + 1.0);
        metrics.raft_proposal_duration.record(duration, &[]);

        sleep(Duration::from_millis(100)).await;
    }

    // Simulate gRPC requests
    for i in 0..20 {
        let metrics = safe_metrics()?;
        metrics.grpc_requests_total.add(1, &[]);
        if i % 5 == 0 {
            metrics.grpc_requests_failed.add(1, &[]);
        }

        // Record request duration
        let duration = 0.005 * (i as f64 % 3.0 + 1.0);
        metrics.grpc_request_duration.record(duration, &[]);

        sleep(Duration::from_millis(50)).await;
    }

    // Update peer connections
    let metrics = safe_metrics()?;
    metrics.peer_connections_active.add(3, &[]);
    sleep(Duration::from_secs(1)).await;
    metrics.peer_connections_active.add(-1, &[]);

    // Simulate VM operations
    metrics.vm_total.add(5, &[]);
    metrics.vm_running.add(3, &[]);
    metrics.vm_create_total.add(5, &[]);
    metrics.vm_create_failed.add(1, &[]);

    tracing::info!("");
    tracing::info!("📊 Metrics Demo Running!");
    tracing::info!("========================");
    tracing::info!("");
    tracing::info!("Open your browser to: http://127.0.0.1:8001/metrics");
    tracing::info!("Or use curl: curl http://127.0.0.1:8001/metrics");
    tracing::info!("");
    tracing::info!("You should see Prometheus metrics like:");
    tracing::info!("  - raft_proposals_total");
    tracing::info!("  - grpc_requests_total");
    tracing::info!("  - vm_running");
    tracing::info!("");
    tracing::info!("Metrics generation completed!");
    
    // Display final metrics
    println!("\n=== Final Metrics Output ===");
    println!("{}", blixard_core::metrics_otel::prometheus_metrics());
    
    Ok(())
}
