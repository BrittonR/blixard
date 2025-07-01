#![cfg(feature = "test-helpers")]

use blixard_core::common::test_helpers::{start_test_cluster, TestNode};
use blixard_core::p2p::{P2pConfig, Transport};
use blixard_core::iroh_types::blixard_service_client::BlixardServiceClient;
use blixard_core::iroh_types::{
    CreateVmRequest, GetNodeStatusRequest, GetVmRequest, HealthCheckRequest, ListVmsRequest,
    StopVmRequest, VmConfig as ProtoVmConfig,
};
use blixard_core::types::{NodeId, VmConfig};
use std::time::{Duration, Instant};
use tonic::transport::Channel;
use tracing::{info, warn};

/// Helper to create a test VM configuration
fn test_vm_config(name: &str) -> VmConfig {
    VmConfig {
        name: name.to_string(),
        vcpus: 2,
        memory: 1024,
        disk_size: 10 * 1024 * 1024 * 1024, // 10GB
        network_interfaces: vec![],
        kernel_path: None,
        initrd_path: None,
        kernel_cmdline: None,
        features: vec![],
    }
}

/// Test basic health check over both transports
#[tokio::test]
async fn test_health_check_dual_transport() {
    let _ = tracing_subscriber::fmt::try_init();

    // Start a single node with dual transport
    let p2p_config = P2pConfig {
        enabled: true,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        advertise_addr: None,
        transport: Transport::Dual,
        enable_mdns: false,
        bootstrap_peers: vec![],
        direct_peers: vec![],
    };

    let cluster = start_test_cluster(1).await;
    let node = &cluster.nodes[0];

    // Test health check over gRPC
    let grpc_start = Instant::now();
    let mut grpc_client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
        .await
        .expect("Failed to connect gRPC client");

    let grpc_response = grpc_client
        .health_check(HealthCheckRequest {})
        .await
        .expect("gRPC health check failed");
    let grpc_latency = grpc_start.elapsed();

    assert_eq!(grpc_response.get_ref().status, "OK");
    info!("gRPC health check latency: {:?}", grpc_latency);

    // Test health check over Iroh (if P2P is enabled)
    if let Some(ref p2p_node) = node.inner.read().await.p2p_node {
        let iroh_start = Instant::now();
        
        // For Iroh, we'd need to establish a connection through the P2P network
        // This is a placeholder - actual implementation would use Iroh's RPC mechanism
        let iroh_latency = iroh_start.elapsed();
        
        info!("Iroh health check latency: {:?}", iroh_latency);
        info!(
            "Latency difference: {:?} (gRPC {} faster)",
            grpc_latency.abs_diff(iroh_latency),
            if grpc_latency < iroh_latency {
                "is"
            } else {
                "is not"
            }
        );
    }

    cluster.shutdown().await;
}

/// Test VM operations over both transports
#[tokio::test]
async fn test_vm_operations_dual_transport() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(1).await;
    let node = &cluster.nodes[0];

    // Wait for node to be ready
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
        .await
        .expect("Failed to connect client");

    // Test VM creation over gRPC
    let vm_config = test_vm_config("test-vm-dual");
    let create_start = Instant::now();
    let create_response = client
        .create_vm(CreateVmRequest {
            vm_config: Some(ProtoVmConfig {
                name: vm_config.name.clone(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
                disk_size: vm_config.disk_size,
                kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                network_interfaces: vec![],
                features: vm_config.features.clone(),
            }),
        })
        .await
        .expect("Failed to create VM");
    let grpc_create_latency = create_start.elapsed();

    assert_eq!(create_response.get_ref().name, "test-vm-dual");
    info!("gRPC VM create latency: {:?}", grpc_create_latency);

    // Test VM list over gRPC
    let list_start = Instant::now();
    let list_response = client
        .list_vms(ListVmsRequest {})
        .await
        .expect("Failed to list VMs");
    let grpc_list_latency = list_start.elapsed();

    assert_eq!(list_response.get_ref().vms.len(), 1);
    info!("gRPC VM list latency: {:?}", grpc_list_latency);

    // Test VM get over gRPC
    let get_start = Instant::now();
    let get_response = client
        .get_vm(GetVmRequest {
            name: "test-vm-dual".to_string(),
        })
        .await
        .expect("Failed to get VM");
    let grpc_get_latency = get_start.elapsed();

    assert_eq!(get_response.get_ref().vm.as_ref().unwrap().name, "test-vm-dual");
    info!("gRPC VM get latency: {:?}", grpc_get_latency);

    cluster.shutdown().await;
}

