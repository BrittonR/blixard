pub mod anti_affinity;
pub mod config_global;
pub mod config_v2;
pub mod config_watcher;
pub mod error;
pub mod network_isolated_backend;
pub mod node;
pub mod node_shared;
pub mod raft_batch_processor;
pub mod raft_codec;
pub mod raft_manager;
pub mod resource_management;
pub mod resource_admission;
pub mod resource_monitor;
pub mod resource_collection;
pub mod raft_storage;
pub mod storage;
pub mod ip_pool;
pub mod ip_pool_manager;
pub mod ip_allocation_service;
pub mod types;
pub mod vm_backend;
pub mod vm_scheduler;
pub mod vm_state_persistence;
// pub mod config_hot_reload; // Temporarily disabled for compilation
// pub mod backup_manager; // Temporarily disabled for compilation
pub mod transaction_log;
// pub mod audit_log; // Temporarily disabled for compilation
// Temporarily disabled: uses tonic which we're removing
// pub mod audit_integration;
// pub mod backup_replication; // Temporarily disabled for compilation
// pub mod remediation_engine; // Temporarily disabled for compilation
// pub mod remediation_raft; // Temporarily disabled for compilation
pub mod metrics_otel;
pub mod metrics_server;
// Temporarily disabled: uses tonic which we're removing
// pub mod tracing_otel;
pub mod cedar_authz;
pub mod cert_generator;
pub mod observability;
pub mod quota_manager;
pub mod quota_system;
pub mod resource_quotas;
pub mod security;
// P2P implementation
pub mod abstractions;
pub mod cluster_state;
pub mod iroh_transport;
pub mod iroh_transport_v2;
pub mod iroh_transport_v3;
pub mod linearizability;
pub mod linearizability_framework;
pub mod nix_image_store;
pub mod p2p_health_check;
pub mod p2p_image_store;
pub mod p2p_manager;
pub mod p2p_monitor;
pub mod p2p_monitor_otel;
pub mod transport;
pub mod vm_auto_recovery;
pub mod vm_health_monitor;
pub mod vm_health_types;
pub mod vm_network_isolation;
// Temporarily disabled: uses gRPC/tonic which we're removing
pub mod common;
pub mod discovery;

// Test helpers are exposed for integration tests
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

// Temporarily disabled: uses gRPC which we're removing
// #[cfg(any(test, feature = "test-helpers"))]
// pub mod test_helpers_concurrent;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_message_filter;

// Native Rust types for Iroh transport
pub mod iroh_types;

// Failpoint support for fault injection
#[cfg(feature = "failpoints")]
pub mod failpoints;

// VOPR fuzzer for deterministic testing
#[cfg(any(test, feature = "vopr"))]
pub mod vopr;
