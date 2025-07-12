//! Demonstration of Iroh transport in a cluster scenario
//!
//! This example shows how to:
//! 1. Start nodes with Iroh transport
//! 2. Form a cluster
//! 3. Send various types of messages
//! 4. Monitor performance

use blixard_core::error::BlixardResult;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::transport::config::{IrohConfig, MigrationStrategy, TransportConfig};
use blixard_core::transport::iroh_health_service::IrohHealthService;
use blixard_core::transport::iroh_service::{IrohRpcClient, IrohRpcServer};
use blixard_core::transport::iroh_status_service::IrohStatusService;
use blixard_core::transport::services::health::{HealthService, HealthServiceImpl};
use blixard_core::transport::services::status::{StatusService, StatusServiceImpl};
use blixard_core::types::NodeConfig;
use iroh::{Endpoint, NodeAddr, SecretKey};
use rand;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,iroh=info")
        .init();

    println!("=== Iroh Cluster Demo ===\n");

    // Create a 3-node cluster
    let nodes = create_cluster_nodes(3).await?;

    // Test various operations
    test_health_checks(&nodes).await?;
    test_status_queries(&nodes).await?;
    test_message_performance(&nodes).await?;

    // Cleanup
    for (_, endpoint, server) in nodes {
        server.shutdown().await;
        endpoint.close().await;
    }

    println!("\n✅ Demo completed successfully!");
    Ok(())
}

async fn create_cluster_nodes(
    count: usize,
) -> BlixardResult<Vec<(Arc<SharedNodeState>, Endpoint, IrohRpcServer)>> {
    println!("Creating {}-node cluster with Iroh transport...\n", count);

    let mut nodes = Vec::new();
    let mut node_addrs = Vec::new();

    for i in 1..=count {
        // Create node configuration
        let config = NodeConfig {
            id: i as u64,
            bind_addr: format!("127.0.0.1:{}", 7000 + i).parse().unwrap(),
            data_dir: std::env::temp_dir()
                .join(format!("blixard-demo-{}", i))
                .to_string_lossy()
                .to_string(),
            vm_backend: "test".to_string(),
            join_addr: if i > 1 {
                Some(format!("127.0.0.1:{}", 7001))
            } else {
                None
            },
            use_tailscale: false,
            transport_config: Some(IrohConfig::default()),
        };

        let node = Arc::new(SharedNodeState::new(config));

        // Create Iroh endpoint
        let secret = SecretKey::generate(rand::thread_rng());
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .bind()
            .await
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to create endpoint: {}", e),
            })?;

        let node_id = endpoint.node_id();
        let node_addr = NodeAddr::new(node_id);
        node_addrs.push((i as u64, node_addr.clone()));

        println!("Node {} started:", i);
        println!("  ID: {}", node.get_id());
        println!("  Iroh NodeId: {}", node_id);

        // Create RPC server
        let server = IrohRpcServer::new(endpoint.clone());

        // Register services
        let health_service = IrohHealthService::new(HealthServiceImpl::new(node.clone()));
        server.register_service("health", Arc::new(health_service));

        let status_service = IrohStatusService::new(StatusServiceImpl::new(node.clone()));
        server.register_service("status", Arc::new(status_service));

        // Start accepting connections
        server.start().await?;

        nodes.push((node.clone(), endpoint, server));
    }

    // Share node addresses among all nodes
    println!("\nSharing node addresses...");
    for (i, (node, _, _)) in nodes.iter().enumerate() {
        for (peer_id, peer_addr) in &node_addrs {
            if *peer_id != (i + 1) as u64 {
                node.add_peer(*peer_id, format!("127.0.0.1:{}", 7000 + peer_id))
                    .await;
                // In a real scenario, we'd also share the Iroh NodeAddr
            }
        }
    }

    println!("\nCluster formation complete!");
    Ok(nodes)
}

