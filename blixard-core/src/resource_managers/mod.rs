//! Resource Manager Implementations
//!
//! This module contains concrete implementations of the ResourceManager trait
//! for different resource types, providing unified interfaces that eliminate
//! code duplication across the resource management system.

pub mod cpu_manager;
pub mod integration_example;
pub mod memory_manager;
pub mod migration_utils;

// Re-export the managers for easy access
pub use cpu_manager::{CpuResourceManager, CpuResourceManagerBuilder, CpuSchedulingPolicy};
pub use memory_manager::{MemoryResourceManager, MemoryResourceManagerBuilder};
pub use migration_utils::{
    convert_tenant_quota_to_limits, generate_migration_report, MigrationStatus,
    ResourceManagerMigrationWrapper,
};

use crate::error::{BlixardError, BlixardResult};
use crate::resource_manager::*;
use async_trait::async_trait;
use std::sync::Arc;

/// Default resource manager factory implementation
#[derive(Debug)]
pub struct DefaultResourceManagerFactory {
    /// Default memory capacity in MB
    default_memory_capacity: u64,
    /// Default number of CPU cores
    default_cpu_cores: u32,
}

impl DefaultResourceManagerFactory {
    /// Create a new factory with default capacities
    pub fn new(default_memory_capacity: u64, default_cpu_cores: u32) -> Self {
        Self {
            default_memory_capacity,
            default_cpu_cores,
        }
    }

    /// Create a factory with system-detected capacities
    pub fn with_system_defaults() -> Self {
        // In production, these would be detected from the system
        Self {
            default_memory_capacity: 16384, // 16GB default
            default_cpu_cores: 8,           // 8 cores default
        }
    }
}

#[async_trait]
impl ResourceManagerFactory for DefaultResourceManagerFactory {
    async fn create(
        &self,
        config: ResourceManagerConfig,
    ) -> BlixardResult<Box<dyn ResourceManager>> {
        match config.resource_type {
            ResourceType::Memory => {
                let capacity = if config.limits.hard_limit > 0 {
                    config.limits.hard_limit
                } else {
                    self.default_memory_capacity
                };

                let manager = MemoryResourceManager::builder()
                    .with_capacity(capacity)
                    .with_overcommit(config.limits.overcommit_ratio)
                    .with_monitoring(config.enable_monitoring, config.monitoring_interval)
                    .with_health_checks(config.enable_health_checks, config.health_check_interval)
                    .build();

                Ok(Box::new(manager))
            }
            ResourceType::Cpu => {
                let cores = if config.limits.hard_limit > 0 {
                    config.limits.hard_limit as u32
                } else {
                    self.default_cpu_cores
                };

                let manager = CpuResourceManager::builder(cores)
                    .with_overcommit(config.limits.overcommit_ratio)
                    .with_monitoring(config.enable_monitoring, config.monitoring_interval)
                    .build();

                Ok(Box::new(manager))
            }
            ResourceType::Disk => {
                // TODO: Implement DiskResourceManager
                Err(BlixardError::NotImplemented {
                    feature: "Disk resource manager".to_string(),
                })
            }
            ResourceType::Network => {
                // TODO: Implement NetworkResourceManager
                Err(BlixardError::NotImplemented {
                    feature: "Network resource manager".to_string(),
                })
            }
            ResourceType::Gpu => {
                // TODO: Implement GpuResourceManager
                Err(BlixardError::NotImplemented {
                    feature: "GPU resource manager".to_string(),
                })
            }
            ResourceType::Custom(ref name) => Err(BlixardError::NotImplemented {
                feature: format!("Custom resource manager: {}", name),
            }),
        }
    }

    fn supported_types(&self) -> Vec<ResourceType> {
        vec![
            ResourceType::Memory,
            ResourceType::Cpu,
            // TODO: Add when implemented
            // ResourceType::Disk,
            // ResourceType::Network,
            // ResourceType::Gpu,
        ]
    }

    async fn validate_config(
        &self,
        resource_type: &ResourceType,
        config: &ResourceManagerConfig,
    ) -> BlixardResult<()> {
        // Basic validation
        if &config.resource_type != resource_type {
            return Err(BlixardError::InvalidInput {
                field: "resource_type".to_string(),
                message: format!(
                    "Config resource type {:?} doesn't match requested type {:?}",
                    config.resource_type, resource_type
                ),
            });
        }

        match resource_type {
            ResourceType::Memory => {
                if config.limits.hard_limit == 0 {
                    return Err(BlixardError::InvalidInput {
                        field: "hard_limit".to_string(),
                        message: "Memory hard limit must be greater than 0".to_string(),
                    });
                }
            }
            ResourceType::Cpu => {
                if config.limits.hard_limit == 0 {
                    return Err(BlixardError::InvalidInput {
                        field: "hard_limit".to_string(),
                        message: "CPU core count must be greater than 0".to_string(),
                    });
                }
            }
            _ => {
                return Err(BlixardError::NotImplemented {
                    feature: format!("Validation for resource type: {:?}", resource_type),
                });
            }
        }

        // Common validation
        if config.limits.soft_limit > config.limits.hard_limit {
            return Err(BlixardError::InvalidInput {
                field: "soft_limit".to_string(),
                message: "Soft limit cannot exceed hard limit".to_string(),
            });
        }

        if config.limits.overcommit_ratio < 1.0 {
            return Err(BlixardError::InvalidInput {
                field: "overcommit_ratio".to_string(),
                message: "Overcommit ratio must be at least 1.0".to_string(),
            });
        }

        Ok(())
    }
}

