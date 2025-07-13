//! Optimized Raft batch processor with streaming and advanced concurrency
//!
//! This module provides an enhanced version of the RaftBatchProcessor with
//! streaming optimizations, adaptive batching, and improved resource management.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock, Semaphore};
use tokio::time::interval;
use tracing::{debug, info, warn};
// Optimized batch processing for Raft proposals

use crate::{
    common::async_utils::{
        concurrent_map, AdaptiveConcurrencyController, AtomicCounter, CircuitBreaker,
        CircuitBreakerError, WeightedLoadBalancer, LoadBalancingStrategy,
    },
    error::{BlixardError, BlixardResult},
    raft::messages::RaftProposal,
    raft_manager::ProposalData,
};

#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, safe_metrics};

/// Advanced configuration for optimized batch processing
#[derive(Debug, Clone)]
pub struct OptimizedBatchConfig {
    /// Basic batching configuration
    pub max_batch_size: usize,
    pub batch_timeout_ms: u64,
    pub max_batch_bytes: usize,
    pub enabled: bool,
    
    /// Streaming and concurrency
    pub enable_streaming: bool,
    pub stream_buffer_size: usize,
    pub max_concurrent_batches: usize,
    
    /// Adaptive batching
    pub enable_adaptive_batching: bool,
    pub min_batch_size: usize,
    pub adaptive_window_size: usize,
    pub performance_threshold_ms: u64,
    
    /// Quality of Service
    pub enable_priority_batching: bool,
    pub high_priority_threshold: Duration,
    pub max_priority_batch_size: usize,
    
    /// Resource management
    pub enable_backpressure: bool,
    pub max_pending_proposals: usize,
    pub memory_pressure_threshold: f64,
    
    /// Fault tolerance
    pub enable_circuit_breaker: bool,
    pub circuit_breaker_failure_threshold: usize,
    pub circuit_breaker_timeout: Duration,
}

impl Default for OptimizedBatchConfig {
    fn default() -> Self {
        Self {
            // Basic config
            max_batch_size: 100,
            batch_timeout_ms: 10,
            max_batch_bytes: 1024 * 1024, // 1MB
            enabled: true,
            
            // Streaming
            enable_streaming: true,
            stream_buffer_size: 1000,
            max_concurrent_batches: 4,
            
            // Adaptive batching
            enable_adaptive_batching: true,
            min_batch_size: 5,
            adaptive_window_size: 20,
            performance_threshold_ms: 50,
            
            // Priority batching
            enable_priority_batching: true,
            high_priority_threshold: Duration::from_millis(5),
            max_priority_batch_size: 20,
            
            // Resource management
            enable_backpressure: true,
            max_pending_proposals: 10000,
            memory_pressure_threshold: 0.8,
            
            // Fault tolerance
            enable_circuit_breaker: true,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(30),
        }
    }
}

/// Priority level for proposal batching
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProposalPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Enhanced proposal with priority and metadata
#[derive(Debug)]
pub struct EnhancedRaftProposal {
    pub inner: RaftProposal,
    pub priority: ProposalPriority,
    pub submitted_at: Instant,
    pub estimated_size: usize,
    pub timeout: Option<Duration>,
}