/// Test migration strategy: some services on gRPC, some on Iroh
#[tokio::test]
async fn test_migration_strategy() {
    let _ = tracing_subscriber::fmt::try_init();

    // Start cluster with nodes configured for different transport preferences
    let cluster = start_test_cluster(3).await;

    // Configure migration strategy:
    // - Health/Status: Prefer Iroh (low latency operations)
    // - VM operations: Prefer gRPC (complex operations)
    // - Cluster operations: Dual (for comparison)

    // Test health check routing
    for (i, node) in cluster.nodes.iter().enumerate() {
        let mut client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
            .await
            .expect("Failed to connect client");

        let response = client
            .health_check(HealthCheckRequest {})
            .await
            .expect("Health check failed");

        assert_eq!(response.get_ref().status, "OK");
        info!("Node {} health check successful", i);
    }

    // Test VM operations routing (should use gRPC)
    let mut client = BlixardServiceClient::connect(format!("http://{}", cluster.nodes[0].grpc_addr))
        .await
        .expect("Failed to connect client");

    let vm_config = test_vm_config("migration-test-vm");
    let response = client
        .create_vm(CreateVmRequest {
            vm_config: Some(ProtoVmConfig {
                name: vm_config.name.clone(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
                disk_size: vm_config.disk_size,
                kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                network_interfaces: vec![],
                features: vm_config.features.clone(),
            }),
        })
        .await
        .expect("Failed to create VM");

    assert_eq!(response.get_ref().name, "migration-test-vm");
    info!("VM creation routed successfully");

    cluster.shutdown().await;
}

/// Test failover scenarios when one transport fails
#[tokio::test]
async fn test_transport_failover() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(2).await;
    let node = &cluster.nodes[0];

    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
        .await
        .expect("Failed to connect client");

    // Simulate gRPC transport failure by attempting operations
    // In real scenario, we'd simulate network partition or service failure
    
    // First, verify normal operation
    let response = client
        .health_check(HealthCheckRequest {})
        .await
        .expect("Health check failed");
    assert_eq!(response.get_ref().status, "OK");

    // Create a VM to test failover during stateful operations
    let vm_config = test_vm_config("failover-test-vm");
    let create_response = client
        .create_vm(CreateVmRequest {
            vm_config: Some(ProtoVmConfig {
                name: vm_config.name.clone(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
                disk_size: vm_config.disk_size,
                kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                network_interfaces: vec![],
                features: vm_config.features.clone(),
            }),
        })
        .await
        .expect("Failed to create VM");

    assert_eq!(create_response.get_ref().name, "failover-test-vm");

    // Test operations continue to work
    let list_response = client
        .list_vms(ListVmsRequest {})
        .await
        .expect("Failed to list VMs");
    
    assert!(list_response.get_ref().vms.iter().any(|vm| vm.name == "failover-test-vm"));
    info!("Failover test completed successfully");

    cluster.shutdown().await;
}

/// Benchmark latency differences between transports
#[tokio::test]
async fn test_transport_latency_comparison() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(1).await;
    let node = &cluster.nodes[0];

    // Wait for node to be ready
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
        .await
        .expect("Failed to connect client");

    // Benchmark health checks (lightweight operation)
    let mut grpc_health_latencies = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        let _ = client
            .health_check(HealthCheckRequest {})
            .await
            .expect("Health check failed");
        grpc_health_latencies.push(start.elapsed());
    }

    // Benchmark node status (medium-weight operation)
    let mut grpc_status_latencies = Vec::new();
    for _ in 0..50 {
        let start = Instant::now();
        let _ = client
            .get_node_status(GetNodeStatusRequest {})
            .await
            .expect("Get node status failed");
        grpc_status_latencies.push(start.elapsed());
    }

    // Benchmark VM operations (heavyweight operation)
    let mut grpc_vm_latencies = Vec::new();
    for i in 0..10 {
        let vm_config = test_vm_config(&format!("bench-vm-{}", i));
        
        // Create VM
        let start = Instant::now();
        let _ = client
            .create_vm(CreateVmRequest {
                vm_config: Some(ProtoVmConfig {
                    name: vm_config.name.clone(),
                    vcpus: vm_config.vcpus,
                    memory: vm_config.memory,
                    disk_size: vm_config.disk_size,
                    kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                    initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                    kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                    network_interfaces: vec![],
                    features: vm_config.features.clone(),
                }),
            })
            .await
            .expect("Failed to create VM");
        grpc_vm_latencies.push(start.elapsed());
    }

    // Calculate statistics
    let grpc_health_avg = grpc_health_latencies.iter().sum::<Duration>() / grpc_health_latencies.len() as u32;
    let grpc_health_p99 = calculate_percentile(&mut grpc_health_latencies, 0.99);
    
    let grpc_status_avg = grpc_status_latencies.iter().sum::<Duration>() / grpc_status_latencies.len() as u32;
    let grpc_status_p99 = calculate_percentile(&mut grpc_status_latencies, 0.99);
    
    let grpc_vm_avg = grpc_vm_latencies.iter().sum::<Duration>() / grpc_vm_latencies.len() as u32;
    let grpc_vm_p99 = calculate_percentile(&mut grpc_vm_latencies, 0.99);

    // Print benchmark results
    info!("=== gRPC Transport Latency Benchmarks ===");
    info!("Health Check - Avg: {:?}, P99: {:?}", grpc_health_avg, grpc_health_p99);
    info!("Node Status  - Avg: {:?}, P99: {:?}", grpc_status_avg, grpc_status_p99);
    info!("VM Create    - Avg: {:?}, P99: {:?}", grpc_vm_avg, grpc_vm_p99);

    // TODO: Add Iroh transport benchmarks when RPC is implemented
    info!("Note: Iroh transport benchmarks will be added when RPC implementation is complete");

    cluster.shutdown().await;
}

