//! Integration example demonstrating the unified ResourceManager interface
//!
//! This example shows how to replace existing quota management, resource monitoring,
//! and admission control with the new unified ResourceManager system.

use crate::error::BlixardResult;
use crate::resource_manager::*;
use crate::resource_managers::*;
use crate::resource_managers::migration_utils::generate_migration_report;
use crate::resource_quotas::TenantQuota;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

/// Example: Replace legacy quota management system
pub async fn replace_legacy_quota_system() -> BlixardResult<()> {
    info!("=== ResourceManager Integration Example ===");
    
    // Step 1: Create unified resource managers
    let memory_manager = create_memory_manager(16384).await?; // 16GB
    let cpu_manager = create_cpu_manager(8).await?;          // 8 cores
    
    // Step 2: Create composite manager for unified access
    let composite = CompositeResourceManager::new();
    composite.add_manager(ResourceType::Memory, Box::new(memory_manager)).await?;
    composite.add_manager(ResourceType::Cpu, Box::new(cpu_manager)).await?;
    
    // Step 3: Initialize and start all managers
    composite.initialize().await?;
    composite.start().await?;
    
    info!("✓ Unified resource managers initialized and started");
    
    // Step 4: Example resource allocation (replaces legacy quota checks)
    let memory_request = ResourceAllocationRequest {
        request_id: "web-server-memory".to_string(),
        tenant_id: "acme-corp".to_string(),
        resource_type: ResourceType::Memory,
        amount: 4096, // 4GB
        priority: 100,
        is_temporary: false,
        expires_at: None,
        metadata: [
            ("application".to_string(), "web-server".to_string()),
            ("environment".to_string(), "production".to_string()),
        ].iter().cloned().collect(),
    };
    
    let cpu_request = ResourceAllocationRequest {
        request_id: "web-server-cpu".to_string(),
        tenant_id: "acme-corp".to_string(),
        resource_type: ResourceType::Cpu,
        amount: 2, // 2 vCPUs
        priority: 100,
        is_temporary: false,
        expires_at: None,
        metadata: [
            ("application".to_string(), "web-server".to_string()),
            ("environment".to_string(), "production".to_string()),
        ].iter().cloned().collect(),
    };
    
    // Step 5: Allocate resources (replaces quota validation + allocation)
    let memory_allocation = composite.allocate(memory_request).await?;
    let cpu_allocation = composite.allocate(cpu_request).await?;
    
    info!("✓ Allocated resources: {} memory, {} CPU", 
          memory_allocation.allocation_id, 
          cpu_allocation.allocation_id);
    
    // Step 6: Check resource usage (replaces usage monitoring)
    let memory_usage = composite.get_usage().await?;
    info!("Memory usage: {:.1}% ({}/{}MB)", 
          memory_usage.utilization_percent(),
          memory_usage.used_amount,
          memory_usage.total_capacity);
    
    // Step 7: Get comprehensive status (replaces multiple status checks)
    let status = composite.get_status().await?;
    info!("Overall health: {:?}", status.health);
    
    // Step 8: Export telemetry (replaces manual metrics collection)
    let telemetry = composite.export_telemetry().await?;
    info!("Telemetry exported: {} data points", telemetry.len());
    
    // Step 9: Clean up resources
    composite.deallocate(&memory_allocation.allocation_id).await?;
    composite.deallocate(&cpu_allocation.allocation_id).await?;
    composite.stop().await?;
    
    info!("✓ Resources deallocated and managers stopped");
    info!("=== Integration example completed successfully ===");
    
    Ok(())
}

