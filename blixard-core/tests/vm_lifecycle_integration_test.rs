#![cfg(feature = "test-helpers")]

use blixard_core::{
    error::BlixardResult,
    test_helpers::TestNode,
    types::{Hypervisor, VmConfig, VmStatus},
};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_single_node_vm_create_start_stop_delete() -> BlixardResult<()> {
    // Create and start a single node
    let mut node = TestNode::new(1).await;
    node.start().await?;
    
    // Give the node time to initialize
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Create a VM configuration
    let vm_config = VmConfig {
        name: "test-vm-1".to_string(),
        vcpus: 1,
        memory: 512, // 512MB
        disk: 5,     // 5GB
        image: None,
        network: None,
        ip_address: None,
        hypervisor: Hypervisor::Qemu,
        metadata: Default::default(),
    };
    
    // Create the VM
    println!("Creating VM: {}", vm_config.name);
    node.create_vm(vm_config.clone()).await?;
    
    // Wait for VM creation to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // List VMs to verify creation
    let vms = node.list_vms().await?;
    assert_eq!(vms.len(), 1, "Expected 1 VM, found {}", vms.len());
    assert_eq!(vms[0].name, "test-vm-1");
    assert_eq!(vms[0].status, VmStatus::Creating);
    
    // Start the VM
    println!("Starting VM: test-vm-1");
    node.start_vm("test-vm-1").await?;
    
    // Wait for VM to start (with timeout)
    let start_result = timeout(Duration::from_secs(30), async {
        loop {
            match node.get_vm_status("test-vm-1").await {
                Ok(status) if status == VmStatus::Running => return Ok(()),
                Ok(status) => {
                    println!("VM status: {:?}", status);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(e) => return Err(e),
            }
        }
    })
    .await;
    
    match start_result {
        Ok(Ok(())) => println!("VM started successfully"),
        Ok(Err(e)) => panic!("Failed to start VM: {}", e),
        Err(_) => panic!("Timeout waiting for VM to start"),
    }
    
    // Verify VM is running
    let status = node.get_vm_status("test-vm-1").await?;
    assert_eq!(status, VmStatus::Running);
    
    // Stop the VM
    println!("Stopping VM: test-vm-1");
    node.stop_vm("test-vm-1").await?;
    
    // Wait for VM to stop
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify VM is stopped
    let status = node.get_vm_status("test-vm-1").await?;
    assert_eq!(status, VmStatus::Stopped);
    
    // Delete the VM
    println!("Deleting VM: test-vm-1");
    node.delete_vm("test-vm-1").await?;
    
    // Wait for deletion to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Verify VM is deleted
    let vms = node.list_vms().await?;
    assert_eq!(vms.len(), 0, "Expected 0 VMs after deletion");
    
    // Clean up
    node.stop().await?;
    
    Ok(())
}

#[tokio::test]
#[ignore] // This test requires proper network setup
async fn test_vm_with_network_connectivity() -> BlixardResult<()> {
    // Create and start a single node
    let mut node = TestNode::new(1).await;
    node.start().await?;
    
    // Give the node time to initialize
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Create a VM with specific network configuration
    let vm_config = VmConfig {
        name: "test-vm-net".to_string(),
        vcpus: 1,
        memory: 512,
        disk: 5,
        image: None,
        network: Some("default".to_string()),
        ip_address: Some("10.0.0.20".to_string()),
        hypervisor: Hypervisor::Qemu,
        metadata: Default::default(),
    };
    
    // Create and start the VM
    node.create_vm(vm_config).await?;
    node.start_vm("test-vm-net").await?;
    
    // Wait for VM to be fully up
    let _start_result = timeout(Duration::from_secs(60), async {
        loop {
            match node.get_vm_status("test-vm-net").await {
                Ok(VmStatus::Running) => {
                    // Additional wait for network to be ready
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    return Ok(());
                }
                Ok(status) => {
                    println!("VM status: {:?}", status);
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
    node.stop_vm("test-vm-net").await?;
    node.delete_vm("test-vm-net").await?;
    node.stop().await?;
    
    Ok(())
}