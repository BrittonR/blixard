//! Demonstration of Iroh transport in a cluster scenario
//!
//! This example shows how to:
//! 1. Start nodes with Iroh transport
//! 2. Form a cluster
//! 3. Send various types of messages
//! 4. Monitor performance

use blixard_core::error::BlixardResult;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::p2p_manager::{PeerInfo, ConnectionQuality};
use blixard_core::transport::config::IrohConfig;
use blixard_core::transport::iroh_health_service::IrohHealthService;
use blixard_core::transport::iroh_service::{IrohRpcClient, IrohRpcServer};
use blixard_core::transport::iroh_status_service::IrohStatusService;
use blixard_core::transport::iroh_health_service::HealthCheckResponse;
use blixard_core::transport::iroh_status_service::ClusterStatusResponse;
use blixard_core::types::NodeConfig;
use iroh::{Endpoint, NodeAddr, SecretKey};
use rand;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use serde_json;
use std::collections::HashMap;

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
    for (_, endpoint, _server_handle) in nodes {
        endpoint.close().await;
        // Server tasks will be cancelled when endpoints close
    }

    println!("\n✅ Demo completed successfully!");
    Ok(())
}

async fn create_cluster_nodes(
    count: usize,
) -> BlixardResult<Vec<(Arc<SharedNodeState>, Endpoint, JoinHandle<BlixardResult<()>>)>> {
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
            topology: blixard_core::types::NodeTopology::default(),
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
        let server = Arc::new(IrohRpcServer::new(endpoint.clone()));

        // Register services
        let health_service = IrohHealthService::new(node.clone());
        server.register_service(health_service).await;

        let status_service = IrohStatusService::new(node.clone());
        server.register_service(status_service).await;

        // Start accepting connections in background
        let server_handle = {
            let server = server.clone();
            tokio::spawn(async move { server.serve().await })
        };

        nodes.push((node.clone(), endpoint, server_handle));
    }

    // Share node addresses among all nodes
    println!("\nSharing node addresses...");
    for (i, (node, _, _)) in nodes.iter().enumerate() {
        for (peer_id, peer_addr) in &node_addrs {
            if *peer_id != (i + 1) as u64 {
                let peer_info = PeerInfo {
                    node_id: peer_id.to_string(),
                    address: format!("127.0.0.1:{}", 7000 + peer_id),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec!["health".to_string(), "status".to_string()],
                    shared_resources: HashMap::new(),
                    connection_quality: ConnectionQuality {
                        latency_ms: 10,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    p2p_node_id: Some(peer_addr.node_id.to_string()),
                    p2p_addresses: peer_addr.direct_addresses.iter().map(|a| a.to_string()).collect(),
                    p2p_relay_url: peer_addr.relay_url.as_ref().map(|u| u.to_string()),
                    is_connected: false,
                };
                node.add_peer(*peer_id, peer_info).await;
            }
        }
    }

    println!("\nCluster formation complete!");
    Ok(nodes)
}

async fn test_health_checks(
    nodes: &[(Arc<SharedNodeState>, Endpoint, JoinHandle<BlixardResult<()>>)],
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
        let request = serde_json::json!({});
        match client
            .call::<serde_json::Value, HealthCheckResponse>(
                target_addr, 
                "health", 
                "check", 
                request
            )
            .await
        {
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
        }
    }

    Ok(())
}

async fn test_status_queries(
    nodes: &[(Arc<SharedNodeState>, Endpoint, JoinHandle<BlixardResult<()>>)],
) -> BlixardResult<()> {
    println!("\n--- Testing Status Queries ---");

    // Each node queries cluster status
    for (i, (_, endpoint, _)) in nodes.iter().enumerate() {
        // Connect to any other node
        let target_idx = (i + 1) % nodes.len();
        let (_, target_endpoint, _) = &nodes[target_idx];
        let target_addr = NodeAddr::new(target_endpoint.node_id());

        let client = IrohRpcClient::new(endpoint.clone());

        let request = serde_json::json!({});
        match client
            .call::<serde_json::Value, ClusterStatusResponse>(
                target_addr,
                "status",
                "get_cluster_status",
                request,
            )
            .await
        {
            Ok(status) => {
                println!(
                    "  Node {} sees {} cluster members",
                    i + 1,
                    status.member_ids.len()
                );
            }
            Err(e) => println!("  Status query failed: {}", e),
        }
    }

    Ok(())
}

async fn test_message_performance(
    nodes: &[(Arc<SharedNodeState>, Endpoint, JoinHandle<BlixardResult<()>>)],
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
    let request = serde_json::json!({});
    let _ = client
        .call::<serde_json::Value, HealthCheckResponse>(
            target_addr.clone(),
            "health",
            "check",
            request.clone(),
        )
        .await;

    // Measure multiple health checks
    let mut latencies = Vec::new();

    for _ in 0..100 {
        let start = Instant::now();

        if let Ok(_) = client
            .call::<serde_json::Value, HealthCheckResponse>(
                target_addr.clone(),
                "health",
                "check",
                request.clone(),
            )
            .await
        {
            latencies.push(start.elapsed());
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