/// Example: Migrate from legacy quota system
pub async fn migrate_from_legacy_system() -> BlixardResult<()> {
    info!("=== Legacy System Migration Example ===");
    
    // Step 1: Simulate legacy quota data
    let mut legacy_quotas = HashMap::new();
    legacy_quotas.insert(
        "tenant-1".to_string(),
        TenantQuota::new("tenant-1".to_string())
    );
    legacy_quotas.insert(
        "tenant-2".to_string(),
        TenantQuota::new("tenant-2".to_string())
    );
    
    // Step 2: Create migration wrapper
    let mut migration_wrapper = ResourceManagerMigrationWrapper::new(32768, 16).await?;
    
    // Step 3: Migrate legacy quotas
    migration_wrapper.migrate_from_quota_manager(&legacy_quotas).await?;
    
    // Step 4: Check migration status
    let status = migration_wrapper.migration_status();
    info!("Migration progress: {:.1}%", status.completion_percentage());
    
    // Step 5: Generate migration report
    let report = generate_migration_report(&status);
    info!("Migration report:\n{}", report);
    
    // Step 6: Use unified interface for legacy operations
    let can_allocate = migration_wrapper.legacy_quota_check("tenant-1", 2048, 4).await?;
    info!("Legacy quota check result: {}", can_allocate);
    
    // Step 7: Allocate resources using unified interface
    if can_allocate {
        let allocations = migration_wrapper.allocate_for_vm(
            "legacy-vm-1", 
            "tenant-1", 
            2048, 
            4
        ).await?;
        info!("✓ Allocated {} resources for legacy VM", allocations.len());
        
        // Clean up
        migration_wrapper.deallocate_for_vm("legacy-vm-1").await?;
        info!("✓ Deallocated resources for legacy VM");
    }
    
    info!("=== Legacy migration example completed ===");
    Ok(())
}

/// Example: Advanced resource management patterns
pub async fn advanced_resource_patterns() -> BlixardResult<()> {
    info!("=== Advanced Resource Management Patterns ===");
    
    // Pattern 1: Factory-based manager creation
    let factory = DefaultResourceManagerFactory::with_system_defaults();
    
    let memory_config = ResourceManagerConfigBuilder::new(ResourceType::Memory)
        .with_hard_limit(8192)
        .with_overcommit(1.5) // 50% overcommit
        .with_monitoring_interval(Duration::from_secs(30))
        .with_telemetry(true)
        .build();
    
    let memory_manager = factory.create(memory_config).await?;
    info!("✓ Created memory manager using factory pattern");
    
    // Pattern 2: Resource efficiency monitoring
    memory_manager.initialize().await?;
    memory_manager.start().await?;
    
    let efficiency = memory_manager.get_efficiency_metrics().await?;
    info!("Memory efficiency: {:.1}% allocation, {:.1}% utilization",
          efficiency.allocation_efficiency * 100.0,
          efficiency.utilization_efficiency * 100.0);
    
    // Pattern 3: Temporary resource reservations
    let reservation_request = ResourceAllocationRequest {
        request_id: "temp-reservation".to_string(),
        tenant_id: "batch-job".to_string(),
        resource_type: ResourceType::Memory,
        amount: 1024,
        priority: 50, // Lower priority
        is_temporary: true,
        expires_at: Some(std::time::SystemTime::now() + Duration::from_secs(300)), // 5 minutes
        metadata: [("job_type".to_string(), "batch_processing".to_string())].iter().cloned().collect(),
    };
    
    let reservation = memory_manager.reserve(reservation_request).await?;
    info!("✓ Created temporary reservation: {}", reservation.allocation_id);
    
    // Activate the reservation when needed
    memory_manager.activate_reservation(&reservation.allocation_id).await?;
    info!("✓ Activated reservation");
    
    // Pattern 4: Health monitoring and alerts
    let health = memory_manager.health_check().await?;
    match health {
        ResourceManagerHealth::Healthy => info!("✓ Memory manager is healthy"),
        ResourceManagerHealth::Degraded(msg) => info!("⚠ Memory manager degraded: {}", msg),
        ResourceManagerHealth::Unhealthy(msg) => info!("✗ Memory manager unhealthy: {}", msg),
        ResourceManagerHealth::Stopped => info!("● Memory manager stopped"),
    }
    
    // Pattern 5: Resource limit updates
    let new_limits = ResourceLimits {
        resource_type: ResourceType::Memory,
        hard_limit: 12288, // Increase to 12GB
        soft_limit: 9830,  // 80% of hard limit
        allow_overcommit: true,
        overcommit_ratio: 1.3,
        system_reserve: 1024, // 1GB system reserve
    };
    
    memory_manager.update_limits(new_limits).await?;
    info!("✓ Updated resource limits");
    
    // Clean up
    memory_manager.deallocate(&reservation.allocation_id).await?;
    memory_manager.stop().await?;
    
    info!("=== Advanced patterns example completed ===");
    Ok(())
}

