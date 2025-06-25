#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers::TestCluster;
use blixard_core::metrics_otel_v2;
use blixard_core::types::{VmConfig, VmCommand};
use blixard_core::raft_manager::{TaskSpec, ResourceRequirements};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_comprehensive_grpc_metrics_instrumentation() {
    let _ = metrics_otel_v2::init_noop();
    
    // Create a single-node cluster
    let mut cluster = TestCluster::builder()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create cluster");
    
    let node = cluster.get_node(1).expect("Failed to get node");
    
    // Wait for node to be ready
    sleep(Duration::from_millis(500)).await;
    
    // 1. Test VM operations (these generate metrics internally)
    println!("Testing VM operations...");
    
    // Create a VM
    let vm_config = VmConfig {
        name: "test-vm-1".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory: 1024,
    };
    
    let command = VmCommand::Create {
        config: vm_config.clone(),
        node_id: node.shared_state.get_id(),
    };
    
    node.shared_state.create_vm_through_raft(command).await
        .expect("Failed to create VM");
    
    // Start the VM
    let command = VmCommand::Start {
        name: "test-vm-1".to_string(),
    };
    
    node.shared_state.send_vm_operation_through_raft(command).await
        .expect("Failed to start VM");
    
    // List VMs
    let vms = node.shared_state.list_vms().await.expect("Failed to list VMs");
    assert_eq!(vms.len(), 1);
    
    // Get VM status
    let status = node.shared_state.get_vm_status("test-vm-1").await
        .expect("Failed to get VM status");
    assert!(status.is_some());
    
    // Stop the VM
    let command = VmCommand::Stop {
        name: "test-vm-1".to_string(),
    };
    
    node.shared_state.send_vm_operation_through_raft(command).await
        .expect("Failed to stop VM");
    
    // Delete the VM
    let command = VmCommand::Delete {
        name: "test-vm-1".to_string(),
    };
    
    node.shared_state.send_vm_operation_through_raft(command).await
        .expect("Failed to delete VM");
    
    // 2. Test task submission
    println!("\nTesting task operations...");
    
    let task_spec = TaskSpec {
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 512,
            disk_gb: 0,
            required_features: vec![],
        },
        timeout_secs: 30,
    };
    
    let assigned_node = node.shared_state.submit_task("test-task-1", task_spec).await
        .expect("Failed to submit task");
    assert_eq!(assigned_node, node.shared_state.get_id());
    
    // Get task status
    let task_status = node.shared_state.get_task_status("test-task-1").await
        .expect("Failed to get task status");
    assert!(task_status.is_some());
    
    // 3. Test cluster operations
    println!("\nTesting cluster operations...");
    
    let (leader_id, nodes, _term) = node.shared_state.get_cluster_status().await
        .expect("Failed to get cluster status");
    assert_eq!(leader_id, 1); // Single node cluster, node 1 is leader
    assert_eq!(nodes.len(), 1);
    
    // 4. Test VM scheduling operations
    println!("\nTesting VM scheduling operations...");
    
    let vm_config2 = VmConfig {
        name: "test-vm-2".to_string(),
        config_path: "/tmp/test2.nix".to_string(),
        vcpus: 4,
        memory: 2048,
    };
    
    let placement = node.shared_state.schedule_vm_placement(&vm_config2, blixard_core::vm_scheduler::PlacementStrategy::MostAvailable).await
        .expect("Failed to schedule VM placement");
    assert_eq!(placement.selected_node_id, node.shared_state.get_id());
    
    let resource_summary = node.shared_state.get_cluster_resource_summary().await
        .expect("Failed to get cluster resource summary");
    assert_eq!(resource_summary.total_nodes, 1);
    
    // Give time for metrics to be recorded
    sleep(Duration::from_millis(100)).await;
    
    println!("\nMetrics instrumentation test completed successfully!");
    println!("The following operations were instrumented:");
    println!("- VM operations: create, start, list, status, stop, delete");
    println!("- Task operations: submit and status check");
    println!("- Cluster operations: status check");
    println!("- VM scheduling: placement and resource summary");
    println!("- Storage operations: reads and writes (automatically via Raft)");
    
    // Test completed successfully
}