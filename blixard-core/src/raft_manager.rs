//! Raft-based distributed consensus manager
//!
//! This module provides a simplified interface to the distributed Raft implementation
//! that has been reorganized into focused modules for better maintainability.

// Re-export main types from the new modular structure
pub use crate::raft::core::RaftManager;
pub use crate::raft::messages::{RaftMessage, RaftConfChange, ConfChangeType, ConfChangeContext};
pub use crate::raft::messages::RaftProposal;
pub use crate::raft::proposals::{
    ProposalData, TaskSpec, TaskResult, ResourceRequirements,
    WorkerCapabilities, WorkerStatus
};

// Legacy compatibility exports - these may be removed in future versions
pub use crate::raft::messages::RaftConfChange as ConfChange;
pub use crate::raft::proposals::ProposalData as RaftProposalData;

/// Create a new RaftManager instance
/// 
/// This is a convenience function that delegates to the modular implementation.
pub async fn create_raft_manager(
    node_id: u64,
    database: std::sync::Arc<redb::Database>,
    peers: Vec<(u64, String)>,
    shared_state: std::sync::Weak<crate::node_shared::SharedNodeState>,
) -> crate::error::BlixardResult<(
    RaftManager,
    tokio::sync::mpsc::UnboundedSender<RaftProposal>,
    tokio::sync::mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    tokio::sync::mpsc::UnboundedSender<RaftConfChange>,
    tokio::sync::mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
)> {
    RaftManager::new(node_id, database, peers, shared_state)
}

// Helper functions that may be used by other modules
pub fn serialize_proposal_data(data: &ProposalData) -> crate::error::BlixardResult<Vec<u8>> {
    bincode::serialize(data).map_err(|e| crate::error::BlixardError::Serialization {
        operation: "serialize proposal data".to_string(),
        source: Box::new(e),
    })
}

pub fn deserialize_proposal_data(data: &[u8]) -> crate::error::BlixardResult<ProposalData> {
    bincode::deserialize(data).map_err(|e| crate::error::BlixardError::Serialization {
        operation: "deserialize proposal data".to_string(),
        source: Box::new(e),
    })
}

/// Validate that a task spec has valid resource requirements
pub fn validate_task_spec(spec: &TaskSpec) -> crate::error::BlixardResult<()> {
    if spec.command.is_empty() {
        return Err(crate::error::BlixardError::Validation {
            field: "command".to_string(),
            message: "Task command cannot be empty".to_string(),
        });
    }

    if spec.resources.cpu_cores == 0 {
        return Err(crate::error::BlixardError::Validation {
            field: "cpu_cores".to_string(),
            message: "Task must require at least 1 CPU core".to_string(),
        });
    }

    if spec.resources.memory_mb == 0 {
        return Err(crate::error::BlixardError::Validation {
            field: "memory_mb".to_string(),
            message: "Task must require at least 1 MB of memory".to_string(),
        });
    }

    if spec.timeout_secs == 0 {
        return Err(crate::error::BlixardError::Validation {
            field: "timeout_secs".to_string(),
            message: "Task timeout must be greater than 0".to_string(),
        });
    }

    Ok(())
}

/// Create a batch proposal from multiple individual proposals
pub fn create_batch_proposal(proposals: Vec<ProposalData>) -> crate::error::BlixardResult<ProposalData> {
    if proposals.is_empty() {
        return Err(crate::error::BlixardError::Validation {
            field: "proposals".to_string(),
            message: "Batch cannot be empty".to_string(),
        });
    }

    // Check for nested batches
    for proposal in &proposals {
        if matches!(proposal, ProposalData::Batch(_)) {
            return Err(crate::error::BlixardError::Validation {
                field: "proposals".to_string(),
                message: "Nested batch proposals are not allowed".to_string(),
            });
        }
    }

    Ok(ProposalData::Batch(proposals))
}

