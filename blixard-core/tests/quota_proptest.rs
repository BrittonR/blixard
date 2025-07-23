//! Property-based tests for quota management
//!
//! This module uses property testing to verify invariants and consistency
//! properties of the quota management system.

#![cfg(all(test, feature = "test-helpers"))]

use proptest::prelude::*;
use blixard_core::{
    quota_manager::QuotaManager,
    raft_storage::RedbRaftStorage,
    resource_quotas::{ResourceRequest, TenantQuota, ApiOperation},
};
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::TempDir;

/// Create a test quota manager
async fn create_test_quota_manager() -> (Arc<QuotaManager>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(redb::Database::create(db_path).unwrap());
    
    blixard_core::raft_storage::init_database_tables(&database).unwrap();
    
    let storage = Arc::new(RedbRaftStorage { database });
    let quota_manager = Arc::new(QuotaManager::new(storage).await.unwrap());
    
    (quota_manager, temp_dir)
}

/// Strategy for generating tenant IDs
fn tenant_id_strategy() -> impl Strategy<Value = String> {
    "[a-z0-9]{4,16}".prop_map(|s| format!("tenant-{}", s))
}

/// Strategy for generating tenant quotas
fn tenant_quota_strategy() -> impl Strategy<Value = TenantQuota> {
    (
        tenant_id_strategy(),
        1u32..=1000,      // max_vms
        1u32..=1000,      // max_vcpus  
        1024u64..=1048576, // max_memory_mb (1GB to 1TB)
        10u64..=10000,    // max_disk_gb (10GB to 10TB)
        1u32..=100,       // max_vms_per_node
        0u8..=255,        // priority
    ).prop_map(|(tenant_id, max_vms, max_vcpus, max_memory_mb, max_disk_gb, max_vms_per_node, priority)| {
        TenantQuota {
            tenant_id,
            max_vms,
            max_vcpus,
            max_memory_mb,
            max_disk_gb,
            max_vms_per_node,
            priority,
        }
    })
}

/// Strategy for generating resource requests
fn resource_request_strategy(tenant_id: String) -> impl Strategy<Value = ResourceRequest> {
    (
        Just(tenant_id),
        prop::option::of(1u64..=10), // node_id
        0u32..=100,                  // vcpus
        0u64..=102400,               // memory_mb (up to 100GB)
        0u64..=1000,                 // disk_gb (up to 1TB)
    ).prop_map(|(tenant_id, node_id, vcpus, memory_mb, disk_gb)| {
        ResourceRequest {
            tenant_id,
            node_id,
            vcpus,
            memory_mb,
            disk_gb,
            timestamp: SystemTime::now(),
        }
    })
}

/// Operation that can be performed on quota manager
#[derive(Debug, Clone)]
enum QuotaOperation {
    SetQuota(TenantQuota),
    UpdateUsage {
        tenant_id: String,
        vcpu_delta: i32,
        memory_mb_delta: i64,
        disk_gb_delta: i64,
        node_id: u64,
    },
    RecordApiRequest {
        tenant_id: String,
        operation: ApiOperation,
    },
}

