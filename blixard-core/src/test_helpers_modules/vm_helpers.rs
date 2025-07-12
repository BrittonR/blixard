// Placeholder for VM helper functions
// TODO: Extract VM configuration and database utilities from original test_helpers.rs

use crate::types::VmConfig;

/// Create a standard VM configuration for testing
pub fn create_vm_config(name: &str) -> VmConfig {
    VmConfig {
        name: name.to_string(),
        config_path: format!("/tmp/{}.nix", name),
        vcpus: 2,
        memory: 1024,
        tenant_id: "test-tenant".to_string(),
        ip_address: None,
        metadata: None,
        anti_affinity: None,
        priority: 500,
        preemptible: true,
        locality_preference: Default::default(),
        health_check_config: None,
    }
}

/// VM test helper utilities
pub struct VmTestHelper;

impl VmTestHelper {
    /// Create a test VM configuration with custom parameters
    pub fn create_custom_vm(name: &str, vcpus: u32, memory: u32) -> VmConfig {
        VmConfig {
            name: name.to_string(),
            config_path: format!("/tmp/{}.nix", name),
            vcpus,
            memory,
            tenant_id: "test-tenant".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        }
    }
}
