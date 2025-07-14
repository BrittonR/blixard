//! Migration utilities for updating existing code to use unified ResourceManager interface
//!
//! This module provides utilities to help migrate existing quota management,
//! resource monitoring, and admission control code to use the new unified
//! ResourceManager trait interface.

use crate::error::{BlixardError, BlixardResult};
use crate::resource_manager::{
    CompositeResourceManager, ResourceAllocation, ResourceAllocationRequest, ResourceLimits,
    ResourceManager, ResourceType,
};
use crate::resource_managers::{create_standard_composite_manager, DefaultResourceManagerFactory};
use crate::resource_quotas::{QuotaViolation, ResourceRequest, TenantId, TenantQuota};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Migration wrapper that provides backward compatibility
#[derive(Debug)]
pub struct ResourceManagerMigrationWrapper {
    /// New unified resource managers
    composite_manager: CompositeResourceManager,
    /// Factory for creating new managers
    #[allow(dead_code)] // Factory reserved for future dynamic manager creation
    factory: Arc<DefaultResourceManagerFactory>,
    /// Migration state tracking
    migration_state: MigrationState,
}

#[derive(Debug, Clone, Default)]
struct MigrationState {
    /// Whether legacy systems have been migrated
    memory_migrated: bool,
    cpu_migrated: bool,
    disk_migrated: bool,
    /// Migration warnings issued
    warnings_issued: Vec<String>,
}


impl ResourceManagerMigrationWrapper {
    /// Create a new migration wrapper
    pub async fn new(memory_capacity: u64, cpu_cores: u32) -> BlixardResult<Self> {
        let composite_manager =
            create_standard_composite_manager(memory_capacity, cpu_cores).await?;
        let factory = Arc::new(DefaultResourceManagerFactory::new(
            memory_capacity,
            cpu_cores,
        ));

        Ok(Self {
            composite_manager,
            factory,
            migration_state: MigrationState::default(),
        })
    }

