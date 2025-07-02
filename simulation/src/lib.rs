//! Simulation testing library for Blixard
//!
//! This crate provides deterministic testing capabilities using MadSim.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Include the generated proto code
pub mod proto {
    tonic::include_proto!("blixard");
}

// Re-export commonly used types for tests
pub use proto::*;

// Test helpers module
pub mod test_helpers;

// Additional types for simulation tests
pub mod types;

// Re-export commonly used items
pub use test_helpers::{TestNode, TestCluster, PortAllocator, timing, wait_for_condition};
pub use types::{SharedNodeState, PeerInfo, RaftStatus, TaskSpec, ResourceRequirements, BlixardError, BlixardResult};

// Define minimal types needed for simulation tests
// These mirror the types in blixard-core but are independent

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: u64,
    pub bind_addr: std::net::SocketAddr,
    pub data_dir: String,
    pub join_addr: Option<String>,
    pub use_tailscale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub memory: u64,
    pub vcpus: u32,
    pub disk_size: u64,
    pub image: String,
    pub network_interfaces: Vec<NetworkInterface>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    Creating,
    Running,
    Stopped,
    Deleted,
    Error,
}

// Common test utilities
pub mod test_utils {
    use super::*;
    use std::sync::atomic::{AtomicU16, Ordering};
    
    static PORT_COUNTER: AtomicU16 = AtomicU16::new(20000);
    
    pub fn get_available_port() -> u16 {
        PORT_COUNTER.fetch_add(1, Ordering::SeqCst)
    }
    
    pub fn test_node_config(id: u64) -> NodeConfig {
        NodeConfig {
            id,
            bind_addr: format!("127.0.0.1:{}", get_available_port()).parse().unwrap(),
            data_dir: format!("/tmp/blixard-test-{}", id),
            join_addr: None,
            use_tailscale: false,
        }
    }
}