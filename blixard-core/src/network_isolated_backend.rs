//! Network-isolated VM backend wrapper
//!
//! This module provides a wrapper around any VM backend that automatically
//! applies network isolation rules when VMs are created/destroyed.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::error::{BlixardError, BlixardResult};
use crate::types::{VmConfig, VmStatus};
use crate::vm_backend::VmBackend;
use crate::vm_network_isolation::{
    check_firewall_privileges, NetworkIsolationConfig, NetworkIsolationManager,
};

/// A VM backend wrapper that adds network isolation
pub struct NetworkIsolatedBackend {
    /// The underlying VM backend
    inner: Arc<dyn VmBackend>,
    /// Network isolation manager
    isolation_manager: Arc<Mutex<NetworkIsolationManager>>,
    /// Whether network isolation is enabled
    enabled: bool,
}

impl NetworkIsolatedBackend {
    /// Create a new network-isolated backend
    pub async fn new(
        inner: Arc<dyn VmBackend>,
        isolation_config: NetworkIsolationConfig,
    ) -> BlixardResult<Self> {
        // Check if we have privileges for firewall management
        let enabled = match check_firewall_privileges() {
            Ok(()) => {
                info!("Network isolation enabled - sufficient privileges detected");
                true
            }
            Err(e) => {
                warn!("Network isolation disabled: {}", e);
                warn!("VMs will run without network isolation. To enable, run with root or CAP_NET_ADMIN");
                false
            }
        };

        let isolation_manager =
            Arc::new(Mutex::new(NetworkIsolationManager::new(isolation_config)));

        if enabled {
            // Initialize firewall rules
            isolation_manager.lock().await.initialize().await?;
        }

        Ok(Self {
            inner,
            isolation_manager,
            enabled,
        })
    }
}

#[async_trait]
impl VmBackend for NetworkIsolatedBackend {
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        // First create the VM using the inner backend
        self.inner.create_vm(config, node_id).await?;

        // Then apply network isolation if enabled
        if self.enabled {
            // Try to get VM IP address
            let vm_ip = if let Some(ip) = &config.ip_address {
                ip.clone()
            } else if let Ok(Some(ip)) = self.inner.get_vm_ip(&config.name).await {
                ip
            } else {
                // Generate a default IP based on VM name hash
                let hash = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                let mut hasher = hash;
                config.name.hash(&mut hasher);
                let hash_value = hasher.finish();

                // Generate IP in 10.0.x.x range (avoiding .0 and .255)
                let octet3 = ((hash_value >> 8) & 0xFF) as u8;
                let octet4 = ((hash_value & 0xFF) % 254 + 1) as u8;
                format!("10.0.{}.{}", octet3, octet4)
            };

            info!(
                "Applying network isolation for VM {} with IP {}",
                config.name, vm_ip
            );

            if let Err(e) = self
                .isolation_manager
                .lock()
                .await
                .add_vm_rules(config, &vm_ip, &config.tenant_id)
                .await
            {
                warn!(
                    "Failed to apply network isolation for VM {}: {}",
                    config.name, e
                );
                // Don't fail VM creation if isolation fails - just log the error
            }
        }

        Ok(())
    }

    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        self.inner.start_vm(name).await
    }

    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        self.inner.stop_vm(name).await
    }

    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        // Get VM info before deletion
        let vm_info = self
            .list_vms()
            .await?
            .into_iter()
            .find(|(config, _)| config.name == name);

        // Delete the VM
        self.inner.delete_vm(name).await?;

        // Remove network isolation rules if enabled
        if self.enabled {
            if let Some((config, _)) = vm_info {
                let vm_ip = if let Some(ip) = &config.ip_address {
                    ip.clone()
                } else if let Ok(Some(ip)) = self.inner.get_vm_ip(name).await {
                    ip
                } else {
                    // Use same hash-based IP generation
                    let hash = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    let mut hasher = hash;
                    name.hash(&mut hasher);
                    let hash_value = hasher.finish();
                    let octet3 = ((hash_value >> 8) & 0xFF) as u8;
                    let octet4 = ((hash_value & 0xFF) % 254 + 1) as u8;
                    format!("10.0.{}.{}", octet3, octet4)
                };

                info!(
                    "Removing network isolation for VM {} with IP {}",
                    name, vm_ip
                );

                if let Err(e) = self
                    .isolation_manager
                    .lock()
                    .await
                    .remove_vm_rules(name, &vm_ip, &config.tenant_id)
                    .await
                {
                    warn!("Failed to remove network isolation for VM {}: {}", name, e);
                }
            }
        }

        Ok(())
    }

    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        self.inner.update_vm_status(name, status).await
    }

    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        self.inner.get_vm_status(name).await
    }

    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        self.inner.list_vms().await
    }

    async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        self.inner.get_vm_ip(name).await
    }

    async fn migrate_vm(&self, name: &str, target_node_id: u64, live: bool) -> BlixardResult<()> {
        // Migration with network isolation is complex - would need to coordinate
        // rule removal on source and addition on target
        if self.enabled {
            return Err(BlixardError::NotImplemented {
                feature: "VM migration with network isolation".to_string(),
            });
        }

        self.inner.migrate_vm(name, target_node_id, live).await
    }
}

/// Create a network-isolated backend with default configuration
pub async fn create_isolated_backend(
    inner: Arc<dyn VmBackend>,
) -> BlixardResult<Arc<dyn VmBackend>> {
    let config = NetworkIsolationConfig::default();
    let backend = NetworkIsolatedBackend::new(inner, config).await?;
    Ok(Arc::new(backend))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm_backend::MockVmBackend;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_network_isolated_backend() {
        // Initialize tracing for the test
        let _ = tracing_subscriber::fmt::try_init();

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(db_path).unwrap());
        let mock_backend = Arc::new(MockVmBackend::new(database));
        let config = NetworkIsolationConfig {
            backend: crate::vm_network_isolation::FirewallBackend::Nftables,
            cluster_networks: vec!["172.16.0.0/12".to_string()],
            vm_network: "10.0.0.0/16".to_string(),
            inter_vm_isolation: false,
            tenant_isolation: true,
            allow_internet: true,
            allowed_ports: vec![],
        };

        // This test will only actually apply rules if run with privileges
        // The initialization might fail if nft/iptables is not available, even if we have privileges
        let isolated_backend = match NetworkIsolatedBackend::new(mock_backend.clone(), config).await
        {
            Ok(backend) => backend,
            Err(e) => {
                // If we can't initialize network isolation (e.g., nft not installed), that's ok
                eprintln!(
                    "Network isolation initialization failed (expected in test environment): {}",
                    e
                );
                // Create a backend without isolation
                NetworkIsolatedBackend {
                    inner: mock_backend,
                    isolation_manager: Arc::new(Mutex::new(NetworkIsolationManager::new(
                        NetworkIsolationConfig::default(),
                    ))),
                    enabled: false,
                }
            }
        };

        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.conf".to_string(),
            vcpus: 2,
            memory: 1024,
            tenant_id: "tenant1".to_string(),
            ip_address: Some("10.0.0.10".to_string()),
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        // Should succeed regardless of privilege level
        // Note: MockVmBackend doesn't actually store VMs, it just logs operations
        isolated_backend.create_vm(&vm_config, 1).await.unwrap();

        // The test passes if we can create a VM without errors
        // Network isolation rules may or may not be applied depending on privileges
    }
}
