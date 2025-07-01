pub mod error;
pub mod types;
pub mod node;
pub mod node_shared;
pub mod raft_codec;
pub mod raft_manager;
pub mod raft_batch_processor;
pub mod storage;
pub mod vm_backend;
pub mod vm_scheduler;
pub mod anti_affinity;
pub mod resource_management;
pub mod network_isolated_backend;
pub mod config_v2;
pub mod config_watcher;
pub mod config_global;
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
pub mod resource_quotas;
pub mod quota_manager;
pub mod security;
pub mod cert_generator;
pub mod cedar_authz;
pub mod quota_system;
pub mod observability;
// P2P implementation
pub mod iroh_transport;
pub mod iroh_transport_v2;
pub mod p2p_image_store;
pub mod nix_image_store;
pub mod p2p_manager;
pub mod cluster_state;
pub mod transport;
pub mod vm_health_monitor;
pub mod vm_auto_recovery;
pub mod vm_network_isolation;
pub mod abstractions;
// Temporarily disabled: uses gRPC/tonic which we're removing
// pub mod common;
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