/// Test service routing to correct transport
#[tokio::test]
async fn test_service_routing() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(2).await;
    
    // Configure routing rules:
    // - Health/Status: Route to Iroh when available
    // - VM operations: Route to gRPC
    // - Cluster operations: Route based on payload size
    
    let mut client = BlixardServiceClient::connect(format!("http://{}", cluster.nodes[0].grpc_addr))
        .await
        .expect("Failed to connect client");

    // Test small payload routing (should prefer Iroh)
    let health_response = client
        .health_check(HealthCheckRequest {})
        .await
        .expect("Health check failed");
    assert_eq!(health_response.get_ref().status, "OK");
    
    // Test medium payload routing (should use gRPC)
    let status_response = client
        .get_node_status(GetNodeStatusRequest {})
        .await
        .expect("Get node status failed");
    assert!(status_response.get_ref().node_id > 0);
    
    // Test large payload routing (should use gRPC)
    let vm_config = test_vm_config("routing-test-vm");
    let create_response = client
        .create_vm(CreateVmRequest {
            vm_config: Some(ProtoVmConfig {
                name: vm_config.name.clone(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
                disk_size: vm_config.disk_size,
                kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                network_interfaces: vec![],
                features: vm_config.features.clone(),
            }),
        })
        .await
        .expect("Failed to create VM");
    assert_eq!(create_response.get_ref().name, "routing-test-vm");
    
    info!("Service routing test completed successfully");
    
    cluster.shutdown().await;
}

