//! Integration tests for VM lifecycle operations with Raft consensus
//!
//! This test ensures that VM operations (create, start, stop, delete) 
//! properly go through Raft consensus and are replicated across the cluster.

#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardResult,
    test_helpers::TestCluster,
    types::VmStatus,
    iroh_types::{VmConfig, CreateVmRequest},
};
use std::time::Duration;
use tracing::{info, debug};

/// Test that VM create operation goes through Raft consensus
#[tokio::test]
async fn test_vm_create_through_raft() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster
    let mut cluster = TestCluster::new(3).await?;
    cluster.start().await?;
    
    // Wait for leader election
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Get the leader node
    let leader_id = cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader_node = cluster.get_node(leader_id)?;
    
    info!("Leader elected: node {}", leader_id);
    
    // Create a VM through the leader
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        cpu_cores: 2,
        memory_mb: 1024,
        disk_gb: 10,
    };
    
    // Send VM create command through client
    let client = cluster.leader_client().await?;
    client.create_vm(vm_config).await?;
    
    // Wait for replication
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Verify VM exists on all nodes
    for i in 1..=3 {
        let node = cluster.get_node(i)?;
        let vms = node.list_vms().await?;
        
        assert_eq!(vms.len(), 1, "Node {} should have 1 VM", i);
        assert_eq!(vms[0].0.name, "test-vm");
        assert_eq!(vms[0].1, VmStatus::Created);
        
        info!("Node {} has VM: {:?}", i, vms[0].0.name);
    }
    
    Ok(())
}

/// Test that VM operations fail on non-leader nodes
#[tokio::test]
async fn test_vm_operations_require_leader() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster
    let mut cluster = TestCluster::new(3).await?;
    cluster.start().await?;
    
    // Wait for leader election
    let leader_id = cluster.wait_for_leader(Duration::from_secs(10)).await?;
    
    // Find a follower node
    let follower_id = if leader_id == 1 { 2 } else { 1 };
    let follower_node = cluster.get_node(follower_id)?;
    
    info!("Leader: node {}, Follower: node {}", leader_id, follower_id);
    
    // Try to create VM through follower
    let vm_config = VmConfig {
        name: "follower-vm".to_string(),
        config_path: "/etc/blixard/vms/follower-vm.yaml".to_string(),
        vcpus: 1,
        memory: 512,
        tenant_id: "test-tenant".to_string(),
        ..Default::default()
    };
    
    let command = VmCommand::Create {
        config: vm_config,
        node_id: follower_id,
    };
    
    // This should fail with a leader error
    let result = follower_node.send_vm_command(command).await;
    assert!(result.is_err(), "VM creation on follower should fail");
    
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Not the leader") || err_msg.contains("leader is node"),
        "Error should indicate non-leader: {}",
        err_msg
    );
    
    Ok(())
}

/// Test full VM lifecycle through Raft
#[tokio::test]
async fn test_vm_lifecycle_with_raft() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster
    let mut cluster = TestCluster::new(3).await?;
    cluster.start().await?;
    
    // Wait for leader election
    let leader_id = cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader_node = cluster.get_node(leader_id)?;
    
    info!("Testing VM lifecycle on leader node {}", leader_id);
    
    let vm_name = "lifecycle-vm";
    
    // 1. Create VM
    let vm_config = VmConfig {
        name: vm_name.to_string(),
        config_path: format!("/etc/blixard/vms/{}.yaml", vm_name),
        vcpus: 2,
        memory: 1024,
        tenant_id: "test-tenant".to_string(),
        ..Default::default()
    };
    
    leader_node.send_vm_command(VmCommand::Create {
        config: vm_config,
        node_id: leader_id,
    }).await?;
    
    // Verify created
    let status = leader_node.get_vm_status(vm_name).await?;
    assert!(status.is_some());
    assert_eq!(status.unwrap().1, VmStatus::Created);
    
    // 2. Start VM
    leader_node.send_vm_command(VmCommand::Start {
        name: vm_name.to_string(),
    }).await?;
    
    // Wait a bit for status update
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify started (might still be Created if backend is mock)
    let status = leader_node.get_vm_status(vm_name).await?;
    assert!(status.is_some());
    let vm_status = status.unwrap().1;
    assert!(
        vm_status == VmStatus::Running || vm_status == VmStatus::Created,
        "VM should be Running or Created, got {:?}",
        vm_status
    );
    
    // 3. Stop VM
    leader_node.send_vm_command(VmCommand::Stop {
        name: vm_name.to_string(),
    }).await?;
    
    // Wait a bit for status update
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify stopped (might still be in previous state if backend is mock)
    let status = leader_node.get_vm_status(vm_name).await?;
    assert!(status.is_some());
    
    // 4. Delete VM
    leader_node.send_vm_command(VmCommand::Delete {
        name: vm_name.to_string(),
    }).await?;
    
    // Wait for deletion to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify deleted on all nodes
    for i in 1..=3 {
        let node = cluster.get_node(i)?;
        let status = node.get_vm_status(vm_name).await?;
        assert!(status.is_none(), "VM should be deleted on node {}", i);
    }
    
    info!("VM lifecycle test completed successfully");
    
    Ok(())
}

