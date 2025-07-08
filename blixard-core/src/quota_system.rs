//! Resource quota system for multi-tenant environments
//!
//! This module provides resource quotas and limits for different tenants
//! to ensure fair resource allocation and prevent resource exhaustion.

use crate::error::{BlixardError, BlixardResult};
use crate::types::VmConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Resource quota for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    /// Tenant ID
    pub tenant_id: String,

    /// Maximum number of VMs
    pub max_vms: u32,

    /// Maximum total vCPUs
    pub max_vcpus: u32,

    /// Maximum total memory in MB
    pub max_memory: u32,

    /// Maximum total disk in GB
    pub max_disk: u32,

    /// Maximum number of tasks
    pub max_tasks: u32,

    /// Whether the tenant is enabled
    pub enabled: bool,
}

/// Current resource usage for a tenant
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TenantUsage {
    /// Number of active VMs
    pub vm_count: u32,

    /// Total vCPUs in use
    pub vcpu_count: u32,

    /// Total memory in use (MB)
    pub memory_mb: u32,

    /// Total disk in use (GB)
    pub disk_gb: u32,

    /// Number of active tasks
    pub task_count: u32,
}

/// Quota manager for enforcing resource limits
#[derive(Debug)]
pub struct QuotaManager {
    /// Tenant quotas
    quotas: Arc<RwLock<HashMap<String, TenantQuota>>>,

    /// Current usage per tenant
    usage: Arc<RwLock<HashMap<String, TenantUsage>>>,
}

impl QuotaManager {
    /// Create a new quota manager
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(RwLock::new(HashMap::new())),
            usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set quota for a tenant
    pub async fn set_quota(&self, quota: TenantQuota) -> BlixardResult<()> {
        let mut quotas = self.quotas.write().await;
        info!("Setting quota for tenant {}: {:?}", quota.tenant_id, quota);
        quotas.insert(quota.tenant_id.clone(), quota);
        Ok(())
    }

    /// Get quota for a tenant
    pub async fn get_quota(&self, tenant_id: &str) -> Option<TenantQuota> {
        let quotas = self.quotas.read().await;
        quotas.get(tenant_id).cloned()
    }

    /// Get current usage for a tenant
    pub async fn get_usage(&self, tenant_id: &str) -> TenantUsage {
        let usage = self.usage.read().await;
        usage.get(tenant_id).cloned().unwrap_or_default()
    }

    /// Check if a VM creation would exceed quota
    pub async fn check_vm_quota(&self, vm_config: &VmConfig) -> BlixardResult<()> {
        let tenant_id = &vm_config.tenant_id;

        // Get quota and usage
        let quotas = self.quotas.read().await;
        let quota = match quotas.get(tenant_id) {
            Some(q) if q.enabled => q,
            Some(_) => {
                return Err(BlixardError::QuotaExceeded {
                    resource: "tenant".to_string(),
                    limit: 0,
                    requested: 1,
                });
            }
            None => {
                // No quota set means unlimited
                return Ok(());
            }
        };

        let usage = self.usage.read().await;
        let current_usage = usage.get(tenant_id).cloned().unwrap_or_default();

        // Check VM count
        if current_usage.vm_count >= quota.max_vms {
            return Err(BlixardError::QuotaExceeded {
                resource: "VMs".to_string(),
                limit: quota.max_vms as u64,
                requested: (current_usage.vm_count + 1) as u64,
            });
        }

        // Check vCPU count
        if current_usage.vcpu_count + vm_config.vcpus > quota.max_vcpus {
            return Err(BlixardError::QuotaExceeded {
                resource: "vCPUs".to_string(),
                limit: quota.max_vcpus as u64,
                requested: (current_usage.vcpu_count + vm_config.vcpus) as u64,
            });
        }

        // Check memory
        if current_usage.memory_mb + vm_config.memory > quota.max_memory {
            return Err(BlixardError::QuotaExceeded {
                resource: "memory".to_string(),
                limit: quota.max_memory as u64,
                requested: (current_usage.memory_mb + vm_config.memory) as u64,
            });
        }

        Ok(())
    }

