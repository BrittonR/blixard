//! Example demonstrating Raft batch processing
//!
//! This example shows how batch processing improves throughput by:
//! - Grouping multiple proposals together
//! - Reducing Raft consensus overhead
//! - Improving overall cluster performance

use blixard_core::{
    node::Node,
    types::{NodeConfig, VmCommand, VmConfig},
    config_v2::{Config, RaftBatchConfig},
    config_global,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
                .add_directive("raft_batch_demo=info".parse()?)
        )
        .init();

    info!("Starting Raft batch processing demo");
    
    // Create temporary directory for node data
    let temp_dir = TempDir::new()?;
    
    // Test with batching disabled first
    info!("\n=== Test 1: Batching Disabled ===");
    let mut config = Config::default();
    config.cluster.raft.batch_processing.enabled = false;
    config_global::set(Arc::new(config));
    
    let disabled_time = run_test(temp_dir.path().join("disabled")).await?;
    
    // Test with batching enabled
    info!("\n=== Test 2: Batching Enabled ===");
    let mut config = Config::default();
    config.cluster.raft.batch_processing = RaftBatchConfig {
        enabled: true,
        max_batch_size: 50,
        batch_timeout_ms: 5,
        max_batch_bytes: 512 * 1024, // 512KB
    };
    config_global::set(Arc::new(config));
    
    let enabled_time = run_test(temp_dir.path().join("enabled")).await?;
    
    // Compare results
    info!("\n=== Results ===");
    info!("Without batching: {:?}", disabled_time);
    info!("With batching: {:?}", enabled_time);
    
    let improvement = ((disabled_time.as_secs_f64() - enabled_time.as_secs_f64()) 
        / disabled_time.as_secs_f64()) * 100.0;
    
    if improvement > 0.0 {
        info!("Performance improvement: {:.1}%", improvement);
    } else {
        info!("Performance difference: {:.1}%", improvement.abs());
    }
    
    info!("\nBatch processing demo completed!");
    Ok(())
}

async fn run_test(data_dir: std::path::PathBuf) -> Result<Duration, Box<dyn std::error::Error>> {
    // Create a single node cluster
    let node_config = NodeConfig {
        id: 1,
        bind_address: "127.0.0.1:0".to_string(), // Random port
        data_dir: data_dir.to_string_lossy().to_string(),
        bootstrap: true,
        transport_config: None,
    };
    
    let mut node = Node::new(node_config);
    node.initialize().await?;
    let node_shared = node.shared();
    
    // Start the node
    let node_arc = Arc::new(node);
    let node_handle = node_arc.clone().start();
    
    // Wait for node to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Measure time to create many VMs
    let num_vms = 100;
    let start = Instant::now();
    
    // Submit many VM creation requests rapidly
    let mut handles = vec![];
    for i in 0..num_vms {
        let shared = node_shared.clone();
        let handle = tokio::spawn(async move {
            let vm_config = VmConfig {
                name: format!("test-vm-{}", i),
                vcpus: 1,
                memory_mb: 512,
                disk_gb: 10,
                image: "test-image".to_string(),
                network: vec![],
                user_data: None,
                metadata: Default::default(),
            };
            
            let command = VmCommand::Create {
                config: vm_config,
                node_id: 1,
            };
            
            // Submit proposal through shared state
            shared.send_vm_command(command).await
        });
        handles.push(handle);
    }
    
    // Wait for all proposals to complete
    for handle in handles {
        let _ = handle.await?;
    }
    
    let elapsed = start.elapsed();
    
    info!("Created {} VMs in {:?}", num_vms, elapsed);
    info!("Average time per VM: {:?}", elapsed / num_vms);
    
    // Shutdown
    node_arc.stop().await?;
    let _ = tokio::time::timeout(Duration::from_secs(5), node_handle).await;
    
    Ok(elapsed)
}