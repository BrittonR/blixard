//! Batch processing for Raft proposals
//!
//! This module provides a batch processor that accumulates multiple Raft proposals
//! and submits them together, improving throughput and reducing overhead.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::interval;
use tracing::{info, warn};

use crate::error::{BlixardError, BlixardResult};
use crate::metrics_otel::{attributes, metrics};
use crate::raft_manager::{ProposalData, RaftProposal};

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of proposals in a single batch
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing a batch (ms)
    pub batch_timeout_ms: u64,
    /// Maximum bytes in a single batch
    pub max_batch_bytes: usize,
    /// Whether batching is enabled
    pub enabled: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            batch_timeout_ms: 10,
            max_batch_bytes: 1024 * 1024, // 1MB
            enabled: true,
        }
    }
}

/// A batch of proposals to be submitted together
#[derive(Debug)]
struct ProposalBatch {
    proposals: Vec<RaftProposal>,
    total_size: usize,
    created_at: Instant,
}

impl ProposalBatch {
    fn new() -> Self {
        Self {
            proposals: Vec::new(),
            total_size: 0,
            created_at: Instant::now(),
        }
    }

    fn add(&mut self, proposal: RaftProposal, size: usize) {
        self.proposals.push(proposal);
        self.total_size += size;
    }

    fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    fn len(&self) -> usize {
        self.proposals.len()
    }

    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Batch processor for Raft proposals
pub struct RaftBatchProcessor {
    config: BatchConfig,
    /// Channel to receive proposals for batching
    proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
    /// Channel to send batched proposals to Raft
    raft_tx: mpsc::UnboundedSender<RaftProposal>,
    /// Current batch being accumulated
    current_batch: Arc<RwLock<ProposalBatch>>,
    /// Metrics
    node_id: u64,
}

impl RaftBatchProcessor {
    /// Create a new batch processor
    pub fn new(
        config: BatchConfig,
        proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
        raft_tx: mpsc::UnboundedSender<RaftProposal>,
        node_id: u64,
    ) -> Self {
        Self {
            config,
            proposal_rx,
            raft_tx,
            current_batch: Arc::new(RwLock::new(ProposalBatch::new())),
            node_id,
        }
    }