impl EnhancedRaftProposal {
    pub fn new(proposal: RaftProposal, priority: ProposalPriority) -> BlixardResult<Self> {
        let estimated_size = Self::estimate_proposal_size(&proposal.data)?;
        Ok(Self {
            inner: proposal,
            priority,
            submitted_at: Instant::now(),
            estimated_size,
            timeout: None,
        })
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    pub fn is_expired(&self) -> bool {
        if let Some(timeout) = self.timeout {
            self.submitted_at.elapsed() > timeout
        } else {
            false
        }
    }
    
    pub fn age(&self) -> Duration {
        self.submitted_at.elapsed()
    }
    
    fn estimate_proposal_size(data: &ProposalData) -> BlixardResult<usize> {
        bincode::serialized_size(data)
            .map_err(|e| BlixardError::Serialization {
                operation: "estimate proposal size".to_string(),
                source: Box::new(e),
            })
            .map(|size| size as usize + 64) // Add overhead
    }
}

/// Optimized batch with advanced features
#[derive(Debug)]
pub struct OptimizedProposalBatch {
    pub proposals: Vec<EnhancedRaftProposal>,
    pub total_size: usize,
    pub created_at: Instant,
    pub priority: ProposalPriority,
    pub batch_id: uuid::Uuid,
}

impl OptimizedProposalBatch {
    pub fn new() -> Self {
        Self {
            proposals: Vec::new(),
            total_size: 0,
            created_at: Instant::now(),
            priority: ProposalPriority::Normal,
            batch_id: uuid::Uuid::new_v4(),
        }
    }
    
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            proposals: Vec::with_capacity(capacity),
            total_size: 0,
            created_at: Instant::now(),
            priority: ProposalPriority::Normal,
            batch_id: uuid::Uuid::new_v4(),
        }
    }
    
    pub fn add(&mut self, proposal: EnhancedRaftProposal) {
        self.total_size += proposal.estimated_size;
        
        // Update batch priority to highest proposal priority
        if proposal.priority > self.priority {
            self.priority = proposal.priority;
        }
        
        self.proposals.push(proposal);
    }
    
    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.proposals.len()
    }
    
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
    
    pub fn should_flush(&self, config: &OptimizedBatchConfig) -> bool {
        if self.is_empty() {
            return false;
        }
        
        // Check size limits
        if self.len() >= config.max_batch_size || self.total_size >= config.max_batch_bytes {
            return true;
        }
        
        // Check priority-based timeouts
        let timeout = match self.priority {
            ProposalPriority::Critical => Duration::from_millis(1),
            ProposalPriority::High => config.high_priority_threshold,
            ProposalPriority::Normal => Duration::from_millis(config.batch_timeout_ms),
            ProposalPriority::Low => Duration::from_millis(config.batch_timeout_ms * 2),
        };
        
        self.age() >= timeout
    }
    
    /// Remove expired proposals
    pub fn remove_expired(&mut self) -> Vec<EnhancedRaftProposal> {
        let mut expired = Vec::new();
        let mut i = 0;
        
        while i < self.proposals.len() {
            if self.proposals[i].is_expired() {
                let proposal = self.proposals.remove(i);
                self.total_size -= proposal.estimated_size;
                expired.push(proposal);
            } else {
                i += 1;
            }
        }
        
        // Recalculate priority
        self.priority = self.proposals.iter()
            .map(|p| p.priority)
            .max()
            .unwrap_or(ProposalPriority::Normal);
        
        expired
    }
}

/// Performance metrics for adaptive batching
#[derive(Debug)]
struct BatchPerformanceMetrics {
    batch_sizes: VecDeque<usize>,
    processing_times: VecDeque<Duration>,
    throughput_samples: VecDeque<f64>,
    last_update: Instant,
}

impl Default for BatchPerformanceMetrics {
    fn default() -> Self {
        Self {
            batch_sizes: VecDeque::with_capacity(50),
            processing_times: VecDeque::with_capacity(50),
            throughput_samples: VecDeque::with_capacity(50),
            last_update: Instant::now(),
        }
    }
}

impl BatchPerformanceMetrics {
    fn new() -> Self {
        Self::default()
    }
    
    fn record_batch(&mut self, batch_size: usize, processing_time: Duration) {
        // Record metrics
        self.batch_sizes.push_back(batch_size);
        self.processing_times.push_back(processing_time);
        
        // Calculate throughput (proposals per second)
        let throughput = batch_size as f64 / processing_time.as_secs_f64();
        self.throughput_samples.push_back(throughput);
        
        // Keep only recent samples
        const MAX_SAMPLES: usize = 50;
        if self.batch_sizes.len() > MAX_SAMPLES {
            self.batch_sizes.pop_front();
            self.processing_times.pop_front();
            self.throughput_samples.pop_front();
        }
        
        self.last_update = Instant::now();
    }
    
    fn get_optimal_batch_size(&self, config: &OptimizedBatchConfig) -> usize {
        if self.throughput_samples.len() < 5 {
            return config.max_batch_size / 2; // Conservative default
        }
        
        // Find batch size with best throughput
        let mut best_size = config.min_batch_size;
        let mut best_throughput = 0.0;
        
        for (&size, &throughput) in self.batch_sizes.iter().zip(self.throughput_samples.iter()) {
            if throughput > best_throughput {
                best_throughput = throughput;
                best_size = size;
            }
        }
        
        // Clamp to configured limits
        best_size.clamp(config.min_batch_size, config.max_batch_size)
    }
    
