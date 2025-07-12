//! Storage layer modules
//!
//! This module provides database storage abstractions and utilities for Blixard.

pub mod database_transaction;
pub mod transaction_demo;

pub use database_transaction::{
    DatabaseTransaction, ToRaftError, TransactionExecutor, TransactionType,
};

// Re-export raft storage items for backwards compatibility
pub use crate::raft_storage::{
    init_database_tables, RedbRaftStorage, CLUSTER_STATE_TABLE, IP_ALLOCATION_TABLE, IP_POOL_TABLE,
    NODE_TOPOLOGY_TABLE, RAFT_LOG_TABLE, RESOURCE_POLICY_TABLE, TASK_ASSIGNMENT_TABLE, TASK_RESULT_TABLE,
    TASK_TABLE, TENANT_QUOTA_TABLE, VM_IP_MAPPING_TABLE, VM_STATE_TABLE, WORKER_STATUS_TABLE,
    WORKER_TABLE,
};
