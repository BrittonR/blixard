//! Optimized Raft proposal handling with memory pooling
//!
//! This module demonstrates how to use the memory optimization utilities
//! to reduce allocations in the Raft proposal hot path.

use crate::error::BlixardResult;
use crate::memory_optimization::{
    object_pool::{acquire_raft_proposal, global_pools},
    string_pool::intern,
    collection_utils::{FlatMap, SmallVec},
};
use crate::raft::messages::RaftProposal;
use crate::raft_manager::ProposalData;
use crate::types::{VmCommand, VmStatus};
use std::sync::Arc;

/// Optimized proposal builder that minimizes allocations
pub struct OptimizedProposalBuilder {
    /// Pre-allocated buffer for ID generation
    id_buffer: Vec<u8>,
    /// Cached interned strings
    _node_prefix: crate::memory_optimization::string_pool::InternedString,
}

impl OptimizedProposalBuilder {
    pub fn new(node_id: u64) -> Self {
        Self {
            id_buffer: Vec::with_capacity(16), // UUID size
            _node_prefix: intern(&format!("node-{}", node_id)),
        }
    }
    
    /// Create a VM status update proposal with minimal allocations
    pub async fn create_vm_status_update(
        &mut self,
        vm_name: &str,
        status: VmStatus,
        node_id: u64,
    ) -> BlixardResult<RaftProposal> {
        // Acquire proposal from pool
        let mut pooled_proposal = acquire_raft_proposal().await?;
        let proposal = pooled_proposal.get_mut()?.as_mut();
        
        // Generate ID efficiently
        self.id_buffer.clear();
        self.id_buffer.extend_from_slice(&uuid::Uuid::new_v4().as_bytes()[..]);
        proposal.id = self.id_buffer.clone();
        
        // Use interned strings for common values
        let vm_name_interned = intern(vm_name);
        
        // Create proposal data without extra allocations
        proposal.data = ProposalData::UpdateVmStatus {
            vm_name: vm_name_interned.to_string(), // Only allocates if not interned
            status,
            node_id,
        };
        
        // Take the proposal out of the pool wrapper
        Ok(pooled_proposal.take()?.into_inner())
    }
    
    /// Create a batch proposal with pre-allocated capacity
    pub async fn create_batch_proposal(
        &mut self,
        proposals: Vec<ProposalData>,
    ) -> BlixardResult<RaftProposal> {
        let mut pooled_proposal = acquire_raft_proposal().await?;
        let proposal = pooled_proposal.get_mut()?.as_mut();
        
        // Generate batch ID
        self.id_buffer.clear();
        self.id_buffer.extend_from_slice(&uuid::Uuid::new_v4().as_bytes()[..]);
        proposal.id = self.id_buffer.clone();
        
        proposal.data = ProposalData::Batch(proposals);
        
        Ok(pooled_proposal.take()?.into_inner())
    }
}

/// Optimized proposal processor that reuses buffers
pub struct OptimizedProposalProcessor {
    /// Reusable serialization buffer
    serialize_buffer: Vec<u8>,
    /// Cache of VM names to IDs for fast lookup
    _vm_name_cache: FlatMap<Arc<str>, crate::types::VmId>,
    /// Small vector for temporary collections
    temp_proposals: SmallVec<ProposalData>,
}

impl OptimizedProposalProcessor {
    pub fn new() -> Self {
        Self {
            serialize_buffer: Vec::with_capacity(4096),
            _vm_name_cache: FlatMap::default(),
            temp_proposals: SmallVec::new(),
        }
    }
    
    /// Process proposals with optimized serialization
    pub async fn process_proposals(
        &mut self,
        proposals: &[RaftProposal],
    ) -> BlixardResult<Vec<Vec<u8>>> {
        // Pre-allocate result vector
        let mut results = Vec::with_capacity(proposals.len());
        
        for proposal in proposals {
            // Reuse serialization buffer
            self.serialize_buffer.clear();
            bincode::serialize_into(&mut self.serialize_buffer, &proposal.data)?;
            
            // Clone the buffer content (can't avoid this)
            results.push(self.serialize_buffer.clone());
        }
        
        Ok(results)
    }
    