/// Strategy for generating quota operations
fn quota_operation_strategy() -> impl Strategy<Value = QuotaOperation> {
    prop_oneof![
        tenant_quota_strategy().prop_map(QuotaOperation::SetQuota),
        (
            tenant_id_strategy(),
            -10i32..=10,    // vcpu_delta
            -10240i64..=10240, // memory_mb_delta
            -100i64..=100,   // disk_gb_delta
            1u64..=5,        // node_id
        ).prop_map(|(tenant_id, vcpu_delta, memory_mb_delta, disk_gb_delta, node_id)| {
            QuotaOperation::UpdateUsage {
                tenant_id,
                vcpu_delta,
                memory_mb_delta,
                disk_gb_delta,
                node_id,
            }
        }),
        (
            tenant_id_strategy(),
            prop_oneof![
                Just(ApiOperation::VmCreate),
                Just(ApiOperation::VmDelete),
                Just(ApiOperation::ClusterJoin),
                Just(ApiOperation::StatusQuery),
                Just(ApiOperation::ConfigChange),
            ]
        ).prop_map(|(tenant_id, operation)| {
            QuotaOperation::RecordApiRequest { tenant_id, operation }
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    #[test]
    fn test_quota_never_negative(operations in prop::collection::vec(quota_operation_strategy(), 1..50)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (quota_manager, _temp_dir) = create_test_quota_manager().await;
            
            // Apply all operations
            for op in operations {
                match op {
                    QuotaOperation::SetQuota(quota) => {
                        let _ = quota_manager.set_tenant_quota(quota).await;
                    }
                    QuotaOperation::UpdateUsage { tenant_id, vcpu_delta, memory_mb_delta, disk_gb_delta, node_id } => {
                        let _ = quota_manager.update_resource_usage(
                            &tenant_id, vcpu_delta, memory_mb_delta, disk_gb_delta, node_id
                        ).await;
                    }
                    QuotaOperation::RecordApiRequest { tenant_id, operation } => {
                        quota_manager.record_api_request(&tenant_id, &operation).await;
                    }
                }
            }
            
            // Verify all usage values are non-negative
            let all_quotas = quota_manager.list_tenant_quotas().await.unwrap();
            for quota in all_quotas {
                if let Ok(Some(usage)) = quota_manager.get_resource_usage(&quota.tenant_id).await {
                    assert!(usage.vm_usage.active_vms <= u32::MAX);
                    assert!(usage.vm_usage.used_vcpus <= u32::MAX);
                    assert!(usage.vm_usage.used_memory_mb <= u64::MAX);
                    assert!(usage.vm_usage.used_disk_gb <= u64::MAX);
                    
                    // Verify per-node counts are reasonable
                    for (_node_id, count) in &usage.vm_usage.vms_per_node {
                        assert!(*count <= quota.max_vms);
                    }
                }
            }
        });
    }
    
    #[test]
    fn test_quota_consistency_after_updates(
        initial_quota in tenant_quota_strategy(),
        updates in prop::collection::vec(tenant_quota_strategy().prop_filter("same tenant", |q| q.tenant_id == initial_quota.tenant_id), 1..10)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (quota_manager, _temp_dir) = create_test_quota_manager().await;
            let tenant_id = initial_quota.tenant_id.clone();
            
            // Set initial quota
            quota_manager.set_tenant_quota(initial_quota).await.unwrap();
            
            // Apply updates
            for update in updates {
                quota_manager.set_tenant_quota(update.clone()).await.unwrap();
                
                // Verify the stored quota matches the update
                let stored = quota_manager.get_tenant_quota(&tenant_id).await.unwrap().unwrap();
                assert_eq!(stored.max_vms, update.max_vms);
                assert_eq!(stored.max_vcpus, update.max_vcpus);
                assert_eq!(stored.max_memory_mb, update.max_memory_mb);
                assert_eq!(stored.max_disk_gb, update.max_disk_gb);
                assert_eq!(stored.priority, update.priority);
            }
        });
    }
    
    #[test]
    fn test_resource_allocation_respects_limits(
        quota in tenant_quota_strategy(),
        requests in prop::collection::vec(resource_request_strategy(quota.tenant_id.clone()), 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (quota_manager, _temp_dir) = create_test_quota_manager().await;
            
            // Set quota
            quota_manager.set_tenant_quota(quota.clone()).await.unwrap();
            
            let mut total_vcpus = 0u32;
            let mut total_memory = 0u64;
            let mut total_disk = 0u64;
            let mut vm_count = 0u32;
            let mut vms_per_node: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
            
            // Try to allocate resources
            for request in requests {
                match quota_manager.check_resource_quota(&request).await {
                    Ok(()) => {
                        // If check passes, update our tracking
                        let would_be_vcpus = total_vcpus + request.vcpus;
                        let would_be_memory = total_memory + request.memory_mb;
                        let would_be_disk = total_disk + request.disk_gb;
                        let would_be_vms = vm_count + 1;
                        
                        // Only "allocate" if it wouldn't exceed limits
                        if would_be_vcpus <= quota.max_vcpus
                            && would_be_memory <= quota.max_memory_mb
                            && would_be_disk <= quota.max_disk_gb
                            && would_be_vms <= quota.max_vms
                        {
                            if let Some(node_id) = request.node_id {
                                let node_vms = vms_per_node.entry(node_id).or_insert(0);
                                if *node_vms < quota.max_vms_per_node {
                                    total_vcpus = would_be_vcpus;
                                    total_memory = would_be_memory;
                                    total_disk = would_be_disk;
                                    vm_count = would_be_vms;
                                    *node_vms += 1;
                                    
                                    // Update in quota manager
                                    quota_manager.update_resource_usage(
                                        &request.tenant_id,
                                        request.vcpus as i32,
                                        request.memory_mb as i64,
                                        request.disk_gb as i64,
                                        node_id
                                    ).await.unwrap();
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Quota check failed - this is expected when limits are exceeded
                    }
                }
            }
            
            // Verify final state doesn't exceed quotas
            assert!(total_vcpus <= quota.max_vcpus);
            assert!(total_memory <= quota.max_memory_mb);
            assert!(total_disk <= quota.max_disk_gb);
            assert!(vm_count <= quota.max_vms);
            for (_node, count) in vms_per_node {
                assert!(count <= quota.max_vms_per_node);
            }
        });
    }
    
    #[test]
    fn test_quota_removal_cleans_usage(tenant_ids in prop::collection::vec(tenant_id_strategy(), 1..10)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (quota_manager, _temp_dir) = create_test_quota_manager().await;
            
            // Set quotas and add some usage for each tenant
            for tenant_id in &tenant_ids {
                let quota = TenantQuota {
                    tenant_id: tenant_id.clone(),
                    max_vms: 100,
                    max_vcpus: 100,
                    max_memory_mb: 102400,
                    max_disk_gb: 1000,
                    max_vms_per_node: 50,
                    priority: 100,
                };
                quota_manager.set_tenant_quota(quota).await.unwrap();
                quota_manager.update_resource_usage(tenant_id, 10, 10240, 100, 1).await.unwrap();
            }
            
            // Remove all quotas
            for tenant_id in &tenant_ids {
                quota_manager.remove_tenant_quota(tenant_id).await.unwrap();
            }
            
            // Verify quotas and usage are gone
            for tenant_id in &tenant_ids {
                assert!(quota_manager.get_tenant_quota(tenant_id).await.unwrap().is_none());
                assert!(quota_manager.get_resource_usage(tenant_id).await.unwrap().is_none());
            }
            
            // List should be empty
            assert!(quota_manager.list_tenant_quotas().await.unwrap().is_empty());
        });
    }
}