async fn test_health_checks(
    nodes: &[(Arc<SharedNodeState>, Endpoint, IrohRpcServer)],
) -> BlixardResult<()> {
    println!("\n--- Testing Health Checks ---");

    // Node 1 checks health of Node 2 and Node 3
    let (_, endpoint1, _) = &nodes[0];

    for i in 1..nodes.len() {
        let (_, endpoint_target, _) = &nodes[i];
        let target_addr = NodeAddr::new(endpoint_target.node_id());

        // Create client
        let client = IrohRpcClient::new(endpoint1.clone());

        // Measure health check latency
        let start = Instant::now();
        match client
            .connect_to_service::<dyn HealthService>(target_addr)
            .await
        {
            Ok(mut health_client) => match health_client.check().await {
                Ok(response) => {
                    let latency = start.elapsed();
                    println!(
                        "  Node 1 → Node {}: Healthy={}, Latency={:?}",
                        i + 1,
                        response.healthy,
                        latency
                    );
                }
                Err(e) => println!("  Health check failed: {}", e),
            },
            Err(e) => println!("  Failed to connect: {}", e),
        }
    }

    Ok(())
}

async fn test_status_queries(
    nodes: &[(Arc<SharedNodeState>, Endpoint, IrohRpcServer)],
) -> BlixardResult<()> {
    println!("\n--- Testing Status Queries ---");

    // Each node queries cluster status
    for (i, (_, endpoint, _)) in nodes.iter().enumerate() {
        // Connect to any other node
        let target_idx = (i + 1) % nodes.len();
        let (_, target_endpoint, _) = &nodes[target_idx];
        let target_addr = NodeAddr::new(target_endpoint.node_id());

        let client = IrohRpcClient::new(endpoint.clone());

        match client
            .connect_to_service::<dyn StatusService>(target_addr)
            .await
        {
            Ok(mut status_client) => match status_client.get_cluster_status().await {
                Ok(status) => {
                    println!(
                        "  Node {} sees {} cluster members",
                        i + 1,
                        status.nodes.len()
                    );
                }
                Err(e) => println!("  Status query failed: {}", e),
            },
            Err(e) => println!("  Failed to connect: {}", e),
        }
    }

    Ok(())
}

async fn test_message_performance(
    nodes: &[(Arc<SharedNodeState>, Endpoint, IrohRpcServer)],
) -> BlixardResult<()> {
    println!("\n--- Testing Message Performance ---");

    if nodes.len() < 2 {
        println!("  Need at least 2 nodes for performance test");
        return Ok(());
    }

    let (_, endpoint1, _) = &nodes[0];
    let (_, endpoint2, _) = &nodes[1];
    let target_addr = NodeAddr::new(endpoint2.node_id());

    // Create client
    let client = IrohRpcClient::new(endpoint1.clone());

    // Warm up connection
    if let Ok(mut health_client) = client
        .connect_to_service::<dyn HealthService>(target_addr.clone())
        .await
    {
        let _ = health_client.check().await;
    }

    // Measure multiple health checks
    let mut latencies = Vec::new();

    for _ in 0..100 {
        let start = Instant::now();

        if let Ok(mut health_client) = client
            .connect_to_service::<dyn HealthService>(target_addr.clone())
            .await
        {
            if let Ok(_) = health_client.check().await {
                latencies.push(start.elapsed());
            }
        }
    }

    if !latencies.is_empty() {
        latencies.sort();
        let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let p50 = latencies[latencies.len() / 2];
        let p99 = latencies[latencies.len() * 99 / 100];

        println!("  Health check performance over Iroh:");
        println!("    Messages: {}", latencies.len());
        println!("    Average: {:?}", avg);
        println!("    P50: {:?}", p50);
        println!("    P99: {:?}", p99);
    }

    Ok(())
}

// Note: This is a simplified demo. In production, you would:
// 1. Use proper service discovery for Iroh NodeAddr sharing
// 2. Implement reconnection logic
// 3. Add comprehensive error handling
// 4. Include metrics collection
// 5. Support configuration hot-reload
