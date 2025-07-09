#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardResult,
    iroh_types,
    test_helpers::TestNode,
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_single_node_vm_create_start_stop_delete() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=debug,iroh=debug")
        .try_init();
    
    // Create and start a single node with mock backend for testing
    let node = TestNode::builder()
        .with_id(1)
        .with_port(9001)
        .with_vm_backend("mock".to_string())
        .build()
        .await?;
    
    // Give the node time to initialize and elect itself as leader
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Verify the node is ready and is the leader
    let raft_status = node.shared_state.get_raft_status().await?;
    println!("Raft status after initialization: is_leader={}, leader_id={:?}", 
             raft_status.is_leader, raft_status.leader_id);
    
    // Get a client to interact with the node
    let client = node.client().await?;
    
    // Create a VM configuration
    let vm_config = iroh_types::VmConfig {
        name: "test-vm-1".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 5,
        owner: "test-user".to_string(),
        metadata: HashMap::new(),
    };
    
    // Create the VM
    println!("Creating VM: {}", vm_config.name);
    let create_response = client.create_vm(vm_config).await?;
    assert!(create_response.into_inner().success, "Failed to create VM");
    
    // Wait for VM creation to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // List VMs to verify creation
    let vms = client.list_vms().await?;
    assert_eq!(vms.len(), 1, "Expected 1 VM, found {}", vms.len());
    assert_eq!(vms[0].name, "test-vm-1");
    
    // Start the VM
    println!("Starting VM: test-vm-1");
    let start_request = iroh_types::StartVmRequest {
        name: "test-vm-1".to_string(),
    };
    let start_response = client.start_vm(start_request).await?;
    assert!(start_response.into_inner().success, "Failed to start VM");
    
    // Give the monitoring task time to detect the status change
    println!("VM start command sent, waiting for monitoring to update status...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Wait for VM to start (with timeout)
    let start_result = timeout(Duration::from_secs(30), async {
        loop {
            match client.get_vm_status("test-vm-1".to_string()).await {
                Ok(vm_info) => {
                    if let Some(info) = vm_info {
                        println!("VM state: {} (expecting 3 for Running)", info.state);
                        // VM states: 0=Unknown, 1=Creating, 2=Starting, 3=Running, 4=Stopping, 5=Stopped, 6=Failed
                        if info.state == 3 { // VmState::VmStateRunning
                            return Ok(());
                        } else if info.state == 2 { // VmState::VmStateStarting
                            println!("VM is in Starting state, waiting for transition to Running...");
                        }
                    } else {
                        println!("VM not found in status response");
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(e) => {
                    println!("Error getting VM status: {}", e);
                    return Err(e);
                }
            }
        }
    })
    .await;
    
    match start_result {
        Ok(Ok(())) => println!("VM started successfully"),
        Ok(Err(e)) => panic!("Failed to start VM: {}", e),
        Err(_) => panic!("Timeout waiting for VM to start"),
    }
    
    // Stop the VM
    println!("Stopping VM: test-vm-1");
    let stop_request = iroh_types::StopVmRequest {
        name: "test-vm-1".to_string(),
    };
    let stop_response = client.stop_vm(stop_request).await?;
    assert!(stop_response.into_inner().success, "Failed to stop VM");
    
    // Wait for VM to stop
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify VM is stopped
    let vm_info = client.get_vm_status("test-vm-1".to_string()).await?;
    assert!(vm_info.is_some());
    assert_eq!(vm_info.unwrap().state, 5); // VmState::VmStateStopped
    
    // Delete the VM
    println!("Deleting VM: test-vm-1");
    let delete_request = iroh_types::DeleteVmRequest {
        name: "test-vm-1".to_string(),
    };
    let delete_response = client.delete_vm(delete_request).await?;
    assert!(delete_response.into_inner().success, "Failed to delete VM");
    
    // Wait for deletion to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Verify VM is deleted
    let vms = client.list_vms().await?;
    assert_eq!(vms.len(), 0, "Expected 0 VMs after deletion");
    
    Ok(())
}

#[tokio::test]
async fn test_three_node_cluster_vm_scheduling() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=debug")
        .try_init();
    
    // Create a three-node cluster
    let node1 = TestNode::builder()
        .with_id(1)
        .with_port(9001)
        .with_vm_backend("mock".to_string())
        .build()
        .await?;
    
    // Give node1 time to become leader
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    let node2 = TestNode::builder()
        .with_id(2)
        .with_port(9002)
        .with_vm_backend("mock".to_string())
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;
        
    let node3 = TestNode::builder()
        .with_id(3)
        .with_port(9003)
        .with_vm_backend("mock".to_string())
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;
    
    // Give cluster time to stabilize
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify cluster formation
    let client1 = node1.client().await?;
    let (leader_id, nodes, _term) = client1.get_cluster_status().await?;
    assert_eq!(nodes.len(), 3, "Expected 3 nodes in cluster");
    println!("Cluster formed with leader: {}, nodes: {}", leader_id, nodes.len());
    
    // Create VMs - they will be created on the node we're connected to (node1)
    let vm1_config = iroh_types::VmConfig {
        name: "test-vm-1".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 5,
        owner: "test-user".to_string(),
        metadata: HashMap::new(),
    };
    
    let vm2_config = iroh_types::VmConfig {
        name: "test-vm-2".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 5,
        owner: "test-user".to_string(),
        metadata: HashMap::new(),
    };
    
    let vm3_config = iroh_types::VmConfig {
        name: "test-vm-3".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 5,
        owner: "test-user".to_string(),
        metadata: HashMap::new(),
    };
    
    // Create VMs through leader (node1)
    println!("Creating VMs through the cluster");
    let create_response1 = client1.create_vm(vm1_config).await?;
    assert!(create_response1.into_inner().success, "Failed to create VM 1");
    
    let create_response2 = client1.create_vm(vm2_config).await?;
    assert!(create_response2.into_inner().success, "Failed to create VM 2");
    
    let create_response3 = client1.create_vm(vm3_config).await?;
    assert!(create_response3.into_inner().success, "Failed to create VM 3");
    
    // Wait for VMs to be created
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // List VMs from different nodes - should see all VMs
    let vms_from_node1 = client1.list_vms().await?;
    assert_eq!(vms_from_node1.len(), 3, "Node1 should see all 3 VMs");
    
    let client2 = node2.client().await?;
    let vms_from_node2 = client2.list_vms().await?;
    assert_eq!(vms_from_node2.len(), 3, "Node2 should see all 3 VMs");
    
    let client3 = node3.client().await?;
    let vms_from_node3 = client3.list_vms().await?;
    assert_eq!(vms_from_node3.len(), 3, "Node3 should see all 3 VMs");
    
    // Start VMs
    println!("Starting VMs");
    let start1 = client1.start_vm(iroh_types::StartVmRequest { 
        name: "test-vm-1".to_string() 
    }).await?;
    assert!(start1.into_inner().success);
    
    let start2 = client1.start_vm(iroh_types::StartVmRequest { 
        name: "test-vm-2".to_string() 
    }).await?;
    assert!(start2.into_inner().success);
    
    let start3 = client1.start_vm(iroh_types::StartVmRequest { 
        name: "test-vm-3".to_string() 
    }).await?;
    assert!(start3.into_inner().success);
    
    // Wait for VMs to start
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Verify VM states from different nodes
    let vm1_status = client2.get_vm_status("test-vm-1".to_string()).await?;
    assert!(vm1_status.is_some());
    assert_eq!(vm1_status.unwrap().state, 3); // Running
    
    let vm2_status = client3.get_vm_status("test-vm-2".to_string()).await?;
    assert!(vm2_status.is_some());
    assert_eq!(vm2_status.unwrap().state, 3); // Running
    
    let vm3_status = client1.get_vm_status("test-vm-3".to_string()).await?;
    assert!(vm3_status.is_some());
    assert_eq!(vm3_status.unwrap().state, 3); // Running
    
    // Clean up - delete VMs
    println!("Cleaning up VMs");
    let delete1 = client1.delete_vm(iroh_types::DeleteVmRequest { 
        name: "test-vm-1".to_string() 
    }).await?;
    assert!(delete1.into_inner().success);
    
    let delete2 = client1.delete_vm(iroh_types::DeleteVmRequest { 
        name: "test-vm-2".to_string() 
    }).await?;
    assert!(delete2.into_inner().success);
    
    let delete3 = client1.delete_vm(iroh_types::DeleteVmRequest { 
        name: "test-vm-3".to_string() 
    }).await?;
    assert!(delete3.into_inner().success);
    
    Ok(())
}

#[tokio::test]
#[ignore] // This test requires proper network setup
async fn test_vm_with_network_connectivity() -> BlixardResult<()> {
    // Create and start a single node with mock backend for testing
    let node = TestNode::builder()
        .with_id(1)
        .with_port(9002)
        .with_vm_backend("mock".to_string())
        .build()
        .await?;
    
    // Give the node time to initialize
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Get a client to interact with the node
    let client = node.client().await?;
    
    // Create a VM with specific network configuration
    let vm_config = iroh_types::VmConfig {
        name: "test-vm-net".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 5,
        owner: "test-user".to_string(),
        metadata: {
            let mut metadata = HashMap::new();
            metadata.insert("network".to_string(), "default".to_string());
            metadata.insert("ip_address".to_string(), "10.0.0.20".to_string());
            metadata
        },
    };
    
    // Create the VM
    println!("Creating VM: {}", vm_config.name);
    let create_response = client.create_vm(vm_config).await?;
    assert!(create_response.into_inner().success, "Failed to create VM");
    
    // Wait for VM creation to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Start the VM
    println!("Starting VM: test-vm-net");
    let start_request = iroh_types::StartVmRequest {
        name: "test-vm-net".to_string(),
    };
    let start_response = client.start_vm(start_request).await?;
    assert!(start_response.into_inner().success, "Failed to start VM");
    
    // Wait for VM to be fully up
    let _start_result = timeout(Duration::from_secs(60), async {
        loop {
            match client.get_vm_status("test-vm-net".to_string()).await {
                Ok(vm_info) => {
                    if let Some(info) = vm_info {
                        if info.state == 3 { // VmState::VmStateRunning
                            // Additional wait for network to be ready
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            return Ok(());
                        }
                        println!("VM state: {}", info.state);
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(e) => return Err(e),
            }
        }
    })
    .await
    .expect("Timeout waiting for VM")
    .expect("Failed to start VM");
    
    // Test network connectivity (ping the VM)
    let ping_output = tokio::process::Command::new("ping")
        .args(&["-c", "3", "-W", "1", "10.0.0.20"])
        .output()
        .await
        .expect("Failed to execute ping");
    
    assert!(
        ping_output.status.success(),
        "Failed to ping VM at 10.0.0.20: {}",
        String::from_utf8_lossy(&ping_output.stderr)
    );
    
    // Clean up
    println!("Stopping VM: test-vm-net");
    let stop_request = iroh_types::StopVmRequest {
        name: "test-vm-net".to_string(),
    };
    let stop_response = client.stop_vm(stop_request).await?;
    assert!(stop_response.into_inner().success, "Failed to stop VM");
    
    // Wait for VM to stop
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    println!("Deleting VM: test-vm-net");
    let delete_request = iroh_types::DeleteVmRequest {
        name: "test-vm-net".to_string(),
    };
    let delete_response = client.delete_vm(delete_request).await?;
    assert!(delete_response.into_inner().success, "Failed to delete VM");
    
    Ok(())
}