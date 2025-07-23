//! Distributed quota management tests using MadSim
//!
//! This module tests quota management in distributed scenarios including
//! network partitions, leader changes, and concurrent modifications.

use blixard_core::{
    error::BlixardResult,
    raft::proposals::{ProposalData, ResourceUsageDelta},
    resource_quotas::{ResourceRequest, TenantQuota},
    test_helpers::{TestCluster, TestNode},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::info;

/// Helper to create a test tenant quota
fn create_test_quota(tenant_id: &str, max_vms: u32) -> TenantQuota {
    TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms,
        max_vcpus: max_vms * 4,
        max_memory_mb: max_vms as u64 * 4096,
        max_disk_gb: max_vms as u64 * 100,
        max_vms_per_node: (max_vms / 3).max(1),
        priority: 100,
    }
}

#[tokio::test]
async fn test_quota_sync_across_cluster() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    // Set quota through leader
    let tenant_id = "sync-test-tenant";
    let quota = create_test_quota(tenant_id, 10);
    
    let proposal = ProposalData::SetTenantQuota { 
        quota: quota.clone() 
    };
    leader.submit_proposal(proposal).await?;
    
    // Wait for replication
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify quota is available on all nodes
    for node in cluster.nodes() {
        if let Some(quota_manager) = node.shared_state.get_quota_manager().await {
            let stored_quota = quota_manager.get_tenant_quota(tenant_id).await?;
            assert!(stored_quota.is_some(), "Node {} missing quota", node.id());
            assert_eq!(stored_quota.unwrap().max_vms, 10);
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_quota_enforcement_during_leader_change() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let initial_leader = cluster.get_leader().await?;
    
    // Set quota and use some resources
    let tenant_id = "leader-change-tenant";
    let quota = create_test_quota(tenant_id, 5);
    
    let proposal = ProposalData::SetTenantQuota { 
        quota: quota.clone() 
    };
    initial_leader.submit_proposal(proposal).await?;
    
    // Allocate 3 VMs
    let usage_proposal = ProposalData::UpdateTenantUsage {
        tenant_id: tenant_id.to_string(),
        delta: ResourceUsageDelta {
            vm_count_delta: 3,
            vcpu_delta: 12,
            memory_mb_delta: 12288,
            disk_gb_delta: 300,
        },
        operation_id: uuid::Uuid::new_v4(),
    };
    initial_leader.submit_proposal(usage_proposal).await?;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Force leader change by stopping current leader
    let initial_leader_id = initial_leader.id();
    info!("Stopping initial leader {}", initial_leader_id);
    drop(initial_leader);
    
    // Wait for new leader election
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Find new leader
    let new_leader = cluster.nodes()
        .into_iter()
        .find(|n| n.id() != initial_leader_id && n.is_leader())
        .expect("Should have new leader");
    
    info!("New leader is {}", new_leader.id());
    
    // Verify quota enforcement still works on new leader
    if let Some(quota_manager) = new_leader.shared_state.get_quota_manager().await {
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(new_leader.id()),
            vcpus: 12, // Would exceed limit (3 + 3 > 5 VMs)
            memory_mb: 12288,
            disk_gb: 300,
            timestamp: SystemTime::now(),
        };
        
        // Should fail due to quota
        assert!(quota_manager.check_resource_quota(&request).await.is_err());
        
        // But smaller request should work
        let small_request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(new_leader.id()),
            vcpus: 4, // 1 more VM
            memory_mb: 4096,
            disk_gb: 100,
            timestamp: SystemTime::now(),
        };
        
        assert!(quota_manager.check_resource_quota(&small_request).await.is_ok());
    }
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_quota_modifications() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    
    let tenant_id = "concurrent-tenant";
    
    // Set initial quota
    let initial_quota = create_test_quota(tenant_id, 20);
    let leader = cluster.get_leader().await?;
    leader.submit_proposal(ProposalData::SetTenantQuota { 
        quota: initial_quota 
    }).await?;
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Spawn concurrent updates from different nodes
    let mut handles = vec![];
    
    for (i, node) in cluster.nodes().into_iter().enumerate() {
        let tenant_id = tenant_id.to_string();
        let handle = tokio::spawn(async move {
            for j in 0..5 {
                // Each node tries to update the quota
                let new_limit = 20 + (i * 10) + j;
                let quota = create_test_quota(&tenant_id, new_limit as u32);
                
                let proposal = ProposalData::SetTenantQuota { quota };
                let _ = node.submit_proposal(proposal).await;
                
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        handles.push(handle);
    }
    
    // Wait for all updates to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify all nodes have the same final quota value
    let mut quota_values = vec![];
    for node in cluster.nodes() {
        if let Some(quota_manager) = node.shared_state.get_quota_manager().await {
            if let Some(quota) = quota_manager.get_tenant_quota(tenant_id).await? {
                quota_values.push(quota.max_vms);
            }
        }
    }
    
    // All nodes should have the same value
    assert!(!quota_values.is_empty());
    let first_value = quota_values[0];
    for value in &quota_values {
        assert_eq!(*value, first_value, "Quota values diverged across nodes");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_quota_persistence_across_restarts() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let data_dir = tempfile::tempdir()?;
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .with_data_dir(data_dir.path())
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    
    // Set quotas for multiple tenants
    let tenants = vec![
        ("tenant-a", 10),
        ("tenant-b", 20),
        ("tenant-c", 30),
    ];
    
    let leader = cluster.get_leader().await?;
    for (tenant_id, max_vms) in &tenants {
        let quota = create_test_quota(tenant_id, *max_vms);
        leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
        
        // Also add some usage
        let usage_delta = ResourceUsageDelta {
            vm_count_delta: (*max_vms / 2) as i32,
            vcpu_delta: (*max_vms * 2) as i32,
            memory_mb_delta: (*max_vms as i64 * 2048),
            disk_gb_delta: (*max_vms as i64 * 50),
        };
        
        leader.submit_proposal(ProposalData::UpdateTenantUsage {
            tenant_id: tenant_id.to_string(),
            delta: usage_delta,
            operation_id: uuid::Uuid::new_v4(),
        }).await?;
    }
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Collect quota state before restart
    let mut pre_restart_state = HashMap::new();
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        for (tenant_id, _) in &tenants {
            let quota = quota_manager.get_tenant_quota(tenant_id).await?.unwrap();
            let usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
            pre_restart_state.insert(tenant_id.to_string(), (quota, usage));
        }
    }
    
    // Restart all nodes
    drop(cluster);
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Create new cluster with same data directory
    let new_cluster = TestCluster::builder()
        .with_nodes(3)
        .with_data_dir(data_dir.path())
        .build()
        .await?;
    
    new_cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let new_leader = new_cluster.get_leader().await?;
    
    // Verify quotas and usage persisted
    if let Some(quota_manager) = new_leader.shared_state.get_quota_manager().await {
        for (tenant_id, (expected_quota, expected_usage)) in pre_restart_state {
            let actual_quota = quota_manager.get_tenant_quota(&tenant_id).await?.unwrap();
            let actual_usage = quota_manager.get_resource_usage(&tenant_id).await?.unwrap();
            
            assert_eq!(actual_quota.max_vms, expected_quota.max_vms);
            assert_eq!(actual_usage.vm_usage.active_vms, expected_usage.vm_usage.active_vms);
            assert_eq!(actual_usage.vm_usage.used_vcpus, expected_usage.vm_usage.used_vcpus);
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_quota_consistency_during_network_partition() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(5)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    // Set initial quota
    let tenant_id = "partition-tenant";
    let quota = create_test_quota(tenant_id, 50);
    leader.submit_proposal(ProposalData::SetTenantQuota { 
        quota: quota.clone() 
    }).await?;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Create network partition: [0,1] | [2,3,4]
    // Assuming nodes are indexed 0-4
    let nodes = cluster.nodes();
    let minority_nodes: Vec<Arc<TestNode>> = nodes[0..2].to_vec();
    let majority_nodes: Vec<Arc<TestNode>> = nodes[2..5].to_vec();
    
    info!("Creating network partition");
    cluster.create_partition(&minority_nodes, &majority_nodes).await?;
    
    // Try to update quota in minority partition (should fail eventually)
    let minority_update = tokio::spawn(async move {
        let new_quota = create_test_quota(tenant_id, 100);
        let proposal = ProposalData::SetTenantQuota { quota: new_quota };
        minority_nodes[0].submit_proposal(proposal).await
    });
    
    // Update quota in majority partition (should succeed)
    let new_quota = create_test_quota(tenant_id, 75);
    let proposal = ProposalData::SetTenantQuota { quota: new_quota };
    majority_nodes[0].submit_proposal(proposal).await?;
    
    // Wait a bit
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Minority update should timeout or fail
    let minority_result = tokio::time::timeout(
        Duration::from_secs(5),
        minority_update
    ).await;
    
    match minority_result {
        Ok(Ok(Ok(_))) => panic!("Minority partition should not accept updates"),
        _ => info!("Minority partition correctly rejected update"),
    }
    
    // Heal partition
    info!("Healing network partition");
    cluster.heal_partition().await?;
    
    // Wait for reconciliation
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify all nodes converge to the majority's value (75)
    for node in cluster.nodes() {
        if let Some(quota_manager) = node.shared_state.get_quota_manager().await {
            let quota = quota_manager.get_tenant_quota(tenant_id).await?.unwrap();
            assert_eq!(quota.max_vms, 75, "Node {} has wrong quota after healing", node.id());
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_quota_cleanup_task_behavior() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    // Create quota and record many API requests
    let tenant_id = "cleanup-tenant";
    let quota = create_test_quota(tenant_id, 10);
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        // Record many API requests to trigger cleanup
        for _ in 0..1000 {
            quota_manager.record_api_request(tenant_id, &blixard_core::resource_quotas::ApiOperation::StatusQuery).await;
        }
        
        // Get initial usage with history
        let initial_usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
        let initial_history_len = initial_usage.api_usage.request_timestamps.len();
        
        info!("Initial request history length: {}", initial_history_len);
        
        // Wait for cleanup task to run (it should trim old timestamps)
        tokio::time::sleep(Duration::from_secs(65)).await; // Cleanup runs every 60s
        
        // Check that old timestamps were cleaned up
        let final_usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
        let final_history_len = final_usage.api_usage.request_timestamps.len();
        
        info!("Final request history length: {}", final_history_len);
        
        // Should have fewer timestamps after cleanup
        assert!(final_history_len < initial_history_len, 
            "Cleanup task should have reduced history size");
    }
    
    Ok(())
}