//! Edge case tests for quota management system
//!
//! This module tests boundary conditions, error scenarios, and edge cases
//! that might not be covered by the basic quota enforcement tests.

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::{BlixardError, BlixardResult},
    quota_manager::QuotaManager,
    raft_storage::RedbRaftStorage,
    resource_quotas::{
        ApiOperation, QuotaViolation, ResourceRequest, TenantQuota, TenantUsage,
        VmResourceUsage,
    },
};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Create a test quota manager with temporary storage
async fn create_test_quota_manager() -> BlixardResult<(Arc<QuotaManager>, TempDir)> {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(redb::Database::create(db_path).unwrap());
    
    // Initialize tables
    blixard_core::raft_storage::init_database_tables(&database)?;
    
    let storage = Arc::new(RedbRaftStorage { database });
    let quota_manager = Arc::new(QuotaManager::new(storage).await?);
    
    Ok((quota_manager, temp_dir))
}

/// Create a minimal tenant quota
fn create_minimal_quota(tenant_id: &str) -> TenantQuota {
    TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 1,
        max_vcpus: 1,
        max_memory_mb: 1024,
        max_disk_gb: 10,
        max_vms_per_node: 1,
        priority: 100,
    }
}

#[tokio::test]
async fn test_exact_quota_boundary() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "boundary-tenant";
    
    // Set minimal quota
    let quota = create_minimal_quota(tenant_id);
    quota_manager.set_tenant_quota(quota.clone()).await.unwrap();
    
    // Allocate exactly at the limit
    let request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 10,
        timestamp: SystemTime::now(),
    };
    
    // First allocation should succeed
    quota_manager.check_resource_quota(&request).await.unwrap();
    quota_manager.update_resource_usage(tenant_id, 1, 1024, 10, 1).await.unwrap();
    
    // Second allocation should fail (even for 0 resources)
    let zero_request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 0,
        memory_mb: 0,
        disk_gb: 0,
        timestamp: SystemTime::now(),
    };
    
    match quota_manager.check_resource_quota(&zero_request).await {
        Err(QuotaViolation::VmCountExceeded { .. }) => (),
        other => panic!("Expected VmCountExceeded, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_one_unit_over_limit() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "over-limit-tenant";
    
    let quota = create_minimal_quota(tenant_id);
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Try to allocate one unit over each limit
    let test_cases = vec![
        (2, 1024, 10, "vcpus"), // One vCPU over
        (1, 1025, 10, "memory"), // One MB over
        (1, 1024, 11, "disk"),   // One GB over
    ];
    
    for (vcpus, memory_mb, disk_gb, expected_violation) in test_cases {
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(1),
            vcpus,
            memory_mb,
            disk_gb,
            timestamp: SystemTime::now(),
        };
        
        match quota_manager.check_resource_quota(&request).await {
            Err(QuotaViolation::CpuQuotaExceeded { .. }) if expected_violation == "vcpus" => (),
            Err(QuotaViolation::MemoryQuotaExceeded { .. }) if expected_violation == "memory" => (),
            Err(QuotaViolation::DiskQuotaExceeded { .. }) if expected_violation == "disk" => (),
            other => panic!("Expected {} violation, got: {:?}", expected_violation, other),
        }
    }
}

#[tokio::test]
async fn test_negative_resource_delta() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "negative-delta-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 10,
        max_vcpus: 10,
        max_memory_mb: 10240,
        max_disk_gb: 100,
        max_vms_per_node: 5,
        priority: 100,
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Allocate some resources
    quota_manager.update_resource_usage(tenant_id, 4, 4096, 40, 1).await.unwrap();
    
    // Release more than allocated (should clamp to 0)
    quota_manager.update_resource_usage(tenant_id, -8, -8192, -80, 1).await.unwrap();
    
    let usage = quota_manager.get_resource_usage(tenant_id).await.unwrap().unwrap();
    assert_eq!(usage.vm_usage.used_vcpus, 0);
    assert_eq!(usage.vm_usage.used_memory_mb, 0);
    assert_eq!(usage.vm_usage.used_disk_gb, 0);
}

#[tokio::test]
async fn test_rapid_allocation_deallocation() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "rapid-alloc-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 100,
        max_vcpus: 100,
        max_memory_mb: 102400,
        max_disk_gb: 1000,
        max_vms_per_node: 50,
        priority: 100,
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Rapidly allocate and deallocate resources
    for i in 0..50 {
        let node_id = (i % 3) + 1;
        
        // Allocate
        quota_manager.update_resource_usage(tenant_id, 2, 2048, 20, node_id).await.unwrap();
        
        // Check intermediate state
        let usage = quota_manager.get_resource_usage(tenant_id).await.unwrap().unwrap();
        assert!(usage.vm_usage.used_vcpus <= 100);
        
        // Deallocate
        quota_manager.update_resource_usage(tenant_id, -2, -2048, -20, node_id).await.unwrap();
    }
    
    // Final usage should be zero
    let final_usage = quota_manager.get_resource_usage(tenant_id).await.unwrap().unwrap();
    assert_eq!(final_usage.vm_usage.used_vcpus, 0);
    assert_eq!(final_usage.vm_usage.used_memory_mb, 0);
}

