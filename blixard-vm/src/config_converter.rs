//! VM configuration conversion module
//!
//! This module handles the conversion between core VM configurations and
//! microvm.nix-specific configurations, including network setup and Nix module handling.

use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use blixard_core::{
    error::{BlixardError, BlixardResult},
    types::VmConfig as CoreVmConfig,
};

use crate::{
    ip_pool::{IpAddressPool, constants as ip_constants},
    types as vm_types,
};

/// Configuration converter for VM configurations
pub struct VmConfigConverter {
    ip_pool: Arc<RwLock<IpAddressPool>>,
}

impl VmConfigConverter {
    /// Create a new configuration converter
    pub fn new(ip_pool: Arc<RwLock<IpAddressPool>>) -> Self {
        Self { ip_pool }
    }

    /// Convert core VM config to enhanced VM config with routed networking
    pub async fn convert_config(
        &self,
        core_config: &CoreVmConfig,
    ) -> BlixardResult<vm_types::VmConfig> {
        // Get network configuration
        let network_config = self.get_network_config(core_config).await?;
        
        // Extract VM index from IP
        let vm_index = Self::extract_vm_index(&network_config.ip)?;
        
        // Generate interface ID
        let interface_id = Self::generate_interface_id(&core_config.name);
        
        // Process Nix modules if using P2P image
        let (nixos_modules, kernel) = self.process_nix_image_config(core_config);

        // Build the final VM configuration
        Ok(self.build_vm_config(
            core_config,
            network_config,
            vm_index,
            interface_id,
            nixos_modules,
            kernel,
        ))
    }

    /// Allocate a unique IP address for a VM
    pub async fn allocate_vm_ip(
        &self,
        vm_name: &str,
    ) -> BlixardResult<NetworkConfig> {
        let mut ip_pool = self.ip_pool.write().await;

        // Allocate IP address
        let vm_ip = ip_pool.allocate_ip()?;

        // Generate MAC address
        let mac_address = ip_pool.generate_mac_address(vm_name);

        // Get network configuration
        let gateway = ip_pool.gateway().to_string();
        let subnet = ip_pool.subnet().to_string();

        Ok(NetworkConfig {
            ip: vm_ip.to_string(),
            mac: mac_address,
            gateway,
            subnet,
        })
    }

    /// Release an allocated IP address
    pub async fn release_vm_ip(&self, ip: &str) {
        if let Ok(ip_addr) = ip.parse::<Ipv4Addr>() {
            let mut ip_pool = self.ip_pool.write().await;
            ip_pool.release_ip(ip_addr);
        }
    }

    /// Get network configuration for a VM
    async fn get_network_config(
        &self,
        core_config: &CoreVmConfig,
    ) -> BlixardResult<NetworkConfig> {
        if let Some(ip) = &core_config.ip_address {
            // Validate provided IP
            let _ip_addr = ip.parse::<Ipv4Addr>()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Invalid IP address format: {}", e),
                })?;
            
            // For centrally managed IPs, use default network config
            let mac_address = self.ip_pool.read().await.generate_mac_address(&core_config.name);
            