    /// Reserve resources for a VM
    pub async fn reserve_vm_resources(&self, vm_config: &VmConfig) -> BlixardResult<()> {
        // First check if we can reserve
        self.check_vm_quota(vm_config).await?;

        // Then update usage
        let mut usage = self.usage.write().await;
        let tenant_usage = usage.entry(vm_config.tenant_id.clone()).or_default();

        tenant_usage.vm_count += 1;
        tenant_usage.vcpu_count += vm_config.vcpus;
        tenant_usage.memory_mb += vm_config.memory;

        info!(
            "Reserved resources for VM {} in tenant {}: {} vCPUs, {} MB memory",
            vm_config.name, vm_config.tenant_id, vm_config.vcpus, vm_config.memory
        );

        Ok(())
    }

    /// Release resources for a VM
    pub async fn release_vm_resources(&self, vm_config: &VmConfig) -> BlixardResult<()> {
        let mut usage = self.usage.write().await;

        if let Some(tenant_usage) = usage.get_mut(&vm_config.tenant_id) {
            // Saturating subtraction to prevent underflow
            tenant_usage.vm_count = tenant_usage.vm_count.saturating_sub(1);
            tenant_usage.vcpu_count = tenant_usage.vcpu_count.saturating_sub(vm_config.vcpus);
            tenant_usage.memory_mb = tenant_usage.memory_mb.saturating_sub(vm_config.memory);

            info!(
                "Released resources for VM {} in tenant {}: {} vCPUs, {} MB memory",
                vm_config.name, vm_config.tenant_id, vm_config.vcpus, vm_config.memory
            );
        }

        Ok(())
    }

    /// Check if a task creation would exceed quota
    pub async fn check_task_quota(&self, tenant_id: &str) -> BlixardResult<()> {
        let quotas = self.quotas.read().await;
        let quota = match quotas.get(tenant_id) {
            Some(q) if q.enabled => q,
            Some(_) => {
                return Err(BlixardError::QuotaExceeded {
                    resource: "tenant".to_string(),
                    limit: 0,
                    requested: 1,
                });
            }
            None => return Ok(()), // No quota means unlimited
        };

        let usage = self.usage.read().await;
        let current_usage = usage.get(tenant_id).cloned().unwrap_or_default();

        if current_usage.task_count >= quota.max_tasks {
            return Err(BlixardError::QuotaExceeded {
                resource: "tasks".to_string(),
                limit: quota.max_tasks as u64,
                requested: (current_usage.task_count + 1) as u64,
            });
        }

        Ok(())
    }

    /// Reserve a task slot
    pub async fn reserve_task(&self, tenant_id: &str) -> BlixardResult<()> {
        self.check_task_quota(tenant_id).await?;

        let mut usage = self.usage.write().await;
        let tenant_usage = usage.entry(tenant_id.to_string()).or_default();
        tenant_usage.task_count += 1;

        Ok(())
    }

    /// Release a task slot
    pub async fn release_task(&self, tenant_id: &str) -> BlixardResult<()> {
        let mut usage = self.usage.write().await;

        if let Some(tenant_usage) = usage.get_mut(tenant_id) {
            tenant_usage.task_count = tenant_usage.task_count.saturating_sub(1);
        }

        Ok(())
    }
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Create default quotas for testing
pub fn create_default_quota(tenant_id: &str) -> TenantQuota {
    TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 10,
        max_vcpus: 40,
        max_memory: 81920, // 80 GB
        max_disk: 1000,    // 1 TB
        max_tasks: 100,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_enforcement() {
        let manager = QuotaManager::new();

        // Set a quota
        let quota = TenantQuota {
            tenant_id: "test-tenant".to_string(),
            max_vms: 2,
            max_vcpus: 4,
            max_memory: 4096,
            max_disk: 100,
            max_tasks: 10,
            enabled: true,
        };
        manager.set_quota(quota).await.unwrap();

        // Create VM configs
        let vm1 = VmConfig {
            name: "vm1".to_string(),
            config_path: "/path".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "test-tenant".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        let vm2 = VmConfig {
            name: "vm2".to_string(),
            config_path: "/path".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "test-tenant".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        let vm3 = VmConfig {
            name: "vm3".to_string(),
            config_path: "/path".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "test-tenant".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        // First two VMs should succeed
        manager.reserve_vm_resources(&vm1).await.unwrap();
        manager.reserve_vm_resources(&vm2).await.unwrap();

        // Third VM should fail (exceeds VM count)
        let result = manager.reserve_vm_resources(&vm3).await;
        assert!(result.is_err());

        // Release one VM
        manager.release_vm_resources(&vm1).await.unwrap();

        // Now third VM should succeed
        manager.reserve_vm_resources(&vm3).await.unwrap();
    }
}