    /// Migrate from legacy quota manager to unified resource managers
    pub async fn migrate_from_quota_manager(
        &mut self,
        legacy_quotas: &HashMap<TenantId, TenantQuota>,
    ) -> BlixardResult<()> {
        info!("Migrating legacy quota manager to unified resource managers");

        for (tenant_id, quota) in legacy_quotas {
            // Convert VM limits to memory allocations
            if quota.vm_limits.max_memory_mb > 0 {
                let memory_request = ResourceAllocationRequest {
                    request_id: format!("migrated-memory-{}", tenant_id),
                    tenant_id: tenant_id.clone(),
                    resource_type: ResourceType::Memory,
                    amount: quota.vm_limits.max_memory_mb,
                    priority: quota.vm_limits.priority as u32,
                    is_temporary: false,
                    expires_at: None,
                    metadata: [
                        ("migrated_from".to_string(), "quota_manager".to_string()),
                        ("original_tenant".to_string(), tenant_id.clone()),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                if let Err(e) = self.composite_manager.allocate(memory_request).await {
                    warn!(
                        "Failed to migrate memory quota for tenant {}: {}",
                        tenant_id, e
                    );
                }
            }

            // Convert VM limits to CPU allocations
            if quota.vm_limits.max_vcpus > 0 {
                let cpu_request = ResourceAllocationRequest {
                    request_id: format!("migrated-cpu-{}", tenant_id),
                    tenant_id: tenant_id.clone(),
                    resource_type: ResourceType::Cpu,
                    amount: quota.vm_limits.max_vcpus as u64,
                    priority: quota.vm_limits.priority as u32,
                    is_temporary: false,
                    expires_at: None,
                    metadata: [
                        ("migrated_from".to_string(), "quota_manager".to_string()),
                        ("original_tenant".to_string(), tenant_id.clone()),
                        (
                            "overcommit_ratio".to_string(),
                            quota.vm_limits.overcommit_ratio.to_string(),
                        ),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                if let Err(e) = self.composite_manager.allocate(cpu_request).await {
                    warn!(
                        "Failed to migrate CPU quota for tenant {}: {}",
                        tenant_id, e
                    );
                }
            }
        }

        self.migration_state.memory_migrated = true;
        self.migration_state.cpu_migrated = true;

        info!(
            "Successfully migrated {} tenant quotas to unified resource managers",
            legacy_quotas.len()
        );
        Ok(())
    }

    /// Convert legacy ResourceRequest to new ResourceAllocationRequest
    pub fn convert_resource_request(
        &self,
        legacy_request: &ResourceRequest,
    ) -> Vec<ResourceAllocationRequest> {
        let mut requests = Vec::new();

        // Create memory allocation request
        if legacy_request.memory_mb > 0 {
            requests.push(ResourceAllocationRequest {
                request_id: format!(
                    "mem-{}-{}",
                    legacy_request.tenant_id,
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                tenant_id: legacy_request.tenant_id.clone(),
                resource_type: ResourceType::Memory,
                amount: legacy_request.memory_mb,
                priority: 100, // Default priority
                is_temporary: false,
                expires_at: None,
                metadata: [
                    (
                        "converted_from".to_string(),
                        "legacy_resource_request".to_string(),
                    ),
                    (
                        "original_vcpus".to_string(),
                        legacy_request.vcpus.to_string(),
                    ),
                    (
                        "original_disk_gb".to_string(),
                        legacy_request.disk_gb.to_string(),
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            });
        }

        // Create CPU allocation request
        if legacy_request.vcpus > 0 {
            requests.push(ResourceAllocationRequest {
                request_id: format!(
                    "cpu-{}-{}",
                    legacy_request.tenant_id,
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                tenant_id: legacy_request.tenant_id.clone(),
                resource_type: ResourceType::Cpu,
                amount: legacy_request.vcpus as u64,
                priority: 100, // Default priority
                is_temporary: false,
                expires_at: None,
                metadata: [
                    (
                        "converted_from".to_string(),
                        "legacy_resource_request".to_string(),
                    ),
                    (
                        "original_memory_mb".to_string(),
                        legacy_request.memory_mb.to_string(),
                    ),
                    (
                        "original_disk_gb".to_string(),
                        legacy_request.disk_gb.to_string(),
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            });
        }

        requests
    }

    /// Convert legacy quota violation to resource manager error
    pub fn convert_quota_violation(&self, violation: &QuotaViolation) -> BlixardError {
        match violation {
            QuotaViolation::VmLimitExceeded {
                limit,
                current,
                requested,
            } => BlixardError::QuotaExceeded {
                resource: "VMs".to_string(),
                limit: *limit as u64,
                requested: (*current + *requested) as u64,
            },
            QuotaViolation::CpuLimitExceeded {
                limit,
                current,
                requested,
            } => BlixardError::InsufficientResources {
                requested: format!("{} vCPUs", requested),
                available: format!(
                    "{} vCPUs (limit: {}, current: {})",
                    limit.saturating_sub(*current),
                    limit,
                    current
                ),
            },
            QuotaViolation::MemoryLimitExceeded {
                limit,
                current,
                requested,
            } => BlixardError::InsufficientResources {
                requested: format!("{}MB memory", requested),
                available: format!(
                    "{}MB memory (limit: {}, current: {})",
                    limit.saturating_sub(*current),
                    limit,
                    current
                ),
            },
            QuotaViolation::DiskLimitExceeded {
                limit,
                current,
                requested,
            } => BlixardError::InsufficientResources {
                requested: format!("{}GB disk", requested),
                available: format!(
                    "{}GB disk (limit: {}, current: {})",
                    limit.saturating_sub(*current),
                    limit,
                    current
                ),
            },
            QuotaViolation::PerNodeVmLimitExceeded {
                node_id,
                limit,
                current,
            } => BlixardError::SchedulingError {
                message: format!(
                    "Node {} VM limit exceeded: {}/{} VMs",
                    node_id, current, limit
                ),
            },
            QuotaViolation::RateLimitExceeded {
                operation,
                limit,
                current,
            } => BlixardError::ResourceExhausted {
                resource: format!(
                    "Rate limit for operation '{}': {}/{} per minute",
                    operation, current, limit
                ),
            },
            QuotaViolation::StorageLimitExceeded {
                limit,
                current,
                requested,
            } => BlixardError::InsufficientResources {
                requested: format!("{}GB storage", requested),
                available: format!(
                    "{}GB storage (limit: {}, current: {})",
                    limit.saturating_sub(*current),
                    limit,
                    current
                ),
            },
        }
    }

    /// Get access to the unified composite manager
    pub fn composite_manager(&self) -> &CompositeResourceManager {
        &self.composite_manager
    }

    /// Get migration status
    pub fn migration_status(&self) -> MigrationStatus {
        MigrationStatus {
            memory_migrated: self.migration_state.memory_migrated,
            cpu_migrated: self.migration_state.cpu_migrated,
            disk_migrated: self.migration_state.disk_migrated,
            warnings_count: self.migration_state.warnings_issued.len(),
            warnings: self.migration_state.warnings_issued.clone(),
        }
    }

    /// Create compatibility wrapper for legacy quota operations
    pub async fn legacy_quota_check(
        &self,
        tenant_id: &str,
        memory_mb: u64,
        vcpus: u32,
    ) -> BlixardResult<bool> {
        // Check memory availability
        let memory_request = ResourceAllocationRequest {
            request_id: format!("check-memory-{}", tenant_id),
            tenant_id: tenant_id.to_string(),
            resource_type: ResourceType::Memory,
            amount: memory_mb,
            priority: 100,
            is_temporary: true,
            expires_at: Some(std::time::SystemTime::now() + Duration::from_secs(1)),
            metadata: [("operation".to_string(), "availability_check".to_string())]
                .iter()
                .cloned()
                .collect(),
        };

        let memory_available = self
            .composite_manager
            .check_availability(&memory_request)
            .await?;

        // Check CPU availability
        let cpu_request = ResourceAllocationRequest {
            request_id: format!("check-cpu-{}", tenant_id),
            tenant_id: tenant_id.to_string(),
            resource_type: ResourceType::Cpu,
            amount: vcpus as u64,
            priority: 100,
            is_temporary: true,
            expires_at: Some(std::time::SystemTime::now() + Duration::from_secs(1)),
            metadata: [("operation".to_string(), "availability_check".to_string())]
                .iter()
                .cloned()
                .collect(),
        };

        let cpu_available = self
            .composite_manager
            .check_availability(&cpu_request)
            .await?;

        Ok(memory_available && cpu_available)
    }

    /// Create unified resource allocation from legacy VM configuration
    pub async fn allocate_for_vm(
        &self,
        vm_name: &str,
        tenant_id: &str,
        memory_mb: u64,
        vcpus: u32,
    ) -> BlixardResult<Vec<ResourceAllocation>> {
        let mut allocations = Vec::new();

        // Allocate memory
        let memory_request = ResourceAllocationRequest {
            request_id: format!("vm-memory-{}", vm_name),
            tenant_id: tenant_id.to_string(),
            resource_type: ResourceType::Memory,
            amount: memory_mb,
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: [
                ("vm_name".to_string(), vm_name.to_string()),
                ("resource_class".to_string(), "vm_allocation".to_string()),
            ]
            .iter()
            .cloned()
            .collect(),
        };

        let memory_allocation = self.composite_manager.allocate(memory_request).await?;
        allocations.push(memory_allocation);

        // Allocate CPU
        let cpu_request = ResourceAllocationRequest {
            request_id: format!("vm-cpu-{}", vm_name),
            tenant_id: tenant_id.to_string(),
            resource_type: ResourceType::Cpu,
            amount: vcpus as u64,
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: [
                ("vm_name".to_string(), vm_name.to_string()),
                ("resource_class".to_string(), "vm_allocation".to_string()),
            ]
            .iter()
            .cloned()
            .collect(),
        };

        let cpu_allocation = self.composite_manager.allocate(cpu_request).await?;
        allocations.push(cpu_allocation);

        info!(
            "Allocated resources for VM {}: {}MB memory, {} vCPUs",
            vm_name, memory_mb, vcpus
        );
        Ok(allocations)
    }

    /// Release all resources for a VM
    pub async fn deallocate_for_vm(&self, vm_name: &str) -> BlixardResult<()> {
        let allocations = self.composite_manager.get_allocations().await?;

        for allocation in allocations {
            if let Some(allocated_vm_name) = allocation.request.metadata.get("vm_name") {
                if allocated_vm_name == vm_name {
                    self.composite_manager
                        .deallocate(&allocation.allocation_id)
                        .await?;
                }
            }
        }

        info!("Deallocated all resources for VM {}", vm_name);
        Ok(())
    }
}

/// Migration status information
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub memory_migrated: bool,
    pub cpu_migrated: bool,
    pub disk_migrated: bool,
    pub warnings_count: usize,
    pub warnings: Vec<String>,
}

impl MigrationStatus {
    /// Check if migration is complete
    pub fn is_complete(&self) -> bool {
        self.memory_migrated && self.cpu_migrated && self.disk_migrated
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        let completed = [self.memory_migrated, self.cpu_migrated, self.disk_migrated]
            .iter()
            .filter(|&&x| x)
            .count();
        (completed as f64 / 3.0) * 100.0
    }
}

/// Utility functions for code migration
/// Convert legacy tenant quota to resource manager limits
pub fn convert_tenant_quota_to_limits(quota: &TenantQuota) -> Vec<ResourceLimits> {
    let mut limits = Vec::new();

    // Memory limits
    limits.push(ResourceLimits {
        resource_type: ResourceType::Memory,
        hard_limit: quota.vm_limits.max_memory_mb,
        soft_limit: (quota.vm_limits.max_memory_mb as f64 * 0.8) as u64,
        allow_overcommit: quota.vm_limits.overcommit_ratio > 1.0,
        overcommit_ratio: quota.vm_limits.overcommit_ratio as f64,
        system_reserve: (quota.vm_limits.max_memory_mb as f64 * 0.1) as u64,
    });

    // CPU limits
    limits.push(ResourceLimits {
        resource_type: ResourceType::Cpu,
        hard_limit: quota.vm_limits.max_vcpus as u64,
        soft_limit: (quota.vm_limits.max_vcpus as f64 * 0.8) as u64,
        allow_overcommit: quota.vm_limits.overcommit_ratio > 1.0,
        overcommit_ratio: quota.vm_limits.overcommit_ratio as f64,
        system_reserve: 1, // Reserve 1 CPU for system
    });

    // Disk limits
    limits.push(ResourceLimits {
        resource_type: ResourceType::Disk,
        hard_limit: quota.vm_limits.max_disk_gb,
        soft_limit: (quota.vm_limits.max_disk_gb as f64 * 0.9) as u64,
        allow_overcommit: false, // Don't overcommit disk by default
        overcommit_ratio: 1.0,
        system_reserve: (quota.vm_limits.max_disk_gb as f64 * 0.05) as u64,
    });

    limits
}

/// Generate migration report
pub fn generate_migration_report(status: &MigrationStatus) -> String {
    let mut report = String::with_capacity(512);

    report.push_str(&format!("Resource Manager Migration Report\n"));
    report.push_str(&format!("=====================================\n\n"));

    report.push_str(&format!(
        "Migration Status: {:.1}% complete\n",
        status.completion_percentage()
    ));
    report.push_str(&format!(
        "Memory Migration: {}\n",
        if status.memory_migrated {
            "✓ Complete"
        } else {
            "✗ Pending"
        }
    ));
    report.push_str(&format!(
        "CPU Migration: {}\n",
        if status.cpu_migrated {
            "✓ Complete"
        } else {
            "✗ Pending"
        }
    ));
    report.push_str(&format!(
        "Disk Migration: {}\n",
        if status.disk_migrated {
            "✓ Complete"
        } else {
            "✗ Pending"
        }
    ));

    if !status.warnings.is_empty() {
        report.push_str(&format!("\nWarnings ({}):\n", status.warnings_count));
        for (i, warning) in status.warnings.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, warning));
        }
    }

    if status.is_complete() {
        report.push_str(&format!("\n✓ Migration completed successfully!\n"));
        report.push_str(&format!(
            "You can now use the unified ResourceManager interface.\n"
        ));
    } else {
        report.push_str(&format!(
            "\n⚠ Migration is incomplete. Continue with remaining steps.\n"
        ));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_migration_wrapper_creation() {
        let wrapper = ResourceManagerMigrationWrapper::new(8192, 4).await.unwrap();

        let status = wrapper.migration_status();
        assert!(!status.is_complete());
        assert_eq!(status.completion_percentage(), 0.0);
    }

    #[tokio::test]
    async fn test_legacy_quota_conversion() {
        let quota = TenantQuota {
            tenant_id: "test-tenant".to_string(),
            vm_limits: crate::types::VmConfig {
                max_memory_mb: 8192,
                max_vcpus: 4,
                max_disk_gb: 100,
                overcommit_ratio: 1.5,
                ..Default::default()
            },
            ..TenantQuota::new("test-tenant".to_string())
        };

        let limits = convert_tenant_quota_to_limits(&quota);
        assert_eq!(limits.len(), 3);

        let memory_limits = limits
            .iter()
            .find(|l| l.resource_type == ResourceType::Memory)
            .unwrap();
        assert_eq!(memory_limits.hard_limit, 8192);
        assert_eq!(memory_limits.overcommit_ratio, 1.5);
    }

    #[tokio::test]
    async fn test_resource_request_conversion() {
        let wrapper = ResourceManagerMigrationWrapper::new(8192, 4).await.unwrap();

        let legacy_request = ResourceRequest {
            tenant_id: "test-tenant".to_string(),
            node_id: Some(1),
            vcpus: 2,
            memory_mb: 4096,
            disk_gb: 50,
            timestamp: std::time::SystemTime::now(),
        };

        let new_requests = wrapper.convert_resource_request(&legacy_request);
        assert_eq!(new_requests.len(), 2); // Memory + CPU

        let memory_request = new_requests
            .iter()
            .find(|r| r.resource_type == ResourceType::Memory)
            .unwrap();
        assert_eq!(memory_request.amount, 4096);

        let cpu_request = new_requests
            .iter()
            .find(|r| r.resource_type == ResourceType::Cpu)
            .unwrap();
        assert_eq!(cpu_request.amount, 2);
    }

    #[tokio::test]
    async fn test_vm_resource_allocation() {
        let wrapper = ResourceManagerMigrationWrapper::new(8192, 8).await.unwrap();

        // Initialize the composite manager
        wrapper.composite_manager.initialize().await.unwrap();
        wrapper.composite_manager.start().await.unwrap();

        let allocations = wrapper
            .allocate_for_vm("test-vm", "test-tenant", 2048, 2)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 2);

        // Test deallocation
        wrapper.deallocate_for_vm("test-vm").await.unwrap();
    }

    #[tokio::test]
    async fn test_migration_status() {
        let status = MigrationStatus {
            memory_migrated: true,
            cpu_migrated: true,
            disk_migrated: false,
            warnings_count: 1,
            warnings: vec!["Test warning".to_string()],
        };

        assert!(!status.is_complete());
        assert_eq!(status.completion_percentage(), 66.66666666666666);

        let report = generate_migration_report(&status);
        assert!(report.contains("66.7% complete"));
        assert!(report.contains("✓ Complete"));
        assert!(report.contains("✗ Pending"));
    }
}
