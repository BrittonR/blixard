//! Performance and stress tests for quota management
//!
//! This module tests the quota system under load to ensure it performs
//! well with many tenants, high request rates, and concurrent operations.

#![cfg(feature = "test-helpers")]

use blixard_core::{
    quota_manager::QuotaManager,
    raft_storage::RedbRaftStorage,
    resource_quotas::{ApiOperation, ResourceRequest, TenantQuota},
};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tempfile::TempDir;
use tokio::sync::Semaphore;

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

#[tokio::test]
async fn test_many_tenants_performance() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    const NUM_TENANTS: usize = 1000;
    
    // Measure time to create many tenant quotas
    let start = Instant::now();
    
    for i in 0..NUM_TENANTS {
        let quota = TenantQuota {
            tenant_id: format!("tenant-{}", i),
            max_vms: 100,
            max_vcpus: 400,
            max_memory_mb: 409600,
            max_disk_gb: 4000,
            max_vms_per_node: 50,
            priority: (i % 256) as u8,
        };
        
        quota_manager.set_tenant_quota(quota).await.unwrap();
    }
    
    let create_duration = start.elapsed();
    println!("Created {} tenants in {:?}", NUM_TENANTS, create_duration);
    
    // Verify reasonable performance (should be < 10ms per tenant)
    let per_tenant_ms = create_duration.as_millis() as f64 / NUM_TENANTS as f64;
    assert!(per_tenant_ms < 10.0, "Quota creation too slow: {}ms per tenant", per_tenant_ms);
    
    // Measure time to list all quotas
    let start = Instant::now();
    let all_quotas = quota_manager.list_tenant_quotas().await.unwrap();
    let list_duration = start.elapsed();
    
    assert_eq!(all_quotas.len(), NUM_TENANTS);
    println!("Listed {} tenants in {:?}", NUM_TENANTS, list_duration);
    
    // Should list quickly (< 1 second for 1000 tenants)
    assert!(list_duration < Duration::from_secs(1));
    
    // Measure random access performance
    let start = Instant::now();
    for i in (0..100).step_by(10) {
        let tenant_id = format!("tenant-{}", i * 10);
        let _ = quota_manager.get_tenant_quota(&tenant_id).await.unwrap();
    }
    let access_duration = start.elapsed();
    
    println!("Random access of 10 quotas took {:?}", access_duration);
    assert!(access_duration < Duration::from_millis(100));
}

#[tokio::test]
async fn test_high_frequency_quota_checks() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    // Set up a tenant with reasonable quota
    let tenant_id = "perf-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 1000,
        max_vcpus: 4000,
        max_memory_mb: 4096000,
        max_disk_gb: 40000,
        max_vms_per_node: 100,
        priority: 100,
    };
    
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    const NUM_CHECKS: usize = 10000;
    
    // Measure time for many quota checks
    let start = Instant::now();
    
    for i in 0..NUM_CHECKS {
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some((i % 10 + 1) as u64),
            vcpus: 2,
            memory_mb: 2048,
            disk_gb: 100,
            timestamp: SystemTime::now(),
        };
        
        quota_manager.check_resource_quota(&request).await.unwrap();
    }
    
    let check_duration = start.elapsed();
    let checks_per_second = NUM_CHECKS as f64 / check_duration.as_secs_f64();
    
    println!("Performed {} quota checks in {:?}", NUM_CHECKS, check_duration);
    println!("Rate: {:.0} checks/second", checks_per_second);
    
    // Should handle at least 1000 checks per second
    assert!(checks_per_second > 1000.0, "Quota checks too slow: {:.0}/s", checks_per_second);
}

