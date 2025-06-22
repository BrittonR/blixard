pub mod error;
pub mod types;
pub mod node;
pub mod node_shared;
pub mod grpc_server;
pub mod raft_codec;
pub mod raft_manager;
pub mod storage;
pub mod vm_manager;
pub mod peer_connector;
pub mod config;

// Test helpers are exposed for integration tests
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

// Include the generated proto code
pub mod proto {
    tonic::include_proto!("blixard");
}