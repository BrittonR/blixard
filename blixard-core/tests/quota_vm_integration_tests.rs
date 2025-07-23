//! Integration tests for quota management with VM lifecycle
//!
//! This module tests how quotas interact with VM operations including
//! creation, deletion, migration, and scheduling.

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::{BlixardError, BlixardResult},
    quota_manager::QuotaManager,
    raft::proposals::{ProposalData, ResourceUsageDelta},
    raft_storage::RedbRaftStorage,
    resource_quotas::{ResourceRequest, TenantQuota},
    test_helpers::{TestCluster, create_test_vm_config},
    types::{VmCommand, VmConfig},
    vm_scheduler::PlacementStrategy,
};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Helper to create a VM config with tenant
fn create_tenant_vm_config(name: &str, tenant_id: &str, vcpus: u32, memory: u32) -> VmConfig {
    let mut config = create_test_vm_config(name);
    config.tenant_id = tenant_id.to_string();
    config.vcpus = vcpus;
    config.memory = memory;
    config
}

#[tokio::test]
async fn test_vm_creation_respects_quota() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    // Set quota for tenant
    let tenant_id = "vm-test-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 3,
        max_vcpus: 12,
        max_memory_mb: 12288, // 12GB
        max_disk_gb: 300,
        max_vms_per_node: 2,
        priority: 100,
    };
    
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Create VMs up to quota limit
    for i in 0..3 {
        let vm_config = create_tenant_vm_config(
            &format!("vm-{}", i),
            tenant_id,
            4,  // 4 vCPUs each
            4096 // 4GB each
        );
        
        let command = VmCommand::Create {
            config: vm_config,
            node_id: leader.id(),
        };
        
        leader.submit_proposal(ProposalData::CreateVm(command)).await?;
    }
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // 4th VM should fail due to quota
    let vm_config = create_tenant_vm_config("vm-4", tenant_id, 4, 4096);
    let command = VmCommand::Create {
        config: vm_config.clone(),
        node_id: leader.id(),
    };
    
    // Check quota before submission
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(leader.id()),
            vcpus: vm_config.vcpus,
            memory_mb: vm_config.memory as u64,
            disk_gb: 100, // Default disk size
            timestamp: SystemTime::now(),
        };
        
        let result = quota_manager.check_resource_quota(&request).await;
        assert!(result.is_err(), "Should exceed VM count quota");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_vm_deletion_releases_quota() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    let tenant_id = "delete-test-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 2,
        max_vcpus: 8,
        max_memory_mb: 8192,
        max_disk_gb: 200,
        max_vms_per_node: 2,
        priority: 100,
    };
    
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Create 2 VMs (at limit)
    let vm1_config = create_tenant_vm_config("vm-1", tenant_id, 4, 4096);
    let vm2_config = create_tenant_vm_config("vm-2", tenant_id, 4, 4096);
    
    leader.submit_proposal(ProposalData::CreateVm(VmCommand::Create {
        config: vm1_config.clone(),
        node_id: leader.id(),
    })).await?;
    
    leader.submit_proposal(ProposalData::CreateVm(VmCommand::Create {
        config: vm2_config.clone(),
        node_id: leader.id(),
    })).await?;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify we're at quota limit
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        let usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
        assert_eq!(usage.vm_usage.active_vms, 2);
        assert_eq!(usage.vm_usage.used_vcpus, 8);
    }
    
    // Delete one VM
    let delete_command = VmCommand::Delete {
        name: "vm-1".to_string(),
    };
    
    // Create batch proposal with deletion and usage update
    let batch_proposal = ProposalData::Batch(vec![
        ProposalData::CreateVm(delete_command),
        ProposalData::UpdateTenantUsage {
            tenant_id: tenant_id.to_string(),
            delta: ResourceUsageDelta::from_vm_delete(&vm1_config),
            operation_id: uuid::Uuid::new_v4(),
        },
    ]);
    
    leader.submit_proposal(batch_proposal).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify quota was released
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        let usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
        assert_eq!(usage.vm_usage.active_vms, 1);
        assert_eq!(usage.vm_usage.used_vcpus, 4);
        
        // Now we should be able to create another VM
        let new_vm_config = create_tenant_vm_config("vm-3", tenant_id, 4, 4096);
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(leader.id()),
            vcpus: new_vm_config.vcpus,
            memory_mb: new_vm_config.memory as u64,
            disk_gb: 100,
            timestamp: SystemTime::now(),
        };
        
        assert!(quota_manager.check_resource_quota(&request).await.is_ok());
    }
    
    Ok(())
}