    /// Run the batch processor
    pub async fn run(mut self) {
        if !self.config.enabled {
            info!("Batch processing disabled, forwarding proposals directly");
            // If batching is disabled, just forward proposals directly
            while let Some(proposal) = self.proposal_rx.recv().await {
                if let Err(e) = self.raft_tx.send(proposal) {
                    warn!("Failed to forward proposal: {}", e);
                }
            }
            return;
        }

        info!(
            "Starting Raft batch processor with config: {:?}",
            self.config
        );

        let mut flush_interval = interval(Duration::from_millis(self.config.batch_timeout_ms));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Receive new proposals
                Some(proposal) = self.proposal_rx.recv() => {
                    if let Err(e) = self.add_to_batch(proposal).await {
                        warn!("Failed to add proposal to batch: {}", e);
                    }
                }

                // Periodic flush based on timeout
                _ = flush_interval.tick() => {
                    if let Err(e) = self.check_and_flush_batch().await {
                        warn!("Failed to flush batch on timeout: {}", e);
                    }
                }

                // Channel closed, exit
                else => {
                    info!("Batch processor shutting down, flushing remaining proposals");
                    // Flush any remaining proposals
                    if let Err(e) = self.flush_batch().await {
                        warn!("Error flushing final batch: {}", e);
                    }
                    break;
                }
            }
        }
    }

    /// Add a proposal to the current batch
    async fn add_to_batch(&self, proposal: RaftProposal) -> BlixardResult<()> {
        // Estimate the size of this proposal
        let proposal_size = self.estimate_proposal_size(&proposal)?;

        let mut batch = self.current_batch.write().await;

        // Check if adding this proposal would exceed limits
        if !batch.is_empty()
            && (batch.len() >= self.config.max_batch_size
                || batch.total_size + proposal_size > self.config.max_batch_bytes)
        {
            // Flush current batch first
            let current = std::mem::replace(&mut *batch, ProposalBatch::new());
            drop(batch); // Release lock before flushing
            self.flush_specific_batch(current).await?;

            // Add to new batch
            let mut batch = self.current_batch.write().await;
            batch.add(proposal, proposal_size);
        } else {
            batch.add(proposal, proposal_size);

            // Check if we should flush immediately based on size
            if batch.len() >= self.config.max_batch_size
                || batch.total_size >= self.config.max_batch_bytes
            {
                let current = std::mem::replace(&mut *batch, ProposalBatch::new());
                drop(batch); // Release lock before flushing
                self.flush_specific_batch(current).await?;
            }
        }

        Ok(())
    }

    /// Check if the current batch should be flushed based on age
    async fn check_and_flush_batch(&self) -> BlixardResult<()> {
        let batch = self.current_batch.read().await;
        if !batch.is_empty() && batch.age() >= Duration::from_millis(self.config.batch_timeout_ms) {
            drop(batch); // Release read lock
            self.flush_batch().await?;
        }
        Ok(())
    }

    /// Flush the current batch
    async fn flush_batch(&self) -> BlixardResult<()> {
        let mut batch = self.current_batch.write().await;
        if !batch.is_empty() {
            let current = std::mem::replace(&mut *batch, ProposalBatch::new());
            drop(batch); // Release lock before flushing
            self.flush_specific_batch(current).await?;
        }
        Ok(())
    }

    /// Flush a specific batch
    async fn flush_specific_batch(&self, batch: ProposalBatch) -> BlixardResult<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let batch_size = batch.len();
        let batch_bytes = batch.total_size;
        let batch_age_ms = batch.age().as_millis() as u64;

        info!(
            "Flushing batch: {} proposals, {} bytes, age {} ms",
            batch_size, batch_bytes, batch_age_ms
        );

        // If we have only one proposal, send it directly
        if batch_size == 1 {
            let proposal =
                batch
                    .proposals
                    .into_iter()
                    .next()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Batch size is 1 but no proposals found".to_string(),
                    })?;
            self.raft_tx
                .send(proposal)
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to send proposal: {}", e),
                })?;
        } else {
            // Create a batched proposal
            let batch_proposal = self.create_batched_proposal(batch)?;

            // Send the batched proposal
            self.raft_tx
                .send(batch_proposal)
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to send batched proposal: {}", e),
                })?;

            // Record batch metrics
            let node_attr = attributes::node_id(self.node_id);
            metrics().raft_batches_total.add(1, &[node_attr.clone()]);
            metrics()
                .raft_batch_size
                .record(batch_size as f64, &[node_attr.clone()]);
            metrics()
                .raft_batch_bytes
                .record(batch_bytes as f64, &[node_attr.clone()]);
            metrics()
                .raft_batch_age_ms
                .record(batch_age_ms as f64, &[node_attr]);
        }

        Ok(())
    }

    /// Create a single batched proposal from multiple proposals
    fn create_batched_proposal(&self, batch: ProposalBatch) -> BlixardResult<RaftProposal> {
        // Collect all proposal data
        let mut batch_data = Vec::new();
        let mut response_channels = Vec::new();

        for proposal in batch.proposals {
            batch_data.push(proposal.data);
            if let Some(tx) = proposal.response_tx {
                response_channels.push((proposal.id.clone(), tx));
            }
        }

        // Create a batched proposal with a unique ID
        let batch_id = uuid::Uuid::new_v4().as_bytes().to_vec();

        // Create a special response channel that will distribute responses
        let (batch_tx, batch_rx) = oneshot::channel::<BlixardResult<()>>();

        // Spawn a task to distribute responses when the batch is committed
        tokio::spawn(async move {
            match batch_rx.await {
                Ok(Ok(())) => {
                    // Batch succeeded, send success to all waiting channels
                    for (_id, tx) in response_channels {
                        let _ = tx.send(Ok(()));
                    }
                }
                Ok(Err(e)) => {
                    // Batch failed, send the same error message to all
                    let error_msg = format!("Batch proposal failed: {}", e);
                    for (_id, tx) in response_channels {
                        let _ = tx.send(Err(BlixardError::Internal {
                            message: error_msg.clone(),
                        }));
                    }
                }
                Err(_) => {
                    // Batch response channel was dropped, send error to all
                    for (_id, tx) in response_channels {
                        let _ = tx.send(Err(BlixardError::Internal {
                            message: "Batch response channel dropped".to_string(),
                        }));
                    }
                }
            }
        });

        Ok(RaftProposal {
            id: batch_id,
            data: ProposalData::Batch(batch_data),
            response_tx: Some(batch_tx),
        })
    }

    /// Estimate the size of a proposal in bytes
    fn estimate_proposal_size(&self, proposal: &RaftProposal) -> BlixardResult<usize> {
        // Use bincode to get accurate size
        let size =
            bincode::serialized_size(&proposal.data).map_err(|e| BlixardError::Serialization {
                operation: "estimate proposal size".to_string(),
                source: Box::new(e),
            })? as usize;

        // Add some overhead for the proposal metadata
        Ok(size + proposal.id.len() + 64)
    }
}

/// Create a batch processor with a channel for submitting proposals
pub fn create_batch_processor(
    config: BatchConfig,
    raft_tx: mpsc::UnboundedSender<RaftProposal>,
    node_id: u64,
) -> (mpsc::UnboundedSender<RaftProposal>, RaftBatchProcessor) {
    let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();
    let processor = RaftBatchProcessor::new(config, proposal_rx, raft_tx, node_id);
    (proposal_tx, processor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_processor_disabled() {
        let (raft_tx, mut raft_rx) = mpsc::unbounded_channel();
        let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();

        let config = BatchConfig {
            enabled: false,
            ..Default::default()
        };

        let processor = RaftBatchProcessor::new(config, proposal_rx, raft_tx, 1);

        // Run processor in background
        tokio::spawn(processor.run());

        // Send a proposal
        let proposal = RaftProposal {
            id: vec![1, 2, 3],
            data: ProposalData::CreateVm(crate::types::VmCommand::Start {
                name: "test-vm".to_string(),
            }),
            response_tx: None,
        };

        let expected_id = proposal.id.clone();
        proposal_tx.send(proposal).unwrap();

        // Should receive it immediately when batching is disabled
        tokio::time::sleep(Duration::from_millis(5)).await;
        let received = raft_rx.try_recv().unwrap();
        assert_eq!(received.id, expected_id);
    }
}
