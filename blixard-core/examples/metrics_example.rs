//! Example of using OpenTelemetry metrics in Blixard
//!
//! This example shows how to:
//! - Initialize metrics with Prometheus exporter
//! - Use metrics in application code
//! - Export metrics for monitoring

use blixard_core::metrics_otel::{self, metrics, Timer, attributes};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Initialize metrics with Prometheus exporter
    println!("Initializing metrics with Prometheus exporter");
    metrics_otel::init_prometheus()?;
    
    // Get metrics instance
    let metrics = metrics();
    
    // Simulate some operations
    println!("Simulating Raft operations...");
    
    for i in 0..100 {
        // Simulate a Raft proposal
        {
            let _timer = Timer::with_attributes(
                metrics.raft_proposal_duration.clone(),
                vec![
                    attributes::node_id(1),
                    attributes::operation("create_vm"),
                ],
            );
            
            metrics.raft_proposals_total.add(1, &[attributes::node_id(1)]);
            
            // Simulate processing time
            sleep(Duration::from_millis(10)).await;
            
            // Randomly fail some proposals
            if i % 7 == 0 {
                metrics.raft_proposals_failed.add(1, &[
                    attributes::node_id(1),
                    attributes::error(true),
                ]);
            }
        }
        
        // Update Raft state
        metrics.raft_term.add(1, &[attributes::node_id(1)]);
        metrics.raft_commit_index.add(1, &[attributes::node_id(1)]);
        metrics.raft_applied_entries.add(1, &[attributes::node_id(1)]);
        
        // Simulate peer connections
        if i % 5 == 0 {
            metrics.peer_reconnect_attempts.add(1, &[attributes::peer_id(2)]);
            metrics.peer_connections_active.add(1, &[]);
        }
        
        // Simulate VM operations
        if i % 10 == 0 {
            let _timer = Timer::with_attributes(
                metrics.vm_create_duration.clone(),
                vec![attributes::vm_name(&format!("vm-{}", i))],
            );
            
            metrics.vm_create_total.add(1, &[]);
            metrics.vm_total.add(1, &[]);
            metrics.vm_running.add(1, &[]);
            
            sleep(Duration::from_millis(50)).await;
        }
        
        // Simulate more storage operations
        if i % 3 == 0 {
            let _timer = Timer::with_attributes(
                metrics.storage_read_duration.clone(),
                vec![attributes::table("cluster_state")],
            );
            
            metrics.storage_reads.add(1, &[attributes::table("cluster_state")]);
            sleep(Duration::from_millis(3)).await;
        }
        
        // Simulate storage operations
        {
            let _timer = Timer::with_attributes(
                metrics.storage_write_duration.clone(),
                vec![attributes::table("vm_state")],
            );
            
            metrics.storage_writes.add(1, &[attributes::table("vm_state")]);
            sleep(Duration::from_millis(5)).await;
        }
        
        if i % 10 == 0 {
            println!("Processed {} operations", i);
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    // Create a simple HTTP server to serve metrics
    println!("\nStarting metrics server on http://0.0.0.0:9090/metrics");
    
    use std::net::SocketAddr;
    
    use hyper::{Body, Request, Response, Server};
    use hyper::service::{make_service_fn, service_fn};
    
    async fn serve_metrics(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        let metrics_text = metrics_otel::prometheus_metrics();
        Ok(Response::new(Body::from(metrics_text)))
    }
    
    let addr = SocketAddr::from(([0, 0, 0, 0], 9090));
    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, hyper::Error>(service_fn(serve_metrics))
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    
    println!("Metrics server running at http://0.0.0.0:9090/metrics");
    println!("You can query them with:");
    println!("  curl http://localhost:9090/metrics");
    println!("\nOr view in Prometheus by adding this target to prometheus.yml:");
    println!("  - targets: ['localhost:9090']");
    
    // Run the server
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
    
    Ok(())
}

/// Example of using metrics in a struct
struct RaftNode {
    id: u64,
}

impl RaftNode {
    async fn propose(&self, data: &[u8]) -> Result<(), String> {
        let metrics = metrics();
        let attrs = vec![
            attributes::node_id(self.id),
            attributes::operation("propose"),
        ];
        
        let _timer = Timer::with_attributes(
            metrics.raft_proposal_duration.clone(),
            attrs.clone(),
        );
        
        metrics.raft_proposals_total.add(1, &attrs);
        
        // Simulate proposal processing
        sleep(Duration::from_millis(20)).await;
        
        if data.is_empty() {
            metrics.raft_proposals_failed.add(1, &attrs);
            Err("Empty proposal".to_string())
        } else {
            Ok(())
        }
    }
}