#[tokio::test]
async fn test_concurrent_quota_operations() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    const NUM_TENANTS: usize = 100;
    const CONCURRENT_OPS: usize = 50;
    
    // Create initial quotas
    for i in 0..NUM_TENANTS {
        let quota = TenantQuota {
            tenant_id: format!("concurrent-tenant-{}", i),
            max_vms: 100,
            max_vcpus: 400,
            max_memory_mb: 409600,
            max_disk_gb: 4000,
            max_vms_per_node: 50,
            priority: 100,
        };
        quota_manager.set_tenant_quota(quota).await.unwrap();
    }
    
    // Limit concurrency to avoid overwhelming the system
    let semaphore = Arc::new(Semaphore::new(CONCURRENT_OPS));
    let quota_manager = Arc::clone(&quota_manager);
    
    let start = Instant::now();
    let mut handles = vec![];
    
    // Spawn many concurrent operations
    for i in 0..1000 {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let qm = Arc::clone(&quota_manager);
        
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let tenant_id = format!("concurrent-tenant-{}", i % NUM_TENANTS);
            
            match i % 4 {
                0 => {
                    // Update quota
                    let quota = TenantQuota {
                        tenant_id: tenant_id.clone(),
                        max_vms: 100 + (i % 50) as u32,
                        max_vcpus: 400,
                        max_memory_mb: 409600,
                        max_disk_gb: 4000,
                        max_vms_per_node: 50,
                        priority: 100,
                    };
                    qm.set_tenant_quota(quota).await.unwrap();
                }
                1 => {
                    // Check quota
                    let request = ResourceRequest {
                        tenant_id: tenant_id.clone(),
                        node_id: Some(1),
                        vcpus: 4,
                        memory_mb: 4096,
                        disk_gb: 100,
                        timestamp: SystemTime::now(),
                    };
                    let _ = qm.check_resource_quota(&request).await;
                }
                2 => {
                    // Update usage
                    let _ = qm.update_resource_usage(&tenant_id, 1, 1024, 10, 1).await;
                }
                3 => {
                    // Record API request
                    qm.record_api_request(&tenant_id, &ApiOperation::StatusQuery).await;
                }
                _ => unreachable!(),
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    let duration = start.elapsed();
    println!("Completed 1000 concurrent operations in {:?}", duration);
    
    // Should complete within reasonable time (< 10 seconds)
    assert!(duration < Duration::from_secs(10));
}

#[tokio::test]
async fn test_rate_limiting_performance() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    let tenant_id = "rate-limit-perf";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 100,
        max_vcpus: 400,
        max_memory_mb: 409600,
        max_disk_gb: 4000,
        max_vms_per_node: 50,
        priority: 100,
    };
    
    quota_manager.set_tenant_quota(quota).await.unwrap();
    
    // Simulate burst of API requests
    let start = Instant::now();
    let mut limited_count = 0;
    
    for _ in 0..1000 {
        match quota_manager.check_api_rate_limit(tenant_id, &ApiOperation::VmCreate).await {
            Ok(()) => {
                quota_manager.record_api_request(tenant_id, &ApiOperation::VmCreate).await;
            }
            Err(_) => {
                limited_count += 1;
            }
        }
        
        // Small delay to spread requests
        tokio::time::sleep(Duration::from_micros(100)).await;
    }
    
    let duration = start.elapsed();
    let rate = 1000.0 / duration.as_secs_f64();
    
    println!("Processed 1000 rate limit checks in {:?}", duration);
    println!("Rate: {:.0} checks/second", rate);
    println!("Limited {} requests", limited_count);
    
    // Should handle rate limiting efficiently
    assert!(rate > 1000.0, "Rate limiting too slow: {:.0}/s", rate);
}

#[tokio::test]
async fn test_memory_usage_with_many_tenants() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    // Get initial memory usage
    let initial_memory = get_current_memory_usage();
    
    const NUM_TENANTS: usize = 5000;
    
    // Create many tenants
    for i in 0..NUM_TENANTS {
        let quota = TenantQuota {
            tenant_id: format!("memory-test-tenant-{}", i),
            max_vms: 100,
            max_vcpus: 400,
            max_memory_mb: 409600,
            max_disk_gb: 4000,
            max_vms_per_node: 50,
            priority: (i % 256) as u8,
        };
        
        quota_manager.set_tenant_quota(quota).await.unwrap();
        
        // Add some usage data
        quota_manager.update_resource_usage(
            &format!("memory-test-tenant-{}", i),
            10,
            10240,
            100,
            1
        ).await.unwrap();
        
        // Record some API requests
        for _ in 0..10 {
            quota_manager.record_api_request(
                &format!("memory-test-tenant-{}", i),
                &ApiOperation::StatusQuery
            ).await;
        }
    }
    
    // Force cleanup to run
    quota_manager.force_cleanup().await;
    
    // Get final memory usage
    let final_memory = get_current_memory_usage();
    let memory_increase = final_memory.saturating_sub(initial_memory);
    let memory_per_tenant = memory_increase / NUM_TENANTS;
    
    println!("Memory usage increased by {} KB for {} tenants", memory_increase / 1024, NUM_TENANTS);
    println!("Average memory per tenant: {} bytes", memory_per_tenant);
    
    // Each tenant should use less than 10KB on average
    assert!(memory_per_tenant < 10240, "Memory usage too high: {} bytes per tenant", memory_per_tenant);
}

#[tokio::test]
async fn test_cleanup_task_performance() {
    let (quota_manager, _temp_dir) = create_test_quota_manager().await;
    
    const NUM_TENANTS: usize = 100;
    
    // Create tenants with lots of API request history
    for i in 0..NUM_TENANTS {
        let tenant_id = format!("cleanup-perf-tenant-{}", i);
        let quota = TenantQuota {
            tenant_id: tenant_id.clone(),
            max_vms: 100,
            max_vcpus: 400,
            max_memory_mb: 409600,
            max_disk_gb: 4000,
            max_vms_per_node: 50,
            priority: 100,
        };
        
        quota_manager.set_tenant_quota(quota).await.unwrap();
        
        // Record many API requests to build up history
        for _ in 0..1000 {
            quota_manager.record_api_request(&tenant_id, &ApiOperation::StatusQuery).await;
        }
    }
    
    // Measure cleanup performance
    let start = Instant::now();
    quota_manager.force_cleanup().await;
    let cleanup_duration = start.elapsed();
    
    println!("Cleanup of {} tenants with 100K total requests took {:?}", NUM_TENANTS, cleanup_duration);
    
    // Cleanup should be fast (< 1 second)
    assert!(cleanup_duration < Duration::from_secs(1), "Cleanup too slow: {:?}", cleanup_duration);
}

/// Helper to get current process memory usage in bytes
fn get_current_memory_usage() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    
    // Fallback: return 0 if we can't determine memory usage
    0
}