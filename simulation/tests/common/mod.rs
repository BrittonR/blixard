// Common test utilities and helpers for integration tests

use std::time::Duration;
use blixard_core::{
    error::{BlixardError, Result as BlixardResult},
    types::{NodeConfig, VmConfig}
};

// Re-export timing utilities for easier access
pub mod test_timing;

// Raft-specific test utilities
pub mod raft_test_utils;

/// Test configuration constants
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const TEST_PORT_BASE: u16 = 7000;

/// Helper to create a test node configuration
pub fn test_node_config(id: u64, port: u16) -> NodeConfig {
    NodeConfig {
        id,
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        data_dir: format!("/tmp/blixard-test-{}", id),
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
    }
}

/// Helper to create a test VM configuration
pub fn test_vm_config(name: &str) -> VmConfig {
    VmConfig {
        name: name.to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 512,
        vcpus: 1,
        tenant_id: "default".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
    }
}

/// Wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(mut condition: F, timeout: Duration) -> BlixardResult<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    // Use consistent time abstractions for deterministic testing
    #[cfg(madsim)]
    use madsim::time::{Instant, sleep};
    #[cfg(not(madsim))]
    use std::time::Instant;
    #[cfg(not(madsim))]
    use tokio::time::sleep;
    
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    
    Err(BlixardError::SystemError(
        format!("Timeout waiting for condition after {:?}", timeout)
    ))
}

/// Property testing utilities
pub mod proptest_utils {
    use proptest::prelude::*;
    
    /// Strategy for generating valid node IDs
    pub fn node_id_strategy() -> impl Strategy<Value = u64> {
        1u64..=1000u64
    }
    
    /// Strategy for generating valid port numbers
    pub fn port_strategy() -> impl Strategy<Value = u16> {
        7000u16..=8000u16
    }
    
    /// Strategy for generating VM names
    pub fn vm_name_strategy() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9-]*".prop_filter("Must be valid VM name", |s| {
            s.len() >= 2 && s.len() <= 32
        })
    }
}