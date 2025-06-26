//! Comprehensive tests for resource quota enforcement
//!
//! This test suite verifies:
//! - Quota validation during VM creation
//! - API rate limiting
//! - Resource usage tracking
//! - Quota persistence and recovery

use blixard_core::{
    quota_manager::QuotaManager,
    resource_quotas::*,
    storage::{Storage, RedbRaftStorage},
    error::BlixardError,
};
use std::sync::Arc;
use tokio::time::Duration;
use redb::Database;
use tempfile::TempDir;

/// Helper function to create a test database
async fn create_test_database() -> (Arc<Database>, TempDir) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test.db");
    
    let database = Arc::new(Database::create(&db_path).expect("Failed to create database"));
    blixard_core::storage::init_database_tables(&database).expect("Failed to init tables");
    
    (database, temp_dir)
}

/// Create a test quota manager with storage
async fn create_test_quota_manager() -> (QuotaManager, TempDir) {
    let (database, temp_dir) = create_test_database().await;
    let storage = Arc::new(RedbRaftStorage { database });
    let quota_manager = QuotaManager::new(storage).await.expect("Failed to create quota manager");
    
    (quota_manager, temp_dir)
}

#[tokio::test]
async fn test_basic_quota_creation_and_retrieval() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    // Create a custom quota
    let tenant_id = "test-tenant".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.vm_limits.max_vms = 10;
    quota.vm_limits.max_vcpus = 50;
    quota.vm_limits.max_memory_mb = 102400; // 100GB
    
    // Set the quota
    quota_manager.set_tenant_quota(quota.clone()).await.expect("Failed to set quota");
    
    // Retrieve the quota
    let retrieved_quota = quota_manager.get_tenant_quota(&tenant_id).await;
    
    assert_eq!(retrieved_quota.tenant_id, tenant_id);
    assert_eq!(retrieved_quota.vm_limits.max_vms, 10);
    assert_eq!(retrieved_quota.vm_limits.max_vcpus, 50);
    assert_eq!(retrieved_quota.vm_limits.max_memory_mb, 102400);
}

#[tokio::test]
async fn test_resource_quota_enforcement() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "resource-test".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.vm_limits.max_vms = 2;
    quota.vm_limits.max_vcpus = 4;
    quota.vm_limits.max_memory_mb = 4096; // 4GB
    quota.vm_limits.max_disk_gb = 20;
    
    quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    
    // First VM should be allowed
    let resource_request1 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 2,
        memory_mb: 2048,
        disk_gb: 10,
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&resource_request1).await.is_ok());
    
    // Update usage after first VM
    quota_manager.update_resource_usage(&tenant_id, 2, 2048, 10, 1).await.expect("Failed to update usage");
    
    // Second VM that would exceed limits should be rejected
    let resource_request2 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 3, // Would exceed vCPU limit (2 + 3 = 5 > 4)
        memory_mb: 2048,
        disk_gb: 10,
        timestamp: std::time::SystemTime::now(),
    };
    
    match quota_manager.check_resource_quota(&resource_request2).await {
        Err(QuotaViolation::CpuLimitExceeded { limit, current, requested }) => {
            assert_eq!(limit, 4);
            assert_eq!(current, 2);
            assert_eq!(requested, 3);
        }
        _ => panic!("Expected CPU limit exceeded error"),
    }
    
    // VM within limits should still be allowed
    let resource_request3 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 2, // Total would be 4, which is at the limit
        memory_mb: 2048, // Total would be 4096, which is at the limit
        disk_gb: 10, // Total would be 20, which is at the limit
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&resource_request3).await.is_ok());
}

#[tokio::test]
async fn test_per_node_vm_limits() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "node-limit-test".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.vm_limits.max_vms = 10; // High global limit
    quota.vm_limits.max_vms_per_node = 2; // Low per-node limit
    quota.vm_limits.max_vcpus = 100;
    quota.vm_limits.max_memory_mb = 100000;
    
    quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    
    // First VM on node 1
    let request1 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&request1).await.is_ok());
    quota_manager.update_resource_usage(&tenant_id, 1, 1024, 5, 1).await.expect("Failed to update usage");
    
    // Second VM on node 1
    let request2 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&request2).await.is_ok());
    quota_manager.update_resource_usage(&tenant_id, 1, 1024, 5, 1).await.expect("Failed to update usage");
    
    // Third VM on node 1 should be rejected (exceeds per-node limit)
    let request3 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        timestamp: std::time::SystemTime::now(),
    };
    
    match quota_manager.check_resource_quota(&request3).await {
        Err(QuotaViolation::PerNodeVmLimitExceeded { node_id, limit, current }) => {
            assert_eq!(node_id, 1);
            assert_eq!(limit, 2);
            assert_eq!(current, 2);
        }
        _ => panic!("Expected per-node VM limit exceeded error"),
    }
    
    // VM on different node should still be allowed
    let request4 = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(2),
        vcpus: 1,
        memory_mb: 1024,
        disk_gb: 5,
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&request4).await.is_ok());
}