/// Helper function to create a composite resource manager with common resource types
pub async fn create_standard_composite_manager(
    memory_capacity: u64,
    cpu_cores: u32,
) -> BlixardResult<CompositeResourceManager> {
    let composite = CompositeResourceManager::new();
    let factory = Arc::new(DefaultResourceManagerFactory::new(
        memory_capacity,
        cpu_cores,
    ));

    // Create memory manager
    let memory_config = ResourceManagerConfigBuilder::new(ResourceType::Memory)
        .with_hard_limit(memory_capacity)
        .with_overcommit(1.2) // 20% overcommit
        .with_monitoring_interval(std::time::Duration::from_secs(30))
        .build();

    let memory_manager = factory.create(memory_config).await?;
    composite
        .add_manager(ResourceType::Memory, memory_manager)
        .await?;

    // Create CPU manager
    let cpu_config = ResourceManagerConfigBuilder::new(ResourceType::Cpu)
        .with_hard_limit(cpu_cores as u64)
        .with_overcommit(2.0) // 2x CPU overcommit
        .with_monitoring_interval(std::time::Duration::from_secs(30))
        .build();

    let cpu_manager = factory.create(cpu_config).await?;
    composite
        .add_manager(ResourceType::Cpu, cpu_manager)
        .await?;

    Ok(composite)
}

/// Helper function to create a memory-only resource manager
pub async fn create_memory_manager(capacity_mb: u64) -> BlixardResult<MemoryResourceManager> {
    let manager = MemoryResourceManager::builder()
        .with_capacity(capacity_mb)
        .with_overcommit(1.2)
        .with_monitoring(true, std::time::Duration::from_secs(30))
        .with_health_checks(true, std::time::Duration::from_secs(60))
        .build();

    manager.initialize().await?;
    Ok(manager)
}

/// Helper function to create a CPU-only resource manager
pub async fn create_cpu_manager(cores: u32) -> BlixardResult<CpuResourceManager> {
    let manager = CpuResourceManager::builder(cores)
        .with_overcommit(2.0)
        .with_scheduling_policy(CpuSchedulingPolicy::FirstFit)
        .with_monitoring(true, std::time::Duration::from_secs(30))
        .build();

    manager.initialize().await?;
    Ok(manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_default_factory_memory_creation() {
        let factory = DefaultResourceManagerFactory::new(8192, 4);

        let config = ResourceManagerConfigBuilder::new(ResourceType::Memory)
            .with_hard_limit(4096)
            .build();

        let manager = factory.create(config).await.unwrap();
        assert_eq!(manager.resource_type(), ResourceType::Memory);
    }

    #[tokio::test]
    async fn test_default_factory_cpu_creation() {
        let factory = DefaultResourceManagerFactory::new(8192, 4);

        let config = ResourceManagerConfigBuilder::new(ResourceType::Cpu)
            .with_hard_limit(8)
            .build();

        let manager = factory.create(config).await.unwrap();
        assert_eq!(manager.resource_type(), ResourceType::Cpu);
    }

    #[tokio::test]
    async fn test_factory_unsupported_resource() {
        let factory = DefaultResourceManagerFactory::new(8192, 4);

        let config = ResourceManagerConfigBuilder::new(ResourceType::Gpu).build();

        let result = factory.create(config).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlixardError::NotImplemented { .. }
        ));
    }

    #[tokio::test]
    async fn test_factory_config_validation() {
        let factory = DefaultResourceManagerFactory::new(8192, 4);

        // Test invalid memory config
        let invalid_config = ResourceManagerConfig {
            resource_type: ResourceType::Memory,
            limits: ResourceLimits {
                resource_type: ResourceType::Memory,
                hard_limit: 0, // Invalid
                ..Default::default()
            },
            ..Default::default()
        };

        let result = factory
            .validate_config(&ResourceType::Memory, &invalid_config)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_standard_composite_manager_creation() {
        let composite = create_standard_composite_manager(8192, 4).await.unwrap();

        let resource_types = composite.get_resource_types().await;
        assert_eq!(resource_types.len(), 2);
        assert!(resource_types.contains(&ResourceType::Memory));
        assert!(resource_types.contains(&ResourceType::Cpu));
    }

    #[tokio::test]
    async fn test_helper_memory_manager_creation() {
        let manager = create_memory_manager(4096).await.unwrap();
        assert_eq!(manager.resource_type(), ResourceType::Memory);

        let usage = manager.get_usage().await.unwrap();
        assert_eq!(usage.total_capacity, 4096);
    }

    #[tokio::test]
    async fn test_helper_cpu_manager_creation() {
        let manager = create_cpu_manager(8).await.unwrap();
        assert_eq!(manager.resource_type(), ResourceType::Cpu);

        let usage = manager.get_usage().await.unwrap();
        assert_eq!(usage.total_capacity, 8);
    }

    #[tokio::test]
    async fn test_factory_supported_types() {
        let factory = DefaultResourceManagerFactory::with_system_defaults();
        let supported = factory.supported_types();

        assert!(supported.contains(&ResourceType::Memory));
        assert!(supported.contains(&ResourceType::Cpu));
        assert!(!supported.contains(&ResourceType::Gpu)); // Not yet implemented
    }
}