/// Schedule a task using a simple round-robin algorithm
/// This is a basic scheduler that will be enhanced with more sophisticated algorithms
pub fn schedule_task(
    database: std::sync::Arc<redb::Database>,
    task_id: &str,
    task_spec: &TaskSpec,
) -> crate::error::BlixardResult<u64> {
    use crate::common::error_context::StorageContext;
    use crate::raft_storage::{WORKER_TABLE, WORKER_STATUS_TABLE};
    use redb::ReadableTable;

    // Read available workers
    let read_txn = database.begin_read().storage_context("begin read transaction")?;
    let worker_table = read_txn.open_table(WORKER_TABLE).storage_context("open worker table")?;
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE).storage_context("open worker status table")?;

    let mut available_workers = Vec::new();

    // Find workers with sufficient resources
    for worker_entry in worker_table.iter().storage_context("iterate workers")? {
        let (worker_key, worker_data) = worker_entry.storage_context("get worker entry")?;
        let worker_id_bytes = worker_key.value();
        if worker_id_bytes.len() == 8 {
            let worker_id = u64::from_le_bytes(worker_id_bytes.try_into().unwrap());
            
            // Check if worker is online
            if let Some(status_data) = status_table.get(worker_id_bytes).storage_context("get worker status")? {
                if status_data.value().len() == 1 && status_data.value()[0] == WorkerStatus::Online as u8 {
                    // Check capabilities
                    let worker_info: (String, WorkerCapabilities) = bincode::deserialize(worker_data.value())
                        .map_err(|e| crate::error::BlixardError::Serialization {
                            operation: "deserialize worker info".to_string(),
                            source: Box::new(e),
                        })?;
                    
                    let (_, capabilities) = worker_info;
                    
                    // Check if worker has sufficient resources
                    if capabilities.cpu_cores >= task_spec.resources.cpu_cores 
                        && capabilities.memory_mb >= task_spec.resources.memory_mb {
                        available_workers.push(worker_id);
                    }
                }
            }
        }
    }

    if available_workers.is_empty() {
        return Err(crate::error::BlixardError::Internal {
            message: "No available workers found for task".to_string(),
        });
    }

    // Simple round-robin: use task_id hash to pick a worker
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    task_id.hash(&mut hasher);
    let index = (hasher.finish() as usize) % available_workers.len();
    
    Ok(available_workers[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_spec() {
        // Valid task spec
        let valid_spec = TaskSpec {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 100,
                disk_gb: 1,
                required_features: vec![],
            },
            timeout_secs: 30,
        };
        assert!(validate_task_spec(&valid_spec).is_ok());

        // Invalid: empty command
        let mut invalid_spec = valid_spec.clone();
        invalid_spec.command = String::new();
        assert!(validate_task_spec(&invalid_spec).is_err());

        // Invalid: zero CPU cores
        let mut invalid_spec = valid_spec.clone();
        invalid_spec.resources.cpu_cores = 0;
        assert!(validate_task_spec(&invalid_spec).is_err());

        // Invalid: zero memory
        let mut invalid_spec = valid_spec.clone();
        invalid_spec.resources.memory_mb = 0;
        assert!(validate_task_spec(&invalid_spec).is_err());

        // Invalid: zero timeout
        let mut invalid_spec = valid_spec.clone();
        invalid_spec.timeout_secs = 0;
        assert!(validate_task_spec(&invalid_spec).is_err());
    }

    #[test]
    fn test_create_batch_proposal() {
        let proposals = vec![
            ProposalData::RegisterWorker {
                node_id: 1,
                address: "localhost:8080".to_string(),
                capabilities: WorkerCapabilities {
                    cpu_cores: 4,
                    memory_mb: 8192,
                    features: vec![],
                },
                topology: crate::types::NodeTopology::default(),
            },
            ProposalData::RegisterWorker {
                node_id: 2,
                address: "localhost:8081".to_string(),
                capabilities: WorkerCapabilities {
                    cpu_cores: 2,
                    memory_mb: 4096,
                    features: vec![],
                },
                topology: crate::types::NodeTopology::default(),
            },
        ];

        let batch = create_batch_proposal(proposals.clone()).unwrap();
        match batch {
            ProposalData::Batch(batch_proposals) => {
                assert_eq!(batch_proposals.len(), 2);
            }
            _ => panic!("Expected batch proposal"),
        }

        // Test empty batch
        assert!(create_batch_proposal(vec![]).is_err());

        // Test nested batch
        let nested_batch = vec![ProposalData::Batch(proposals)];
        assert!(create_batch_proposal(nested_batch).is_err());
    }

    #[test]
    fn test_proposal_serialization() {
        let proposal = ProposalData::RegisterWorker {
            node_id: 1,
            address: "localhost:8080".to_string(),
            capabilities: WorkerCapabilities {
                cpu_cores: 4,
                memory_mb: 8192,
                features: vec!["gpu".to_string()],
            },
            topology: crate::types::NodeTopology::default(),
        };

        let serialized = serialize_proposal_data(&proposal).unwrap();
        let deserialized = deserialize_proposal_data(&serialized).unwrap();

        match (proposal, deserialized) {
            (ProposalData::RegisterWorker { node_id: id1, .. }, ProposalData::RegisterWorker { node_id: id2, .. }) => {
                assert_eq!(id1, id2);
            }
            _ => panic!("Serialization/deserialization failed"),
        }
    }
}