/// Example: Integration with VM scheduling
pub async fn vm_scheduling_integration() -> BlixardResult<()> {
    info!("=== VM Scheduling Integration Example ===");
    
    // Create composite manager for VM scheduling
    let composite = create_standard_composite_manager(16384, 8).await?;
    composite.initialize().await?;
    composite.start().await?;
    
    // Simulate VM placement decisions
    let vm_requirements = [
        ("web-server", 2048, 2),     // 2GB, 2 vCPUs
        ("database", 4096, 4),       // 4GB, 4 vCPUs
        ("cache", 1024, 1),          // 1GB, 1 vCPU
        ("worker", 512, 1),          // 512MB, 1 vCPU
    ];
    
    let mut vm_allocations = Vec::new();
    
    for (vm_name, memory_mb, vcpus) in vm_requirements.iter() {
        // Check if resources are available
        let memory_request = ResourceAllocationRequest {
            request_id: format!("{}-memory-check", vm_name),
            tenant_id: "production".to_string(),
            resource_type: ResourceType::Memory,
            amount: *memory_mb,
            priority: 100,
            is_temporary: true,
            expires_at: None,
            metadata: HashMap::new(),
        };
        
        let cpu_request = ResourceAllocationRequest {
            request_id: format!("{}-cpu-check", vm_name),
            tenant_id: "production".to_string(),
            resource_type: ResourceType::Cpu,
            amount: *vcpus as u64,
            priority: 100,
            is_temporary: true,
            expires_at: None,
            metadata: HashMap::new(),
        };
        
        let memory_available = composite.check_availability(&memory_request).await?;
        let cpu_available = composite.check_availability(&cpu_request).await?;
        
        if memory_available && cpu_available {
            // Allocate resources for VM
            let memory_alloc_request = ResourceAllocationRequest {
                request_id: format!("{}-memory", vm_name),
                tenant_id: "production".to_string(),
                resource_type: ResourceType::Memory,
                amount: *memory_mb,
                priority: 100,
                is_temporary: false,
                expires_at: None,
                metadata: [("vm_name".to_string(), vm_name.to_string())].iter().cloned().collect(),
            };
            
            let cpu_alloc_request = ResourceAllocationRequest {
                request_id: format!("{}-cpu", vm_name),
                tenant_id: "production".to_string(),
                resource_type: ResourceType::Cpu,
                amount: *vcpus as u64,
                priority: 100,
                is_temporary: false,
                expires_at: None,
                metadata: [("vm_name".to_string(), vm_name.to_string())].iter().cloned().collect(),
            };
            
            let memory_allocation = composite.allocate(memory_alloc_request).await?;
            let cpu_allocation = composite.allocate(cpu_alloc_request).await?;
            
            vm_allocations.push((vm_name.to_string(), memory_allocation.allocation_id, cpu_allocation.allocation_id));
            info!("✓ Scheduled VM {}: {}MB memory, {} vCPUs", vm_name, memory_mb, vcpus);
        } else {
            info!("✗ Cannot schedule VM {}: insufficient resources", vm_name);
        }
    }
    
    // Show cluster resource usage
    let usage = composite.get_usage().await?;
    info!("Cluster usage: {:.1}% utilization ({}/{})", 
          usage.utilization_percent(),
          usage.used_amount,
          usage.total_capacity);
    
    // Clean up all VM allocations
    for (vm_name, memory_id, cpu_id) in vm_allocations {
        composite.deallocate(&memory_id).await?;
        composite.deallocate(&cpu_id).await?;
        info!("✓ Deallocated resources for VM {}", vm_name);
    }
    
    composite.stop().await?;
    info!("=== VM scheduling integration example completed ===");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_replace_legacy_quota_system() {
        let result = replace_legacy_quota_system().await;
        assert!(result.is_ok(), "Legacy quota system replacement failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_migrate_from_legacy_system() {
        let result = migrate_from_legacy_system().await;
        assert!(result.is_ok(), "Legacy system migration failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_advanced_resource_patterns() {
        let result = advanced_resource_patterns().await;
        assert!(result.is_ok(), "Advanced resource patterns failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_vm_scheduling_integration() {
        let result = vm_scheduling_integration().await;
        assert!(result.is_ok(), "VM scheduling integration failed: {:?}", result);
    }
}