#[tokio::test]
async fn test_api_rate_limiting() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "rate-limit-test".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.api_limits.requests_per_second = 2; // Very low limit for testing
    quota.api_limits.operation_limits.vm_create_per_minute = 3;
    
    quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    
    // First few requests should be allowed
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await.is_ok());
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await.is_ok());
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await.is_ok());
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    
    // Fourth request should be rejected (exceeds vm_create_per_minute limit)
    match quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await {
        Err(QuotaViolation::RateLimitExceeded { operation, limit, current }) => {
            assert_eq!(operation, "vm_create");
            assert_eq!(limit, 3);
            assert_eq!(current, 3);
        }
        _ => panic!("Expected rate limit exceeded error"),
    }
}

#[tokio::test]
async fn test_concurrent_request_limiting() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "concurrent-test".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.api_limits.max_concurrent_requests = 2;
    
    quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    
    // Start first request
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await.is_ok());
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    
    // Start second request
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmStart).await.is_ok());
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmStart).await;
    
    // Third request should be rejected (exceeds concurrent limit)
    match quota_manager.check_rate_limit(&tenant_id, &ApiOperation::StatusQuery).await {
        Err(QuotaViolation::RateLimitExceeded { operation, limit, current }) => {
            assert_eq!(operation, "concurrent");
            assert_eq!(limit, 2);
            assert_eq!(current, 2);
        }
        _ => panic!("Expected concurrent request limit exceeded error"),
    }
    
    // End one request
    quota_manager.record_api_request_end(&tenant_id).await;
    
    // Now third request should be allowed
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::StatusQuery).await.is_ok());
}

#[tokio::test]
async fn test_quota_persistence() {
    let (database, temp_dir) = create_test_database().await;
    
    let tenant_id = "persistence-test".to_string();
    
    // Create first quota manager and set a quota
    {
        let storage = Arc::new(RedbRaftStorage { database: database.clone() });
        let quota_manager = QuotaManager::new(storage).await.expect("Failed to create quota manager");
        
        let mut quota = TenantQuota::new(tenant_id.clone());
        quota.vm_limits.max_vms = 15;
        quota.vm_limits.max_vcpus = 60;
        
        quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    }
    
    // Create second quota manager and verify quota is loaded
    {
        let storage = Arc::new(RedbRaftStorage { database: database.clone() });
        let quota_manager = QuotaManager::new(storage).await.expect("Failed to create quota manager");
        
        let retrieved_quota = quota_manager.get_tenant_quota(&tenant_id).await;
        assert_eq!(retrieved_quota.vm_limits.max_vms, 15);
        assert_eq!(retrieved_quota.vm_limits.max_vcpus, 60);
    }
    
    // Keep temp_dir alive until end of test
    drop(temp_dir);
}

#[tokio::test]
async fn test_usage_tracking_accuracy() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "usage-test".to_string();
    
    // Initial usage should be zero
    let initial_usage = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(initial_usage.vm_usage.active_vms, 0);
    assert_eq!(initial_usage.vm_usage.used_vcpus, 0);
    assert_eq!(initial_usage.vm_usage.used_memory_mb, 0);
    
    // Add first VM
    quota_manager.update_resource_usage(&tenant_id, 2, 2048, 10, 1).await.expect("Failed to update usage");
    
    let usage1 = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(usage1.vm_usage.active_vms, 1);
    assert_eq!(usage1.vm_usage.used_vcpus, 2);
    assert_eq!(usage1.vm_usage.used_memory_mb, 2048);
    assert_eq!(usage1.vm_usage.used_disk_gb, 10);
    assert_eq!(usage1.vm_usage.vms_per_node.get(&1), Some(&1));
    
    // Add second VM on different node
    quota_manager.update_resource_usage(&tenant_id, 4, 4096, 20, 2).await.expect("Failed to update usage");
    
    let usage2 = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(usage2.vm_usage.active_vms, 2);
    assert_eq!(usage2.vm_usage.used_vcpus, 6);
    assert_eq!(usage2.vm_usage.used_memory_mb, 6144);
    assert_eq!(usage2.vm_usage.used_disk_gb, 30);
    assert_eq!(usage2.vm_usage.vms_per_node.get(&1), Some(&1));
    assert_eq!(usage2.vm_usage.vms_per_node.get(&2), Some(&1));
    
    // Remove first VM (negative values)
    quota_manager.update_resource_usage(&tenant_id, -2, -2048, -10, 1).await.expect("Failed to update usage");
    
    let usage3 = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(usage3.vm_usage.active_vms, 1);
    assert_eq!(usage3.vm_usage.used_vcpus, 4);
    assert_eq!(usage3.vm_usage.used_memory_mb, 4096);
    assert_eq!(usage3.vm_usage.used_disk_gb, 20);
    assert_eq!(usage3.vm_usage.vms_per_node.get(&1), None); // Should be removed
    assert_eq!(usage3.vm_usage.vms_per_node.get(&2), Some(&1));
}