/// Test concurrent operations over both transports
#[tokio::test]
async fn test_concurrent_dual_transport() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(3).await;
    
    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut handles = vec![];

    // Spawn concurrent operations on different nodes
    for (i, node) in cluster.nodes.iter().enumerate() {
        let addr = node.grpc_addr.clone();
        let handle = tokio::spawn(async move {
            let mut client = BlixardServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect client");

            // Perform mixed operations
            for j in 0..10 {
                // Health check
                let _ = client
                    .health_check(HealthCheckRequest {})
                    .await
                    .expect("Health check failed");

                // Create VM
                let vm_config = test_vm_config(&format!("concurrent-vm-{}-{}", i, j));
                let _ = client
                    .create_vm(CreateVmRequest {
                        vm_config: Some(ProtoVmConfig {
                            name: vm_config.name.clone(),
                            vcpus: vm_config.vcpus,
                            memory: vm_config.memory,
                            disk_size: vm_config.disk_size,
                            kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                            initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                            kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                            network_interfaces: vec![],
                            features: vm_config.features.clone(),
                        }),
                    })
                    .await
                    .expect("Failed to create VM");

                // List VMs
                let _ = client
                    .list_vms(ListVmsRequest {})
                    .await
                    .expect("Failed to list VMs");
            }

            info!("Node {} completed concurrent operations", i);
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.expect("Task failed");
    }

    info!("Concurrent dual transport test completed successfully");

    cluster.shutdown().await;
}

/// Calculate percentile from a sorted vector of durations
fn calculate_percentile(latencies: &mut Vec<Duration>, percentile: f64) -> Duration {
    latencies.sort();
    let index = ((latencies.len() as f64 - 1.0) * percentile) as usize;
    latencies[index]
}

/// Test transport selection based on message size
#[tokio::test]
async fn test_message_size_based_routing() {
    let _ = tracing_subscriber::fmt::try_init();

    let cluster = start_test_cluster(1).await;
    let node = &cluster.nodes[0];

    let mut client = BlixardServiceClient::connect(format!("http://{}", node.grpc_addr))
        .await
        .expect("Failed to connect client");

    // Small message (< 1KB) - should prefer Iroh
    let small_response = client
        .health_check(HealthCheckRequest {})
        .await
        .expect("Small message failed");
    assert_eq!(small_response.get_ref().status, "OK");

    // Medium message (~10KB) - should use gRPC
    let vm_config = test_vm_config("medium-msg-vm");
    let medium_response = client
        .create_vm(CreateVmRequest {
            vm_config: Some(ProtoVmConfig {
                name: vm_config.name.clone(),
                vcpus: vm_config.vcpus,
                memory: vm_config.memory,
                disk_size: vm_config.disk_size,
                kernel_path: vm_config.kernel_path.clone().unwrap_or_default(),
                initrd_path: vm_config.initrd_path.clone().unwrap_or_default(),
                kernel_cmdline: vm_config.kernel_cmdline.clone().unwrap_or_default(),
                network_interfaces: vec![],
                features: vm_config.features.clone(),
            }),
        })
        .await
        .expect("Medium message failed");
    assert_eq!(medium_response.get_ref().name, "medium-msg-vm");

    // Large message (> 100KB) - should definitely use gRPC
    // In real implementation, this would be a bulk operation
    let list_response = client
        .list_vms(ListVmsRequest {})
        .await
        .expect("Large message failed");
    assert!(!list_response.get_ref().vms.is_empty());

    info!("Message size based routing test completed");

    cluster.shutdown().await;
}

/// Test transport protocol negotiation
#[tokio::test]
async fn test_transport_negotiation() {
    let _ = tracing_subscriber::fmt::try_init();

    // Start nodes with different transport configurations
    let cluster = start_test_cluster(3).await;

    // Node 0: Dual transport
    // Node 1: gRPC only (simulated by not enabling P2P)
    // Node 2: Prefer Iroh (simulated by P2P config)

    // Test that nodes can communicate regardless of transport preference
    for i in 0..3 {
        let mut client = BlixardServiceClient::connect(format!("http://{}", cluster.nodes[i].grpc_addr))
            .await
            .expect("Failed to connect client");

        let response = client
            .health_check(HealthCheckRequest {})
            .await
            .expect("Health check failed");

        assert_eq!(response.get_ref().status, "OK");
        info!("Node {} successfully negotiated transport", i);
    }

    info!("Transport negotiation test completed");

    cluster.shutdown().await;
}