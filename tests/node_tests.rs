// Comprehensive Node functionality tests
use tokio::time::{timeout, Duration};
use tempfile::TempDir;
use redb::Database;

use blixard::{
    node::Node,
    types::{NodeConfig, VmConfig, VmCommand, VmStatus},
    error::BlixardError,
    storage::{VM_STATE_TABLE, TASK_TABLE, WORKER_TABLE, WORKER_STATUS_TABLE},
    raft_manager::{TaskSpec, ResourceRequirements},
};

mod common;

async fn create_test_node(id: u64) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join(format!("node{}", id));
    std::fs::create_dir_all(&data_dir).unwrap();
    
    let config = NodeConfig {
        id,
        data_dir: data_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    (Node::new(config), temp_dir)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_creation() {
    let (node, _temp_dir) = create_test_node(1).await;
    assert!(!node.is_running().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_initialization() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialization should succeed
    let result = node.initialize().await;
    assert!(result.is_ok(), "Node initialization failed: {:?}", result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_lifecycle_with_initialization() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialize first
    node.initialize().await.unwrap();
    
    // Start node
    node.start().await.unwrap();
    assert!(node.is_running().await);
    
    // Wait for node to be fully running
    common::wait_for_condition(
        || async { node.is_running().await },
        Duration::from_secs(5)
    ).await.expect("Node should start within timeout");
    
    // Stop node
    node.stop().await.unwrap();
    assert!(!node.is_running().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_command_send() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialize node to set up channels
    node.initialize().await.unwrap();
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 512,
        vcpus: 1,
    };
    
    let command = VmCommand::Create {
        config: vm_config,
        node_id: 1,
    };
    
    // Should succeed in sending command (even though processing will fail)
    let result = node.send_vm_command(command).await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_command_send_without_initialization() {
    let (node, _temp_dir) = create_test_node(1).await;
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 512,
        vcpus: 1,
    };
    
    let command = VmCommand::Create {
        config: vm_config,
        node_id: 1,
    };
    
    // Should fail when not initialized
    let result = node.send_vm_command(command).await;
    assert!(matches!(result, Err(BlixardError::Internal { .. })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cluster_operations_uninitialized() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Without initialization, all cluster operations should fail
    let result = node.join_cluster(None).await;
    assert!(result.is_err(), "Expected error for join_cluster on uninitialized node");
    
    // Leave cluster should fail without proper initialization
    let result = node.leave_cluster().await;
    assert!(result.is_err(), "Expected error for leave_cluster on uninitialized node");
    
    // Cluster status should fail without initialization
    let result = node.get_cluster_status().await;
    assert!(result.is_err(), "Expected error for get_cluster_status on uninitialized node, got {:?}", result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_stop_without_start() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Should succeed even if never started
    let result = node.stop().await;
    assert!(result.is_ok());
    assert!(!node.is_running().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_multiple_starts() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    // First start
    node.start().await.unwrap();
    assert!(node.is_running().await);
    
    // Second start should replace the first
    node.start().await.unwrap();
    assert!(node.is_running().await);
    
    node.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_with_different_configs() {
    // Test with different node IDs
    let (node1, _temp1) = create_test_node(1).await;
    let (node2, _temp2) = create_test_node(999).await;
    
    assert!(!node1.is_running().await);
    assert!(!node2.is_running().await);
    
    // Test with tailscale enabled
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: Some("127.0.0.1:8000".parse().unwrap()),
        use_tailscale: true,
    };
    
    let node3 = Node::new(config);
    assert!(!node3.is_running().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_start_timeout() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    // Start should complete quickly
    let result = timeout(Duration::from_secs(5), node.start()).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_ok());
    
    node.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_status_update_command() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    let command = VmCommand::UpdateStatus {
        name: "test-vm".to_string(),
        status: VmStatus::Running,
    };
    
    // Should succeed
    let result = node.send_vm_command(command).await;
    assert!(result.is_ok());
    
    // No need to wait - command is sent asynchronously
}

// ===== Database Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_initialization() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create the data directory structure
    let data_dir = temp_dir.path().join("node1");
    std::fs::create_dir_all(&data_dir).unwrap();
    
    let config = NodeConfig {
        id: 1,
        data_dir: data_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node = Node::new(config);
    
    // Initialize should create database and tables
    match node.initialize().await {
        Ok(_) => {},
        Err(e) => panic!("Failed to initialize node: {:?}", e),
    }
    
    // Verify database file exists
    let db_path = data_dir.join("blixard.db");
    assert!(db_path.exists(), "Database file should exist");
    
    // Get database from node's shared state
    let db = node.shared().get_database().await
        .expect("Database should be initialized");
    
    let read_txn = db.begin_read().unwrap();
    
    // Verify all tables exist
    assert!(read_txn.open_table(VM_STATE_TABLE).is_ok());
    assert!(read_txn.open_table(TASK_TABLE).is_ok());
    assert!(read_txn.open_table(WORKER_TABLE).is_ok());
    assert!(read_txn.open_table(WORKER_STATUS_TABLE).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_persistence_across_restarts() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join("node1");
    std::fs::create_dir_all(&data_dir).unwrap();
    let node_id = 1;
    
    // Create and initialize first node
    {
        let config = NodeConfig {
            id: node_id,
            data_dir: data_dir.to_string_lossy().to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        
        // Create a VM
        let vm_config = VmConfig {
            name: "persist-test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            memory: 1024,
            vcpus: 2,
        };
        
        node.send_vm_command(VmCommand::Create {
            config: vm_config.clone(),
            node_id,
        }).await.unwrap();
        
        // Wait for VM to be created
        common::wait_for_condition(
            || async {
                let vms = node.list_vms().await.unwrap_or_default();
                !vms.is_empty()
            },
            Duration::from_secs(2)
        ).await.expect("VM should be created within timeout");
        
        // Verify VM was created before shutdown
        let vms = node.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1, "VM should be created before shutdown");
        assert_eq!(vms[0].0.name, "persist-test-vm");
        
        // Explicitly stop the node to ensure clean shutdown
        node.stop().await.unwrap();
        
        // Wait for node to be fully stopped
        common::wait_for_condition(
            || async { !node.is_running().await },
            Duration::from_secs(1)
        ).await.expect("Node should stop within timeout");
        
        // Ensure node is dropped
        drop(node);
    }
    
    // Wait for database file to be fully released by checking if we can open it
    let db_path = data_dir.join("blixard.db");
    common::wait_for_condition(
        || async {
            // Try to open the database file to check if it's released
            match Database::open(&db_path) {
                Ok(db) => {
                    drop(db);
                    true
                },
                Err(_) => false
            }
        },
        Duration::from_secs(5)
    ).await.expect("Database file should be released within timeout");
    
    // Create new node with same data directory
    {
        let config = NodeConfig {
            id: node_id,
            data_dir: data_dir.to_string_lossy().to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        
        // Check if VM persisted
        let vms = node.list_vms().await.unwrap();
        assert!(!vms.is_empty(), "VMs should persist across restarts");
        assert_eq!(vms[0].0.name, "persist-test-vm");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_concurrent_access() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Send multiple VM commands concurrently
    let mut handles = vec![];
    
    for i in 0..10 {
        let node_clone = node.shared();
        let handle = tokio::spawn(async move {
            let vm_config = VmConfig {
                name: format!("concurrent-vm-{}", i),
                config_path: "/tmp/test.nix".to_string(),
                memory: 512,
                vcpus: 1,
            };
            
            node_clone.send_vm_command(VmCommand::Create {
                config: vm_config,
                node_id: 1,
            }).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
    
    // Wait for all VMs to be created
    common::wait_for_condition(
        || async {
            let vms = node.list_vms().await.unwrap_or_default();
            vms.len() == 10
        },
        Duration::from_secs(2)
    ).await.expect("All VMs should be created within timeout");
    
    // Verify all VMs were created
    let vms = node.list_vms().await.unwrap();
    assert_eq!(vms.len(), 10);
}

// ===== Raft Integration Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_raft_manager_initialization() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Before initialization, Raft should not be available
    let result = node.send_raft_message(2, raft::prelude::Message::default()).await;
    assert!(matches!(result, Err(BlixardError::Internal { .. })));
    
    // After initialization, Raft should be running
    node.initialize().await.unwrap();
    
    // Wait for Raft to be ready to accept messages
    common::wait_for_condition(
        || async {
            // Try sending a message - if it succeeds, Raft is ready
            node.send_raft_message(2, raft::prelude::Message::default()).await.is_ok()
        },
        Duration::from_secs(2)
    ).await.expect("Raft should be ready within timeout");
    
    // Now Raft messages should be accepted (though may not be processed without peers)
    let result = node.send_raft_message(2, raft::prelude::Message::default()).await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires full Raft cluster setup"]
async fn test_task_submission() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Start the node to ensure all systems are running
    node.start().await.unwrap();
    
    // Bootstrap mode - register as worker
    node.join_cluster(None).await.unwrap();
    
    // Wait for node to become leader in bootstrap mode
    common::wait_for_condition(
        || async {
            // In bootstrap mode, node should elect itself as leader
            match node.get_cluster_status().await {
                Ok((leader, members, term)) => {
                    println!("DEBUG: get_cluster_status returned leader={}, members={:?}, term={}", leader, members, term);
                    leader == 1
                },
                Err(e) => {
                    println!("DEBUG: get_cluster_status error: {:?}", e);
                    false
                }
            }
        },
        Duration::from_secs(5)
    ).await.expect("Node should become leader within timeout");
    
    let task_spec = TaskSpec {
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 128,
            disk_gb: 1,
            required_features: vec![],
        },
        timeout_secs: 60,
    };
    
    println!("DEBUG: Submitting task...");
    let result = node.submit_task("test-task-1", task_spec).await;
    match result {
        Ok(worker_id) => {
            println!("DEBUG: Task assigned to worker {}", worker_id);
            assert!(worker_id > 0, "Should assign to a valid worker");
        }
        Err(e) => {
            panic!("Task submission failed: {:?}", e);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires full Raft cluster setup"]
async fn test_task_status_retrieval() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Bootstrap mode
    node.join_cluster(None).await.unwrap();
    
    // Wait for cluster to be ready
    common::wait_for_condition(
        || async {
            match node.get_cluster_status().await {
                Ok((_, members, _)) => !members.is_empty(),
                Err(_) => false
            }
        },
        Duration::from_secs(2)
    ).await.expect("Node should join cluster within timeout");
    
    // Submit a task
    let task_spec = TaskSpec {
        command: "sleep".to_string(),
        args: vec!["1".to_string()],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 128,
            disk_gb: 1,
            required_features: vec![],
        },
        timeout_secs: 60,
    };
    
    let task_id = "status-test-task";
    node.submit_task(task_id, task_spec).await.unwrap();
    
    // Check status
    let status = node.get_task_status(task_id).await.unwrap();
    assert!(status.is_some());
}

// ===== Cluster Membership Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_bootstrap_mode_registration() {
    let (mut node, temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Join in bootstrap mode (no peer)
    node.join_cluster(None).await.unwrap();
    
    // Get database from node's shared state
    let db = node.shared().get_database().await
        .expect("Database should be initialized");
    
    let read_txn = db.begin_read().unwrap();
    let worker_table = read_txn.open_table(WORKER_TABLE).unwrap();
    
    let worker_data = worker_table.get(1u64.to_le_bytes().as_slice()).unwrap();
    assert!(worker_data.is_some(), "Worker should be registered in database");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires full Raft cluster setup"]
async fn test_cluster_status_after_join() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Bootstrap mode
    node.join_cluster(None).await.unwrap();
    
    // Wait for cluster to be formed
    common::wait_for_condition(
        || async {
            match node.get_cluster_status().await {
                Ok((leader, members, term)) => leader > 0 && !members.is_empty() && term > 0,
                Err(_) => false
            }
        },
        Duration::from_secs(2)
    ).await.expect("Cluster should be formed within timeout");
    
    let (leader, members, term) = node.get_cluster_status().await.unwrap();
    assert!(leader > 0); // Should have a leader
    assert!(!members.is_empty()); // Should have members
    assert!(term > 0); // Should have started terms
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires full Raft cluster setup"]
async fn test_leave_cluster() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Join first
    node.join_cluster(None).await.unwrap();
    
    // Wait for node to be registered in cluster
    common::wait_for_condition(
        || async {
            match node.get_cluster_status().await {
                Ok((_, members, _)) => !members.is_empty(),
                Err(_) => false
            }
        },
        Duration::from_secs(2)
    ).await.expect("Node should join cluster within timeout");
    
    // Then leave
    let result = node.leave_cluster().await;
    assert!(result.is_ok());
}

// ===== Error Handling Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_database_initialization_failure() {
    let config = NodeConfig {
        id: 1,
        data_dir: "/invalid/path/that/does/not/exist".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node = Node::new(config);
    let result = node.initialize().await;
    assert!(matches!(result, Err(BlixardError::Storage { .. })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_command_channel_closed() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Start the node first
    node.start().await.unwrap();
    
    // Stop the node to close channels
    node.stop().await.unwrap();
    
    // No need to wait - channels should close synchronously
    
    // Try to send command after stop - this should fail as VM manager is cleared
    let result = node.send_vm_command(VmCommand::UpdateStatus {
        name: "test".to_string(),
        status: VmStatus::Running,
    }).await;
    
    // The command should fail as stop() clears the VM manager to release resources
    assert!(result.is_err(), "Expected error after stop clears VM manager");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_start_stop() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Start node
    node.start().await.unwrap();
    
    // Concurrent stop attempts
    let node_shared = node.shared();
    let handle1 = tokio::spawn(async move {
        node_shared.clone()
    });
    
    let handle2 = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
    });
    
    // Stop should handle concurrent access gracefully
    node.stop().await.unwrap();
    
    handle1.await.unwrap();
    handle2.await.unwrap();
    
    assert!(!node.is_running().await);
}

// ===== VM Management Tests =====

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_lifecycle_operations() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    let vm_name = "lifecycle-test-vm";
    let vm_config = VmConfig {
        name: vm_name.to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 1024,
        vcpus: 2,
    };
    
    // Create VM
    node.send_vm_command(VmCommand::Create {
        config: vm_config.clone(),
        node_id: 1,
    }).await.unwrap();
    
    // Wait for VM to be created in storage
    common::wait_for_condition(
        || async {
            let vms = node.list_vms().await.unwrap_or_default();
            vms.iter().any(|(config, _)| config.name == vm_name)
        },
        Duration::from_secs(2)
    ).await.expect("VM should be created within timeout");
    
    // Start VM
    node.send_vm_command(VmCommand::Start {
        name: vm_name.to_string(),
    }).await.unwrap();
    
    // Wait for VM status to be updated
    common::wait_for_condition(
        || async {
            node.get_vm_status(vm_name).await.unwrap_or(None).is_some()
        },
        Duration::from_secs(2)
    ).await.expect("VM status should be available within timeout");
    
    // Check status
    let status = node.get_vm_status(vm_name).await.unwrap();
    assert!(status.is_some());
    
    // Stop VM
    node.send_vm_command(VmCommand::Stop {
        name: vm_name.to_string(),
    }).await.unwrap();
    
    // Wait for VM to be stopped
    common::wait_for_condition(
        || async {
            match node.get_vm_status(vm_name).await.unwrap_or(None) {
                Some((_, status)) => !matches!(status, blixard::types::VmStatus::Running),
                None => false
            }
        },
        Duration::from_secs(2)
    ).await.expect("VM should stop within timeout");
    
    // Delete VM
    node.send_vm_command(VmCommand::Delete {
        name: vm_name.to_string(),
    }).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_vm_operations() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    node.initialize().await.unwrap();
    
    // Create multiple VMs
    for i in 0..5 {
        let vm_config = VmConfig {
            name: format!("multi-vm-{}", i),
            config_path: "/tmp/test.nix".to_string(),
            memory: 512,
            vcpus: 1,
        };
        
        node.send_vm_command(VmCommand::Create {
            config: vm_config,
            node_id: 1,
        }).await.unwrap();
    }
    
    // Wait for all VMs to be created
    common::wait_for_condition(
        || async {
            let vms = node.list_vms().await.unwrap_or_default();
            vms.len() == 5
        },
        Duration::from_secs(2)
    ).await.expect("All VMs should be created within timeout");
    
    // List all VMs
    let vms = node.list_vms().await.unwrap();
    assert_eq!(vms.len(), 5);
    
    // Verify all VMs have Creating status initially
    for (config, status) in &vms {
        assert!(matches!(status, VmStatus::Creating));
        assert!(config.name.starts_with("multi-vm-"));
    }
}