#[tokio::test]
async fn test_priority_zero_tenant() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "zero-priority-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 10,
        max_vcpus: 10,
        max_memory_mb: 10240,
        max_disk_gb: 100,
        max_vms_per_node: 5,
        priority: 0, // Lowest priority
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Priority 0 should still allow resource allocation
    let request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 2,
        memory_mb: 2048,
        disk_gb: 20,
        timestamp: SystemTime::now(),
    };
    
    quota_manager.check_resource_quota(&request).await.unwrap();
}

#[tokio::test]
async fn test_max_values_quota() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "max-values-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: u32::MAX,
        max_vcpus: u32::MAX,
        max_memory_mb: u64::MAX,
        max_disk_gb: u64::MAX,
        max_vms_per_node: u32::MAX,
        priority: 255,
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Should handle large allocations without overflow
    let request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 1000000,
        memory_mb: 1000000000,
        disk_gb: 1000000,
        timestamp: SystemTime::now(),
    };
    
    quota_manager.check_resource_quota(&request).await.unwrap();
}

#[tokio::test]
async fn test_concurrent_rate_limit_edge_case() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "rate-limit-edge-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 100,
        max_vcpus: 100,
        max_memory_mb: 102400,
        max_disk_gb: 1000,
        max_vms_per_node: 50,
        priority: 100,
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Record requests at the exact rate limit boundary
    for _ in 0..10 {
        quota_manager.record_api_request(tenant_id, &ApiOperation::VmCreate).await;
        tokio::time::sleep(Duration::from_millis(100)).await; // 10 requests per second
    }
    
    // The 11th request within a second should be rate limited
    match quota_manager.check_api_rate_limit(tenant_id, &ApiOperation::VmCreate).await {
        Err(QuotaViolation::ApiRateLimitExceeded { .. }) => (),
        other => panic!("Expected rate limit violation, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_vm_deletion_with_zero_usage() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "zero-usage-tenant";
    
    let quota = create_minimal_quota(tenant_id);
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Try to deallocate when usage is already zero
    quota_manager.update_resource_usage(tenant_id, -1, -1024, -10, 1).await.unwrap();
    
    // Usage should remain at zero (not negative)
    let usage = quota_manager.get_resource_usage(tenant_id).await.unwrap().unwrap();
    assert_eq!(usage.vm_usage.active_vms, 0);
    assert_eq!(usage.vm_usage.used_vcpus, 0);
    assert_eq!(usage.vm_usage.used_memory_mb, 0);
}

#[tokio::test]
async fn test_quota_update_with_active_usage() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "update-active-tenant";
    
    // Set initial quota and use some resources
    let initial_quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 10,
        max_vcpus: 10,
        max_memory_mb: 10240,
        max_disk_gb: 100,
        max_vms_per_node: 5,
        priority: 100,
    };
    quota_manager.set_tenant_quota(initial_quota).await.unwrap();
    quota_manager.update_resource_usage(tenant_id, 4, 4096, 40, 1).await.unwrap();
    
    // Update quota to lower values (below current usage)
    let reduced_quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 2, // Less than current 4
        max_vcpus: 2,
        max_memory_mb: 2048,
        max_disk_gb: 20,
        max_vms_per_node: 1,
        priority: 100,
    };
    quota_manager.set_tenant_quota(reduced_quota).await.unwrap();
    
    // New allocations should fail
    let request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 10,
        timestamp: SystemTime::now(),
    };
    
    match quota_manager.check_resource_quota(&request).await {
        Err(QuotaViolation::VmCountExceeded { .. }) => (),
        other => panic!("Expected VmCountExceeded, got: {:?}", other),
    }
    
    // Existing usage should remain unchanged
    let usage = quota_manager.get_resource_usage(tenant_id).await.unwrap().unwrap();
    assert_eq!(usage.vm_usage.used_vcpus, 4);
}

#[tokio::test]
async fn test_node_specific_vm_limit_distribution() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await.unwrap();
    let tenant_id = "node-limit-tenant";
    
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 10,
        max_vcpus: 20,
        max_memory_mb: 20480,
        max_disk_gb: 200,
        max_vms_per_node: 3, // Key limit
        priority: 100,
    };
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Allocate 3 VMs on node 1 (at limit)
    for _ in 0..3 {
        quota_manager.update_resource_usage(tenant_id, 1, 1024, 10, 1).await.unwrap();
    }
    
    // 4th VM on node 1 should fail
    let request = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 10,
        timestamp: SystemTime::now(),
    };
    
    match quota_manager.check_resource_quota(&request).await {
        Err(QuotaViolation::VmsPerNodeExceeded { .. }) => (),
        other => panic!("Expected VmsPerNodeExceeded, got: {:?}", other),
    }
    
    // But allocation on node 2 should succeed
    let request_node2 = ResourceRequest {
        tenant_id: tenant_id.to_string(),
        node_id: Some(2),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 10,
        timestamp: SystemTime::now(),
    };
    
    quota_manager.check_resource_quota(&request_node2).await.unwrap();
}