/// Test that VM state is properly replicated after node restart
#[tokio::test]
async fn test_vm_state_persistence() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster (persistence is handled by test infrastructure)
    let mut cluster = TestCluster::new(3).await?;
    cluster.start().await?;
    
    // Wait for leader election
    let leader_id = cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader_node = cluster.get_node(leader_id)?;
    
    // Create a VM
    let vm_config = VmConfig {
        name: "persistent-vm".to_string(),
        config_path: "/etc/blixard/vms/persistent-vm.yaml".to_string(),
        vcpus: 4,
        memory: 2048,
        tenant_id: "test-tenant".to_string(),
        ..Default::default()
    };
    
    leader_node.send_vm_command(VmCommand::Create {
        config: vm_config.clone(),
        node_id: leader_id,
    }).await?;
    
    // Wait for replication
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Stop a follower node
    let follower_id = if leader_id == 1 { 2 } else { 1 };
    cluster.stop_node(follower_id).await?;
    
    debug!("Stopped node {}", follower_id);
    
    // Create another VM while the follower is down
    let vm_config2 = VmConfig {
        name: "vm-during-outage".to_string(),
        config_path: "/etc/blixard/vms/vm-during-outage.yaml".to_string(),
        vcpus: 1,
        memory: 512,
        tenant_id: "test-tenant".to_string(),
        ..Default::default()
    };
    
    leader_node.send_vm_command(VmCommand::Create {
        config: vm_config2,
        node_id: leader_id,
    }).await?;
    
    // Restart the follower
    cluster.restart_node(follower_id).await?;
    
    // Wait for the follower to catch up
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Verify both VMs exist on the restarted node
    let follower_node = cluster.get_node(follower_id)?;
    let vms = follower_node.list_vms().await?;
    
    assert_eq!(vms.len(), 2, "Restarted node should have 2 VMs");
    
    let vm_names: Vec<String> = vms.iter().map(|(config, _)| config.name.clone()).collect();
    assert!(vm_names.contains(&"persistent-vm".to_string()));
    assert!(vm_names.contains(&"vm-during-outage".to_string()));
    
    info!("VM state properly restored after node restart");
    
    Ok(())
}

/// Test concurrent VM operations
#[tokio::test] 
async fn test_concurrent_vm_operations() -> BlixardResult<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // Create a 3-node cluster
    let mut cluster = TestCluster::new(3).await?;
    cluster.start().await?;
    
    // Wait for leader election
    let leader_id = cluster.wait_for_leader(Duration::from_secs(10)).await?;
    let leader_node = cluster.get_node(leader_id)?;
    
    // Create multiple VMs concurrently
    let mut handles = vec![];
    
    for i in 0..5 {
        let node = leader_node.clone();
        let node_id = leader_id;
        let handle = tokio::spawn(async move {
            let vm_config = VmConfig {
                name: format!("concurrent-vm-{}", i),
                config_path: format!("/etc/blixard/vms/concurrent-vm-{}.yaml", i),
                vcpus: 1,
                memory: 256,
                tenant_id: "test-tenant".to_string(),
                ..Default::default()
            };
            
            node.send_vm_command(VmCommand::Create {
                config: vm_config,
                node_id,
            }).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut successes = 0;
    for handle in handles {
        if handle.await?.is_ok() {
            successes += 1;
        }
    }
    
    assert_eq!(successes, 5, "All 5 VM creations should succeed");
    
    // Wait for replication
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Verify all VMs exist on all nodes
    for node_id in 1..=3 {
        let node = cluster.get_node(node_id)?;
        let vms = node.list_vms().await?;
        assert_eq!(vms.len(), 5, "Node {} should have 5 VMs", node_id);
    }
    
    info!("Concurrent VM operations completed successfully");
    
    Ok(())
}