//! Integration tests for Raft batch processor

use blixard_core::{
    raft_batch_processor::{BatchConfig, create_batch_processor},
    raft_manager::{RaftProposal, ProposalData},
    types::VmCommand,
};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_batch_processor_basic() {
    // Create channels
    let (raft_tx, mut raft_rx) = mpsc::unbounded_channel();
    
    // Create batch processor with small batch size for testing
    let config = BatchConfig {
        enabled: true,
        max_batch_size: 3,
        batch_timeout_ms: 50,
        max_batch_bytes: 1024 * 1024,
    };
    
    let (proposal_tx, processor) = create_batch_processor(config, raft_tx, 1);
    
    // Spawn the processor
    tokio::spawn(processor.run());
    
    // Send multiple proposals rapidly
    for i in 0..5 {
        let proposal = RaftProposal {
            id: vec![i],
            data: ProposalData::CreateVm(VmCommand::Start {
                name: format!("vm-{}", i),
            }),
            response_tx: None,
        };
        proposal_tx.send(proposal).unwrap();
    }
    
    // Wait for batch processing
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // We should receive batches
    let mut received_count = 0;
    while let Ok(proposal) = raft_rx.try_recv() {
        match &proposal.data {
            ProposalData::Batch(proposals) => {
                println!("Received batch with {} proposals", proposals.len());
                received_count += proposals.len();
            }
            _ => {
                println!("Received single proposal");
                received_count += 1;
            }
        }
    }
    
    assert_eq!(received_count, 5, "Should have received all 5 proposals");
}

#[tokio::test]
async fn test_batch_processor_timeout_flush() {
    // Create channels
    let (raft_tx, mut raft_rx) = mpsc::unbounded_channel();
    
    // Create batch processor with large batch size but short timeout
    let config = BatchConfig {
        enabled: true,
        max_batch_size: 100, // Large batch size
        batch_timeout_ms: 20, // Short timeout
        max_batch_bytes: 1024 * 1024,
    };
    
    let (proposal_tx, processor) = create_batch_processor(config, raft_tx, 1);
    
    // Spawn the processor
    tokio::spawn(processor.run());
    
    // Send only 2 proposals (less than batch size)
    for i in 0..2 {
        let proposal = RaftProposal {
            id: vec![i],
            data: ProposalData::CreateVm(VmCommand::Start {
                name: format!("vm-{}", i),
            }),
            response_tx: None,
        };
        proposal_tx.send(proposal).unwrap();
    }
    
    // Wait for timeout to trigger flush
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // Should receive one batch due to timeout
    let proposal = raft_rx.try_recv().expect("Should receive a proposal");
    match &proposal.data {
        ProposalData::Batch(proposals) => {
            assert_eq!(proposals.len(), 2, "Batch should contain 2 proposals");
        }
        _ => panic!("Expected a batch proposal"),
    }
}

#[tokio::test]
async fn test_batch_processor_response_distribution() {
    // Create channels
    let (raft_tx, mut raft_rx) = mpsc::unbounded_channel();
    
    // Create batch processor
    let config = BatchConfig {
        enabled: true,
        max_batch_size: 3,
        batch_timeout_ms: 50,
        max_batch_bytes: 1024 * 1024,
    };
    
    let (proposal_tx, processor) = create_batch_processor(config, raft_tx, 1);
    
    // Spawn the processor
    tokio::spawn(processor.run());
    
    // Send proposals with response channels
    let mut response_receivers = vec![];
    
    for i in 0..3 {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let proposal = RaftProposal {
            id: vec![i],
            data: ProposalData::CreateVm(VmCommand::Start {
                name: format!("vm-{}", i),
            }),
            response_tx: Some(tx),
        };
        proposal_tx.send(proposal).unwrap();
        response_receivers.push(rx);
    }
    
    // Wait for batch to be created
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Get the batched proposal
    let batch_proposal = raft_rx.try_recv().expect("Should receive batch proposal");
    
    // Simulate Raft committing the batch
    if let Some(batch_tx) = batch_proposal.response_tx {
        batch_tx.send(Ok(())).unwrap();
    }
    
    // All individual proposals should receive responses
    for (i, rx) in response_receivers.into_iter().enumerate() {
        match rx.await {
            Ok(Ok(())) => println!("Proposal {} received success", i),
            Ok(Err(e)) => panic!("Proposal {} received error: {}", i, e),
            Err(_) => panic!("Proposal {} response channel dropped", i),
        }
    }
}