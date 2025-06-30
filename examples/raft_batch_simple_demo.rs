//! Simple example demonstrating Raft batch processing
//!
//! This example shows the basic functionality of the batch processor
//! without needing a full cluster setup.

use blixard_core::{
    raft_batch_processor::{BatchConfig, create_batch_processor},
    raft_manager::{RaftProposal, ProposalData},
    types::VmCommand,
};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
        )
        .init();

    info!("Starting Raft batch processing demo");
    
    // Create channels
    let (raft_tx, mut raft_rx) = mpsc::unbounded_channel();
    
    // Test 1: Batching enabled
    info!("\n=== Test 1: Batching Enabled ===");
    
    let config = BatchConfig {
        enabled: true,
        max_batch_size: 5,
        batch_timeout_ms: 50,
        max_batch_bytes: 1024 * 1024,
    };
    
    let (proposal_tx, processor) = create_batch_processor(config, raft_tx.clone(), 1);
    
    // Spawn the processor
    let processor_handle = tokio::spawn(processor.run());
    
    // Send 10 proposals rapidly
    info!("Sending 10 proposals...");
    for i in 0..10 {
        let proposal = RaftProposal {
            id: vec![i],
            data: ProposalData::CreateVm(VmCommand::Start {
                name: format!("vm-{}", i),
            }),
            response_tx: None,
        };
        proposal_tx.send(proposal).unwrap();
    }
    
    // Drop sender to signal processor to flush remaining
    drop(proposal_tx);
    
    // Give processor time to flush and shutdown
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Count received batches
    let mut batch_count = 0;
    let mut total_proposals = 0;
    
    while let Ok(proposal) = raft_rx.try_recv() {
        match &proposal.data {
            ProposalData::Batch(proposals) => {
                info!("Received batch {} with {} proposals", batch_count + 1, proposals.len());
                batch_count += 1;
                total_proposals += proposals.len();
            }
            _ => {
                info!("Received single proposal");
                total_proposals += 1;
            }
        }
    }
    
    info!("Total batches: {}, Total proposals: {}", batch_count, total_proposals);
    info!("Average proposals per batch: {:.1}", total_proposals as f64 / batch_count as f64);
    
    // Shutdown processor
    let _ = tokio::time::timeout(Duration::from_secs(1), processor_handle).await;
    
    // Test 2: Timeout-based batching
    info!("\n=== Test 2: Timeout-based Batching ===");
    
    let config = BatchConfig {
        enabled: true,
        max_batch_size: 100, // Large batch size
        batch_timeout_ms: 20, // Short timeout
        max_batch_bytes: 1024 * 1024,
    };
    
    let (proposal_tx, processor) = create_batch_processor(config, raft_tx.clone(), 1);
    let processor_handle = tokio::spawn(processor.run());
    
    // Send only 3 proposals with delays
    info!("Sending 3 proposals with delays...");
    for i in 0..3 {
        let proposal = RaftProposal {
            id: vec![i + 100],
            data: ProposalData::CreateVm(VmCommand::Start {
                name: format!("delayed-vm-{}", i),
            }),
            response_tx: None,
        };
        proposal_tx.send(proposal).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    
    // Wait for timeout to trigger
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Check results
    let mut timeout_batches = 0;
    while let Ok(proposal) = raft_rx.try_recv() {
        if let ProposalData::Batch(proposals) = &proposal.data {
            info!("Timeout-triggered batch with {} proposals", proposals.len());
            timeout_batches += 1;
        }
    }
    
    info!("Batches created by timeout: {}", timeout_batches);
    
    // Shutdown
    drop(proposal_tx);
    let _ = tokio::time::timeout(Duration::from_secs(1), processor_handle).await;
    
    info!("\nBatch processing demo completed!");
    Ok(())
}