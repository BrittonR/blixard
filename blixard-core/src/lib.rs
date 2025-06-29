pub mod error;
pub mod types;
pub mod node;
pub mod node_shared;
pub mod grpc_server;
pub mod grpc_server_legacy;
pub mod raft_codec;
pub mod raft_manager;
pub mod storage;
pub mod vm_backend;
pub mod vm_scheduler;
pub mod network_isolated_backend;
pub mod peer_connector;
pub mod config_v2;
pub mod config_watcher;
pub mod config_global;
pub mod metrics_otel;
pub mod metrics_server;
pub mod tracing_otel;
pub mod resource_quotas;
pub mod quota_manager;
pub mod security;
pub mod grpc_security;
pub mod cert_generator;
// P2P implementation
pub mod iroh_transport;
pub mod p2p_image_store;
pub mod nix_image_store;
pub mod p2p_manager;
pub mod cluster_state;
pub mod transport;
pub mod vm_health_monitor;
pub mod vm_auto_recovery;
pub mod vm_network_isolation;
pub mod abstractions;
pub mod common;

// Test helpers are exposed for integration tests
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers_concurrent;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_message_filter;

// Include the generated proto code
pub mod proto {
    tonic::include_proto!("blixard");
}