            Ok(NetworkConfig {
                ip: ip.clone(),
                mac: mac_address,
                gateway: ip_constants::DEFAULT_GATEWAY.to_string(),
                subnet: ip_constants::DEFAULT_SUBNET.to_string(),
            })
        } else {
            // Fallback to local allocation
            self.allocate_vm_ip(&core_config.name).await
        }
    }

    /// Extract VM index from IP address (last octet)
    fn extract_vm_index(ip: &str) -> BlixardResult<u32> {
        let ip_addr = ip.parse::<Ipv4Addr>()
            .map_err(|e| BlixardError::Internal {
                message: format!("Invalid IP address format: {}", e),
            })?;
        
        Ok(ip_addr.octets()[3] as u32)
    }

    /// Generate interface ID for a VM
    fn generate_interface_id(vm_name: &str) -> String {
        format!("vm-{}", vm_name)
    }

    /// Process Nix image configuration if present
    fn process_nix_image_config(
        &self,
        core_config: &CoreVmConfig,
    ) -> (Vec<vm_types::NixModule>, Option<vm_types::KernelConfig>) {
        if let Some(metadata) = &core_config.metadata {
            if let Some(nix_image_id) = metadata.get("nix_image_id") {
                info!(
                    "VM {} is using Nix image: {}",
                    core_config.name, nix_image_id
                );

                let nixos_module = vm_types::NixModule::Inline(format!(
                    r#"
                    # Reference P2P-distributed Nix image
                    # Image ID: {}
                    # This will be resolved to actual paths during VM start
                    boot.loader.grub.enable = false;
                    boot.isContainer = true;
                "#,
                    nix_image_id
                ));

                return (vec![nixos_module], None);
            }
        }
        
        (vec![], None)
    }

    /// Build the final VM configuration
    fn build_vm_config(
        &self,
        core_config: &CoreVmConfig,
        network_config: NetworkConfig,
        vm_index: u32,
        interface_id: String,
        nixos_modules: Vec<vm_types::NixModule>,
        kernel: Option<vm_types::KernelConfig>,
    ) -> vm_types::VmConfig {
        vm_types::VmConfig {
            name: core_config.name.clone(),
            vm_index,
            hypervisor: vm_types::Hypervisor::Qemu,
            vcpus: core_config.vcpus,
            memory: core_config.memory,
            init_command: None,
            kernel,
            networks: vec![vm_types::NetworkConfig::Routed {
                id: interface_id,
                mac: network_config.mac,
                ip: network_config.ip,
                gateway: network_config.gateway,
                subnet: network_config.subnet,
            }],
            volumes: vec![],
            nixos_modules,
            flake_modules: vec![],
        }
    }
}

/// Network configuration for a VM
pub struct NetworkConfig {
    pub ip: String,
    pub mac: String,
    pub gateway: String,
    pub subnet: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_config_conversion() {
        let ip_pool = Arc::new(RwLock::new(IpAddressPool::new()));
        let converter = VmConfigConverter::new(ip_pool);

        let core_config = CoreVmConfig {
            name: "test-vm".to_string(),
            config_path: "/test/path".to_string(),
            vcpus: 2,
            memory: 1024,
            tenant_id: "test-tenant".to_string(),
            ip_address: Some("10.0.0.20".to_string()),
            metadata: None,
            anti_affinity: None,
            priority: 100,
            preemptible: false,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        let vm_config = converter.convert_config(&core_config).await.unwrap();
        
        assert_eq!(vm_config.name, "test-vm");
        assert_eq!(vm_config.vcpus, 2);
        assert_eq!(vm_config.memory, 1024);
        assert_eq!(vm_config.vm_index, 20);
        
        // Check network configuration
        match &vm_config.networks[0] {
            vm_types::NetworkConfig::Routed { ip, .. } => {
                assert_eq!(ip, "10.0.0.20");
            }
            _ => panic!("Expected Routed network config"),
        }
    }

    #[tokio::test]
    async fn test_nix_image_processing() {
        let ip_pool = Arc::new(RwLock::new(IpAddressPool::new()));
        let converter = VmConfigConverter::new(ip_pool);

        let mut metadata = HashMap::new();
        metadata.insert("nix_image_id".to_string(), "abc123".to_string());

        let core_config = CoreVmConfig {
            name: "nix-vm".to_string(),
            config_path: "/test/path".to_string(),
            vcpus: 1,
            memory: 512,
            tenant_id: "test-tenant".to_string(),
            ip_address: None,
            metadata: Some(metadata),
            anti_affinity: None,
            priority: 100,
            preemptible: false,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        let vm_config = converter.convert_config(&core_config).await.unwrap();
        
        assert!(!vm_config.nixos_modules.is_empty());
        
        match &vm_config.nixos_modules[0] {
            vm_types::NixModule::Inline(content) => {
                assert!(content.contains("abc123"));
            }
            _ => panic!("Expected inline Nix module"),
        }
    }
}