    fn get_avg_processing_time(&self) -> Duration {
        if self.processing_times.is_empty() {
            Duration::from_millis(10)
        } else {
            let total: Duration = self.processing_times.iter().sum();
            total / self.processing_times.len() as u32
        }
    }
}

/// Optimized Raft batch processor with streaming and advanced features
pub struct OptimizedRaftBatchProcessor {
    config: OptimizedBatchConfig,
    node_id: u64,
    
    /// Streaming channels
    proposal_rx: mpsc::Receiver<EnhancedRaftProposal>,
    raft_tx: mpsc::UnboundedSender<RaftProposal>,
    
    /// Priority queues for different proposal types
    priority_batches: Arc<RwLock<Vec<OptimizedProposalBatch>>>,
    
    /// Adaptive batching
    performance_metrics: Arc<RwLock<BatchPerformanceMetrics>>,
    adaptive_controller: AdaptiveConcurrencyController,
    
    /// Resource management
    pending_proposals: AtomicCounter,
    processing_semaphore: Arc<Semaphore>,
    
    /// Fault tolerance
    circuit_breaker: CircuitBreaker,
    load_balancer: WeightedLoadBalancer<String>,
    
    /// Background task handles
    background_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl OptimizedRaftBatchProcessor {
    /// Create a new optimized batch processor
    pub fn new(
        config: OptimizedBatchConfig,
        proposal_rx: mpsc::Receiver<EnhancedRaftProposal>,
        raft_tx: mpsc::UnboundedSender<RaftProposal>,
        node_id: u64,
    ) -> Self {
        let processing_semaphore = Arc::new(Semaphore::new(config.max_concurrent_batches));
        let circuit_breaker = CircuitBreaker::new(
            config.circuit_breaker_failure_threshold,
            config.circuit_breaker_timeout,
        );
        let adaptive_controller = AdaptiveConcurrencyController::new(1, config.max_concurrent_batches);
        let load_balancer = WeightedLoadBalancer::new(LoadBalancingStrategy::ResponseTimeBased);
        
        // Initialize load balancer with workers after construction
        
        let processor = Self {
            config,
            node_id,
            proposal_rx,
            raft_tx,
            priority_batches: Arc::new(RwLock::new(Vec::new())),
            performance_metrics: Arc::new(RwLock::new(BatchPerformanceMetrics::new())),
            adaptive_controller,
            pending_proposals: AtomicCounter::new(0),
            processing_semaphore,
            circuit_breaker,
            load_balancer,
            background_tasks: Vec::new(),
        };
        
        // Initialize load balancer with workers
        for i in 0..processor.config.max_concurrent_batches {
            futures::executor::block_on(
                processor.load_balancer.add_resource(format!("worker-{}", i), 1.0)
            );
        }
        
        processor
    }
    
    /// Run the optimized batch processor
    pub async fn run(mut self) {
        if !self.config.enabled {
            info!("Optimized batch processing disabled, forwarding proposals directly");
            self.run_passthrough_mode().await;
            return;
        }
        
        info!("Starting optimized Raft batch processor with config: {:?}", self.config);
        
        // Start background tasks
        self.start_background_tasks().await;
        
        if self.config.enable_streaming {
            self.run_streaming_mode().await;
        } else {
            self.run_traditional_mode().await;
        }
        
        // Clean up background tasks
        for task in self.background_tasks {
            task.abort();
        }
    }
    
    /// Run in streaming mode with continuous processing
    async fn run_streaming_mode(&mut self) {
        
        loop {
            let mut chunk = Vec::with_capacity(self.config.stream_buffer_size);
            
            // Collect up to stream_buffer_size proposals
            for _ in 0..self.config.stream_buffer_size {
                match self.proposal_rx.recv().await {
                    Some(proposal) => chunk.push(proposal),
                    None => {
                        // Channel closed
                        if !chunk.is_empty() {
                            if let Err(e) = self.process_proposal_chunk(chunk).await {
                                warn!("Error processing final proposal chunk: {}", e);
                            }
                        }
                        return;
                    }
                }
                
                // If we have a full batch or the channel would block, process now
                if chunk.len() == self.config.stream_buffer_size || self.proposal_rx.is_empty() {
                    break;
                }
            }
            
            if !chunk.is_empty() {
                if let Err(e) = self.process_proposal_chunk(chunk).await {
                    warn!("Error processing proposal chunk: {}", e);
                }
            }
        }
    }
    
    /// Run in traditional batching mode
    async fn run_traditional_mode(&mut self) {
        let mut flush_interval = interval(Duration::from_millis(self.config.batch_timeout_ms));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        
        loop {
            tokio::select! {
                Some(proposal) = self.proposal_rx.recv() => {
                    if let Err(e) = self.add_to_priority_batch(proposal).await {
                        warn!("Failed to add proposal to batch: {}", e);
                    }
                }
                
                _ = flush_interval.tick() => {
                    if let Err(e) = self.flush_ready_batches().await {
                        warn!("Failed to flush batches on timeout: {}", e);
                    }
                }
                
                else => {
                    info!("Optimized batch processor shutting down");
                    if let Err(e) = self.flush_all_batches().await {
                        warn!("Error flushing final batches: {}", e);
                    }
                    break;
                }
            }
        }
    }
    
    /// Run in passthrough mode (no batching)
    async fn run_passthrough_mode(&mut self) {
        while let Some(enhanced_proposal) = self.proposal_rx.recv().await {
            if let Err(e) = self.raft_tx.send(enhanced_proposal.inner) {
                warn!("Failed to forward proposal: {}", e);
            }
        }
    }
    
    /// Process a chunk of proposals concurrently
    async fn process_proposal_chunk(&self, proposals: Vec<EnhancedRaftProposal>) -> BlixardResult<()> {
        if proposals.is_empty() {
            return Ok(());
        }
        
        // Group proposals by priority
        let mut priority_groups: std::collections::HashMap<ProposalPriority, Vec<EnhancedRaftProposal>> = 
            std::collections::HashMap::new();
        
        for proposal in proposals {
            priority_groups.entry(proposal.priority).or_default().push(proposal);
        }
        
        // Process priority groups concurrently
        let results = concurrent_map(
            priority_groups.into_iter().collect::<Vec<_>>(),
            self.config.max_concurrent_batches,
            |(priority, group)| self.process_priority_group(priority, group),
        ).await;
        
        // Check for errors
        for result in results {
            result?;
        }
        
        Ok(())
    }
    
    /// Process a group of proposals with the same priority
    async fn process_priority_group(
        &self,
        priority: ProposalPriority,
        proposals: Vec<EnhancedRaftProposal>,
    ) -> BlixardResult<()> {
        // Acquire processing permit
        let _permit = self.processing_semaphore.acquire().await.map_err(|_| {
            BlixardError::Internal {
                message: "Processing semaphore closed".to_string(),
            }
        })?;
        
        // Create batch with optimal size
        let optimal_size = if self.config.enable_adaptive_batching {
            self.performance_metrics.read().await.get_optimal_batch_size(&self.config)
        } else {
            self.config.max_batch_size
        };
        
        // Split proposals into optimally-sized batches
        let mut proposals_iter = proposals.into_iter().peekable();
        while proposals_iter.peek().is_some() {
            let mut batch = OptimizedProposalBatch::with_capacity(optimal_size);
            batch.priority = priority;
            
            for _ in 0..optimal_size {
                if let Some(proposal) = proposals_iter.next() {
                    batch.add(proposal);
                } else {
                    break;
                }
            }
            
            // Process batch with circuit breaker
            let batch_result = self.circuit_breaker.call(|| async {
                self.process_single_batch(batch).await
            }).await;
            
            match batch_result {
                Ok(_) => {
                    debug!("Successfully processed batch for priority {:?}", priority);
                }
                Err(CircuitBreakerError::CircuitOpen) => {
                    warn!("Circuit breaker is open, dropping batch");
                    return Err(BlixardError::Internal {
                        message: "Circuit breaker open".to_string(),
                    });
                }
                Err(CircuitBreakerError::OperationFailed(e)) => {
                    warn!("Batch processing failed: {}", e);
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Add proposal to appropriate priority batch
    async fn add_to_priority_batch(&self, proposal: EnhancedRaftProposal) -> BlixardResult<()> {
        // Check backpressure
        if self.config.enable_backpressure {
            let pending = self.pending_proposals.get();
            if pending >= self.config.max_pending_proposals as u64 {
                return Err(BlixardError::ResourceExhausted {
                    resource: format!("Pending proposals queue (limit: {})", self.config.max_pending_proposals),
                });
            }
        }
        
        self.pending_proposals.increment();
        
        let mut batches = self.priority_batches.write().await;
        
        // Find existing batch for this priority or create new one
        let batch_index = batches.iter()
            .position(|b| b.priority == proposal.priority)
            .unwrap_or_else(|| {
                batches.push(OptimizedProposalBatch::new());
                batches.len() - 1
            });
        
        batches[batch_index].add(proposal);
        
        // Check if batch should be flushed
        if batches[batch_index].should_flush(&self.config) {
            let batch = batches.remove(batch_index);
            drop(batches); // Release lock
            self.process_single_batch(batch).await?;
        }
        
        Ok(())
    }
    
    /// Flush all ready batches
    async fn flush_ready_batches(&self) -> BlixardResult<()> {
        let mut batches_to_flush = Vec::new();
        
        {
            let mut batches = self.priority_batches.write().await;
            let mut i = 0;
            
            while i < batches.len() {
                if batches[i].should_flush(&self.config) {
                    batches_to_flush.push(batches.remove(i));
                } else {
                    // Remove expired proposals
                    let expired = batches[i].remove_expired();
                    for mut expired_proposal in expired {
                        // Send timeout error to proposal
                        let duration = expired_proposal.age();
                        if let Some(tx) = expired_proposal.inner.response_tx.take() {
                            let _ = tx.send(Err(BlixardError::Timeout {
                                operation: "Proposal processing".to_string(),
                                duration,
                            }));
                        }
                    }
                    i += 1;
                }
            }
        }
        
        // Process batches concurrently
        let results = concurrent_map(
            batches_to_flush,
            self.config.max_concurrent_batches,
            |batch| self.process_single_batch(batch),
        ).await;
        
        // Check for errors
        for result in results {
            result?;
        }
        
        Ok(())
    }
    
    /// Flush all remaining batches
    async fn flush_all_batches(&self) -> BlixardResult<()> {
        let batches = {
            let mut batches = self.priority_batches.write().await;
            std::mem::take(&mut *batches)
        };
        
        for batch in batches {
            if !batch.is_empty() {
                self.process_single_batch(batch).await?;
            }
        }
        
        Ok(())
    }
    
    /// Process a single batch
    async fn process_single_batch(&self, batch: OptimizedProposalBatch) -> BlixardResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        
        let start = Instant::now();
        let batch_size = batch.len();
        let batch_bytes = batch.total_size;
        let batch_age_ms = batch.age().as_millis() as u64;
        
        debug!(
            "Processing batch {}: {} proposals, {} bytes, age {} ms, priority {:?}",
            batch.batch_id, batch_size, batch_bytes, batch_age_ms, batch.priority
        );
        
        // Convert to regular Raft proposals
        let raft_proposals: Vec<RaftProposal> = batch.proposals
            .into_iter()
            .map(|enhanced| enhanced.inner)
            .collect();
        
        // Send proposals based on batch size
        if raft_proposals.len() == 1 {
            // Send single proposal directly
            let proposal = raft_proposals.into_iter().next().unwrap();
            self.raft_tx.send(proposal).map_err(|e| {
                BlixardError::Internal {
                    message: format!("Failed to send proposal: {}", e),
                }
            })?;
        } else {
            // Create batched proposal
            let batch_proposal = self.create_batched_proposal(raft_proposals)?;
            self.raft_tx.send(batch_proposal).map_err(|e| {
                BlixardError::Internal {
                    message: format!("Failed to send batched proposal: {}", e),
                }
            })?;
        }
        
        // Record performance metrics
        let processing_time = start.elapsed();
        self.performance_metrics.write().await.record_batch(batch_size, processing_time);
        
        // Update pending counter
        for _ in 0..batch_size {
            self.pending_proposals.decrement();
        }
        
        // Record observability metrics
        #[cfg(feature = "observability")]
        {
            if let Ok(metrics) = safe_metrics() {
                let node_attr = attributes::node_id(self.node_id);
                metrics.raft_batches_total.add(1, &[node_attr.clone()]);
                metrics.raft_batch_size.record(batch_size as f64, &[node_attr.clone()]);
                metrics.raft_batch_bytes.record(batch_bytes as f64, &[node_attr.clone()]);
                metrics.raft_batch_age_ms.record(batch_age_ms as f64, &[node_attr.clone()]);
                metrics.raft_proposal_duration.record(
                    processing_time.as_millis() as f64,
                    &[node_attr],
                );
            }
        }
        
        Ok(())
    }
    
    /// Create batched proposal from multiple proposals
    fn create_batched_proposal(&self, proposals: Vec<RaftProposal>) -> BlixardResult<RaftProposal> {
        let mut batch_data = Vec::new();
        let mut response_channels = Vec::new();
        
        for proposal in proposals {
            batch_data.push(proposal.data);
            if let Some(tx) = proposal.response_tx {
                response_channels.push((proposal.id.clone(), tx));
            }
        }
        
        let batch_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        let (batch_tx, batch_rx) = oneshot::channel::<BlixardResult<()>>();
        
        // Spawn response distribution task
        tokio::spawn(async move {
            match batch_rx.await {
                Ok(Ok(())) => {
                    for (_id, tx) in response_channels {
                        let _ = tx.send(Ok(()));
                    }
                }
                Ok(Err(e)) => {
                    let error_msg = format!("Batch proposal failed: {}", e);
                    for (_id, tx) in response_channels {
                        let _ = tx.send(Err(BlixardError::Internal {
                            message: error_msg.clone(),
                        }));
                    }
                }
                Err(_) => {
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
    
    /// Start background tasks
    async fn start_background_tasks(&mut self) {
        // Cleanup task for expired proposals
        let priority_batches = Arc::clone(&self.priority_batches);
        let _config = self.config.clone();
        self.background_tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                let mut batches = priority_batches.write().await;
                for batch in batches.iter_mut() {
                    let _expired = batch.remove_expired();
                    // TODO: Handle expired proposals appropriately
                }
            }
        }));
        
        // Memory pressure monitoring
        if self.config.enable_backpressure {
            let pending_proposals = self.pending_proposals.clone();
            let config = self.config.clone();
            self.background_tasks.push(tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                
                loop {
                    interval.tick().await;
                    
                    let pending = pending_proposals.get() as f64;
                    let pressure = pending / config.max_pending_proposals as f64;
                    
                    if pressure > config.memory_pressure_threshold {
                        warn!("High memory pressure: {:.2}% ({} pending proposals)", 
                             pressure * 100.0, pending as usize);
                    }
                }
            }));
        }
    }
}

/// Create an optimized batch processor with enhanced proposal channel
pub fn create_optimized_batch_processor(
    config: OptimizedBatchConfig,
    raft_tx: mpsc::UnboundedSender<RaftProposal>,
    node_id: u64,
) -> (mpsc::Sender<EnhancedRaftProposal>, OptimizedRaftBatchProcessor) {
    let (proposal_tx, proposal_rx) = mpsc::channel(config.stream_buffer_size);
    let processor = OptimizedRaftBatchProcessor::new(config, proposal_rx, raft_tx, node_id);
    (proposal_tx, processor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_enhanced_proposal_creation() {
        let raft_proposal = RaftProposal {
            id: vec![1, 2, 3],
            data: ProposalData::CreateVm(crate::types::VmCommand::Start {
                name: "test-vm".to_string(),
            }),
            response_tx: None,
        };
        
        let enhanced = EnhancedRaftProposal::new(raft_proposal, ProposalPriority::High).unwrap();
        assert_eq!(enhanced.priority, ProposalPriority::High);
        assert!(!enhanced.is_expired());
    }
    
    #[tokio::test]
    async fn test_optimized_batch_creation() {
        let mut batch = OptimizedProposalBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.priority, ProposalPriority::Normal);
        
        let proposal = EnhancedRaftProposal::new(
            RaftProposal {
                id: vec![1],
                data: ProposalData::CreateVm(crate::types::VmCommand::Start {
                    name: "test".to_string(),
                }),
                response_tx: None,
            },
            ProposalPriority::High,
        ).unwrap();
        
        batch.add(proposal);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.priority, ProposalPriority::High);
    }
    
    #[test]
    fn test_batch_performance_metrics() {
        let mut metrics = BatchPerformanceMetrics::new();
        
        metrics.record_batch(10, Duration::from_millis(100));
        metrics.record_batch(20, Duration::from_millis(150));
        
        let config = OptimizedBatchConfig::default();
        let optimal_size = metrics.get_optimal_batch_size(&config);
        
        assert!(optimal_size >= config.min_batch_size);
        assert!(optimal_size <= config.max_batch_size);
    }
}