#[tokio::test]
async fn test_quota_reset() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "reset-test".to_string();
    
    // Set up some usage
    quota_manager.update_resource_usage(&tenant_id, 4, 4096, 20, 1).await.expect("Failed to update usage");
    
    // Record some API requests
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    quota_manager.record_api_request(&tenant_id, &ApiOperation::VmCreate).await;
    
    // Verify usage exists
    let usage_before = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(usage_before.vm_usage.active_vms, 1);
    assert_eq!(usage_before.vm_usage.used_vcpus, 4);
    
    // Reset usage
    quota_manager.reset_tenant_usage(&tenant_id).await.expect("Failed to reset usage");
    
    // Verify usage is reset
    let usage_after = quota_manager.get_tenant_usage(&tenant_id).await;
    assert_eq!(usage_after.vm_usage.active_vms, 0);
    assert_eq!(usage_after.vm_usage.used_vcpus, 0);
    assert_eq!(usage_after.vm_usage.used_memory_mb, 0);
    assert_eq!(usage_after.vm_usage.vms_per_node.len(), 0);
}

#[tokio::test]
async fn test_multiple_tenants() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant1 = "tenant1".to_string();
    let tenant2 = "tenant2".to_string();
    
    // Set different quotas for each tenant
    let mut quota1 = TenantQuota::new(tenant1.clone());
    quota1.vm_limits.max_vms = 5;
    
    let mut quota2 = TenantQuota::new(tenant2.clone());
    quota2.vm_limits.max_vms = 10;
    
    quota_manager.set_tenant_quota(quota1).await.expect("Failed to set quota1");
    quota_manager.set_tenant_quota(quota2).await.expect("Failed to set quota2");
    
    // Use resources for tenant1
    quota_manager.update_resource_usage(&tenant1, 2, 2048, 10, 1).await.expect("Failed to update usage");
    
    // Use resources for tenant2  
    quota_manager.update_resource_usage(&tenant2, 4, 4096, 20, 1).await.expect("Failed to update usage");
    
    // Verify tenants have independent usage tracking
    let usage1 = quota_manager.get_tenant_usage(&tenant1).await;
    let usage2 = quota_manager.get_tenant_usage(&tenant2).await;
    
    assert_eq!(usage1.vm_usage.used_vcpus, 2);
    assert_eq!(usage2.vm_usage.used_vcpus, 4);
    
    // Verify tenants have independent quotas
    let quota1_retrieved = quota_manager.get_tenant_quota(&tenant1).await;
    let quota2_retrieved = quota_manager.get_tenant_quota(&tenant2).await;
    
    assert_eq!(quota1_retrieved.vm_limits.max_vms, 5);
    assert_eq!(quota2_retrieved.vm_limits.max_vms, 10);
}

#[tokio::test]
async fn test_disabled_quota() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "disabled-test".to_string();
    let mut quota = TenantQuota::new(tenant_id.clone());
    quota.vm_limits.max_vms = 1; // Very restrictive
    quota.enabled = false; // But disabled
    
    quota_manager.set_tenant_quota(quota).await.expect("Failed to set quota");
    
    // Even though limits are restrictive, requests should be allowed when disabled
    let resource_request = ResourceRequest {
        tenant_id: tenant_id.clone(),
        node_id: Some(1),
        vcpus: 100, // Way over any reasonable limit
        memory_mb: 100000,
        disk_gb: 1000,
        timestamp: std::time::SystemTime::now(),
    };
    
    assert!(quota_manager.check_resource_quota(&resource_request).await.is_ok());
    assert!(quota_manager.check_rate_limit(&tenant_id, &ApiOperation::VmCreate).await.is_ok());
}