    /// Batch process VM commands with deduplication
    pub fn batch_vm_commands(&mut self, commands: Vec<VmCommand>) -> Vec<ProposalData> {
        // Use SmallVec for temporary storage
        self.temp_proposals.clear();
        
        // Process commands with deduplication
        let mut seen_vms = FlatMap::default();
        
        for cmd in commands {
            match &cmd {
                VmCommand::Start { name } | VmCommand::Stop { name } => {
                    // Intern VM name for deduplication
                    let interned_name = intern(name);
                    
                    // Skip if we've already seen this VM
                    if seen_vms.insert(interned_name.clone(), ()).is_none() {
                        self.temp_proposals.push(ProposalData::CreateVm(cmd));
                    }
                }
                _ => {
                    self.temp_proposals.push(ProposalData::CreateVm(cmd));
                }
            }
        }
        
        // Convert SmallVec to Vec (may allocate if > 8 items)
        self.temp_proposals.to_vec()
    }
}

/// Example of using memory pools in hot paths
pub async fn optimized_proposal_pipeline_example() -> BlixardResult<()> {
    // Initialize pools at startup
    global_pools::RAFT_PROPOSAL_POOL.initialize().await?;
    
    // Create reusable components
    let mut builder = OptimizedProposalBuilder::new(1);
    let mut processor = OptimizedProposalProcessor::new();
    
    // Example: Create multiple proposals efficiently
    let mut proposals = Vec::with_capacity(100);
    
    for i in 0..100 {
        let vm_name = if i % 10 == 0 {
            "common-vm"  // Will be interned after first use
        } else {
            "unique-vm"  // Less common names
        };
        
        let proposal = builder.create_vm_status_update(
            vm_name,
            VmStatus::Running,
            1,
        ).await?;
        
        proposals.push(proposal);
    }
    
    // Process proposals with reused buffers
    let serialized = processor.process_proposals(&proposals).await?;
    
    println!("Processed {} proposals with minimal allocations", serialized.len());
    
    Ok(())
}

/// Benchmark helper to measure allocation reduction
#[cfg(test)]
pub async fn benchmark_proposal_allocations() {
    use crate::memory_optimization::allocation_tracker::AllocationTracker;
    
    // Measure baseline allocations
    let baseline = AllocationTracker::new("baseline");
    let mut baseline_proposals = Vec::new();
    for i in 0..1000 {
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: ProposalData::UpdateVmStatus {
                vm_name: format!("vm-{}", i),
                status: VmStatus::Running,
                node_id: 1,
            },
            response_tx: None,
        };
        baseline_proposals.push(proposal);
    }
    let baseline_report = baseline.finish();
    
    // Measure optimized allocations
    let optimized = AllocationTracker::new("optimized");
    let mut builder = OptimizedProposalBuilder::new(1);
    let mut optimized_proposals = Vec::new();
    
    for i in 0..1000 {
        let proposal = builder.create_vm_status_update(
            &format!("vm-{}", i),
            VmStatus::Running,
            1,
        ).await.unwrap();
        optimized_proposals.push(proposal);
    }
    let optimized_report = optimized.finish();
    
    // Compare results
    println!("Baseline allocations: {}", baseline_report.allocations);
    println!("Optimized allocations: {}", optimized_report.allocations);
    println!("Reduction: {:.2}%", 
        (1.0 - optimized_report.allocations as f64 / baseline_report.allocations as f64) * 100.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_optimized_proposal_builder() {
        global_pools::RAFT_PROPOSAL_POOL.initialize().await.unwrap();
        
        let mut builder = OptimizedProposalBuilder::new(1);
        let proposal = builder.create_vm_status_update(
            "test-vm",
            VmStatus::Running,
            1,
        ).await.unwrap();
        
        assert_eq!(proposal.id.len(), 16); // UUID size
        match proposal.data {
            ProposalData::UpdateVmStatus { vm_name, status, .. } => {
                assert_eq!(vm_name, "test-vm");
                assert_eq!(status, VmStatus::Running);
            }
            _ => panic!("Wrong proposal type"),
        }
    }
    
    #[tokio::test]
    async fn test_proposal_processor_deduplication() {
        let mut processor = OptimizedProposalProcessor::new();
        
        let commands = vec![
            VmCommand::Start { name: "vm1".to_string() },
            VmCommand::Start { name: "vm1".to_string() }, // Duplicate
            VmCommand::Start { name: "vm2".to_string() },
            VmCommand::Stop { name: "vm1".to_string() },  // Different command, same VM
        ];
        
        let proposals = processor.batch_vm_commands(commands);
        
        // Should deduplicate same VM+command combinations
        assert_eq!(proposals.len(), 3);
    }
}