#[tokio::test]
async fn test_per_node_vm_limits() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    let tenant_id = "per-node-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 6,
        max_vcpus: 24,
        max_memory_mb: 24576,
        max_disk_gb: 600,
        max_vms_per_node: 2, // Key limit
        priority: 100,
    };
    
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    let nodes = cluster.nodes();
    
    // Create 2 VMs on each node (total 6)
    for (node_idx, node) in nodes.iter().enumerate() {
        for vm_idx in 0..2 {
            let vm_config = create_tenant_vm_config(
                &format!("vm-node{}-{}", node_idx, vm_idx),
                tenant_id,
                4,
                4096
            );
            
            let command = VmCommand::Create {
                config: vm_config.clone(),
                node_id: node.id(),
            };
            
            // Update usage for tracking
            let usage_update = ProposalData::UpdateTenantUsage {
                tenant_id: tenant_id.to_string(),
                delta: ResourceUsageDelta::from_vm_create(&vm_config),
                operation_id: uuid::Uuid::new_v4(),
            };
            
            leader.submit_proposal(ProposalData::Batch(vec![
                ProposalData::CreateVm(command),
                usage_update,
            ])).await?;
        }
    }
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Try to create 3rd VM on first node - should fail
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(nodes[0].id()),
            vcpus: 4,
            memory_mb: 4096,
            disk_gb: 100,
            timestamp: SystemTime::now(),
        };
        
        match quota_manager.check_resource_quota(&request).await {
            Err(blixard_core::resource_quotas::QuotaViolation::VmsPerNodeExceeded { .. }) => (),
            other => panic!("Expected VmsPerNodeExceeded, got: {:?}", other),
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_vm_scheduling_with_quotas() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    // Set quotas for multiple tenants with different priorities
    let tenants = vec![
        ("high-priority", 150, 5),   // priority 150
        ("medium-priority", 100, 5), // priority 100
        ("low-priority", 50, 5),     // priority 50
    ];
    
    for (tenant_id, priority, max_vms) in tenants {
        let quota = TenantQuota {
            tenant_id: tenant_id.to_string(),
            max_vms,
            max_vcpus: max_vms * 4,
            max_memory_mb: max_vms as u64 * 4096,
            max_disk_gb: max_vms as u64 * 100,
            max_vms_per_node: 3,
            priority,
        };
        
        leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    }
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Test scheduling decisions with quota considerations
    let vm_config = create_tenant_vm_config("scheduled-vm", "high-priority", 4, 4096);
    
    // Use scheduler to find best placement
    let decision = leader.shared_state
        .schedule_vm_placement(&vm_config, PlacementStrategy::MostAvailable)
        .await?;
    
    // High priority tenant should get placement
    assert!(decision.selected_node_id > 0);
    
    Ok(())
}

#[tokio::test]
async fn test_quota_with_vm_migration() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    let tenant_id = "migration-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 3,
        max_vcpus: 12,
        max_memory_mb: 12288,
        max_disk_gb: 300,
        max_vms_per_node: 2,
        priority: 100,
    };
    
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    let nodes = cluster.nodes();
    let source_node = nodes[0].id();
    let target_node = nodes[1].id();
    
    // Create 2 VMs on source node
    for i in 0..2 {
        let vm_config = create_tenant_vm_config(&format!("vm-{}", i), tenant_id, 4, 4096);
        let command = VmCommand::Create {
            config: vm_config.clone(),
            node_id: source_node,
        };
        
        leader.submit_proposal(ProposalData::Batch(vec![
            ProposalData::CreateVm(command),
            ProposalData::UpdateTenantUsage {
                tenant_id: tenant_id.to_string(),
                delta: ResourceUsageDelta::from_vm_create(&vm_config),
                operation_id: uuid::Uuid::new_v4(),
            },
        ])).await?;
    }
    
    // Create 1 VM on target node
    let vm_config = create_tenant_vm_config("vm-target", tenant_id, 4, 4096);
    let command = VmCommand::Create {
        config: vm_config.clone(),
        node_id: target_node,
    };
    
    leader.submit_proposal(ProposalData::Batch(vec![
        ProposalData::CreateVm(command),
        ProposalData::UpdateTenantUsage {
            tenant_id: tenant_id.to_string(),
            delta: ResourceUsageDelta::from_vm_create(&vm_config),
            operation_id: uuid::Uuid::new_v4(),
        },
    ])).await?;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Try to migrate one VM from source to target
    // This should fail because target already has 1 VM and limit is 2 per node
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        // Check if migration would violate per-node limit
        let request = ResourceRequest {
            tenant_id: tenant_id.to_string(),
            node_id: Some(target_node),
            vcpus: 4,
            memory_mb: 4096,
            disk_gb: 100,
            timestamp: SystemTime::now(),
        };
        
        // Should succeed since target has room for one more
        assert!(quota_manager.check_resource_quota(&request).await.is_ok());
        
        // But if we try to migrate both VMs, the second should fail
        // First migration updates usage
        quota_manager.update_resource_usage(tenant_id, 0, 0, 0, target_node).await?;
        
        // Second migration check should fail
        let result = quota_manager.check_resource_quota(&request).await;
        assert!(result.is_err(), "Should exceed per-node limit after first migration");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_quota_enforcement_with_batch_operations() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await?;
    
    cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader = cluster.get_leader().await?;
    
    let tenant_id = "batch-tenant";
    let quota = TenantQuota {
        tenant_id: tenant_id.to_string(),
        max_vms: 5,
        max_vcpus: 20,
        max_memory_mb: 20480,
        max_disk_gb: 500,
        max_vms_per_node: 3,
        priority: 100,
    };
    
    leader.submit_proposal(ProposalData::SetTenantQuota { quota }).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Create a batch of VM operations
    let mut batch_ops = vec![];
    
    // Add 3 VM creations
    for i in 0..3 {
        let vm_config = create_tenant_vm_config(&format!("batch-vm-{}", i), tenant_id, 4, 4096);
        let create_cmd = VmCommand::Create {
            config: vm_config.clone(),
            node_id: leader.id(),
        };
        
        batch_ops.push(ProposalData::CreateVm(create_cmd));
        batch_ops.push(ProposalData::UpdateTenantUsage {
            tenant_id: tenant_id.to_string(),
            delta: ResourceUsageDelta::from_vm_create(&vm_config),
            operation_id: uuid::Uuid::new_v4(),
        });
    }
    
    // Submit batch
    leader.submit_proposal(ProposalData::Batch(batch_ops)).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify usage is correct
    if let Some(quota_manager) = leader.shared_state.get_quota_manager().await {
        let usage = quota_manager.get_resource_usage(tenant_id).await?.unwrap();
        assert_eq!(usage.vm_usage.active_vms, 3);
        assert_eq!(usage.vm_usage.used_vcpus, 12);
        assert_eq!(usage.vm_usage.used_memory_mb, 12288);
    }
    
    Ok(())
}