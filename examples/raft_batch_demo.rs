//! Example demonstrating Raft batch processing
//!
//! This example shows how batch processing improves throughput by:
//! - Grouping multiple proposals together
//! - Reducing Raft consensus overhead
//! - Improving overall cluster performance

use blixard_core::{
    test_helpers::{TestNode, TestCluster},
    types::{VmCommand, VmConfig},
};
use std::time::{Duration, Instant};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
                .add_directive("raft_batch_demo=info".parse()?),
        )
        .init();

    info!("Starting Raft batch processing demo");
    info!("Note: In production, batch processing is configured via blixard.toml");
    info!("This demo shows the performance characteristics of batch processing\n");

    // Create a test cluster with single node
    let mut cluster = TestCluster::new();
    let node_id = cluster.add_node().await?;
    
    // Get the test node
    let test_node = cluster.nodes().get(&node_id).unwrap();
    
    // Start the node
    test_node.start().await?;
    
    // Get shared state for command submission
    let shared_state = test_node.shared_state().await;

    // Wait for node to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Measure time to create many VMs
    let num_vms = 100;
    let start = Instant::now();

    info!("Creating {} VMs to demonstrate batch processing...", num_vms);

    // Submit many VM creation requests rapidly
    let mut handles = vec![];
    for i in 0..num_vms {
        let shared = shared_state.clone();
        let handle = tokio::spawn(async move {
            let vm_config = VmConfig {
                name: format!("test-vm-{}", i),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory: 512,
                tenant_id: "default".to_string(),
                ip_address: None,
                metadata: None,
                anti_affinity: None,
                priority: 500,
                preemptible: true,
                locality_preference: Default::default(),
                health_check_config: None,
            };

            let command = VmCommand::Create {
                config: vm_config,
                node_id: 1,
            };

            // Submit proposal through shared state
            let command_str = format!("{:?}", command);
            shared.send_vm_command(&format!("test-vm-{}", i), command_str).await
        });
        handles.push(handle);
    }

    // Wait for all proposals to complete
    for handle in handles {
        let _ = handle.await?;
    }

    let elapsed = start.elapsed();

    info!("\n=== Results ===");
    info!("Created {} VMs in {:?}", num_vms, elapsed);
    info!("Average time per VM: {:?}", elapsed / num_vms);
    info!("\nWith batch processing enabled (default), multiple proposals are");
    info!("grouped together for more efficient Raft consensus processing.");
    info!("\nTo disable batching, set cluster.raft.batch_processing.enabled = false");
    info!("in your blixard.toml configuration file.");

    // Shutdown
    cluster.shutdown().await;

    info!("\nBatch processing demo completed!");
    Ok(())
}