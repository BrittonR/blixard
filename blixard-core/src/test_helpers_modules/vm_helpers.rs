// Placeholder for VM helper functions
// TODO: Extract VM configuration and database utilities from original test_helpers.rs

use crate::error::BlixardResult;
use crate::types::VmConfig;

/// Create a standard VM configuration for testing
pub fn create_vm_config(name: &str) -> VmConfig {
    VmConfig {
        name: name.to_string(),
        vcpus: 2,
        memory: 1024,
        tenant_id: "test-tenant".to_string(),
        assigned_node: None,
        metadata: None,
        anti_affinity: None,
        locality_preferences: None,
        priority: None,
    }
}

/// VM test helper utilities
pub struct VmTestHelper;

impl VmTestHelper {
    /// Create a test VM configuration with custom parameters
    pub fn create_custom_vm(name: &str, vcpus: u32, memory: u32) -> VmConfig {
        VmConfig {
            name: name.to_string(),
            vcpus,
            memory,
            tenant_id: "test-tenant".to_string(),
            assigned_node: None,
            metadata: None,
            anti_affinity: None,
            locality_preferences: None,
            priority: None,
        }
    }
}