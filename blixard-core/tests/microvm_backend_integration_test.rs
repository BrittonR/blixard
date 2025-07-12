#![cfg(all(feature = "test-helpers", target_os = "linux"))]

use blixard_core::{
    error::BlixardResult, iroh_types, test_helpers::TestNode, vm_backend::VmBackendRegistry,
};
use blixard_vm::MicrovmBackendFactory;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Test that verifies the real microvm backend can be registered and used
#[tokio::test]
#[ignore] // This test requires Nix and proper system setup
async fn test_microvm_backend_registration() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=debug,blixard_vm=debug")
        .try_init();

    // Create a custom VM backend registry with microvm support
    let mut registry = VmBackendRegistry::default();
    registry.register(Arc::new(MicrovmBackendFactory));

    // Verify the microvm backend is available
    let backends = registry.list_available_backends();
    assert!(
        backends.contains(&"mock"),
        "Mock backend should be available"
    );
    assert!(
        backends.contains(&"microvm"),
        "Microvm backend should be available"
    );

    println!("Available backends: {:?}", backends);

    Ok(())
}

/// Test creating a simple VM with the microvm backend
#[tokio::test]
#[ignore] // This test requires Nix, microvm.nix, and proper system setup
async fn test_microvm_create_simple_vm() -> BlixardResult<()> {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=debug,blixard_vm=debug")
        .try_init();

    // Create a test node with microvm backend
    let node = TestNode::builder()
        .with_id(1)
        .with_port(9011)
        .with_vm_backend("microvm".to_string())
        .with_custom_vm_registry({
            let mut registry = VmBackendRegistry::default();
            registry.register(Arc::new(MicrovmBackendFactory));
            registry
        })
        .build()
        .await?;

    // Give the node time to initialize
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Get a client to interact with the node
    let client = node.client().await?;

    // Create a minimal VM configuration
    let vm_config = iroh_types::VmConfig {
        name: "test-microvm-1".to_string(),
        cpu_cores: 1,
        memory_mb: 256, // Use minimal memory for testing
        disk_gb: 1,     // Minimal disk
        owner: "test-user".to_string(),
        metadata: HashMap::new(),
    };

    // Create the VM
    println!("Creating microvm: {}", vm_config.name);
    let create_response = client.create_vm(vm_config).await?;
    assert!(
        create_response.into_inner().success,
        "Failed to create microvm"
    );

    // Wait for VM creation to propagate
    tokio::time::sleep(Duration::from_secs(2)).await;

    // List VMs to verify creation
    let vms = client.list_vms().await?;
    assert_eq!(vms.len(), 1, "Expected 1 VM, found {}", vms.len());
    assert_eq!(vms[0].name, "test-microvm-1");

    // Note: We don't start the VM in this test as it requires
    // proper networking setup, tap interfaces, etc.

    // Delete the VM
    println!("Deleting microvm: test-microvm-1");
    let delete_request = iroh_types::DeleteVmRequest {
        name: "test-microvm-1".to_string(),
    };
    let delete_response = client.delete_vm(delete_request).await?;
    assert!(
        delete_response.into_inner().success,
        "Failed to delete microvm"
    );

    Ok(())
}

/// Test VM lifecycle with proper network setup
#[tokio::test]
#[ignore] // This test requires Nix, network setup, and root/sudo access
async fn test_microvm_with_networking() -> BlixardResult<()> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=debug,blixard_vm=debug")
        .try_init();

    // Check if we have the necessary permissions
    if !check_network_permissions() {
        eprintln!("Skipping test: requires network setup permissions");
        eprintln!("Run: sudo ./scripts/setup-tap-networking.sh");
        return Ok(());
    }

    // Create a test node with microvm backend
    let node = TestNode::builder()
        .with_id(1)
        .with_port(9012)
        .with_vm_backend("microvm".to_string())
        .with_custom_vm_registry({
            let mut registry = VmBackendRegistry::default();
            registry.register(Arc::new(MicrovmBackendFactory));
            registry
        })
        .build()
        .await?;

    // Give the node time to initialize
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Get a client
    let client = node.client().await?;

    // Create a VM with network configuration
    let mut metadata = HashMap::new();
    metadata.insert("tap_interface".to_string(), "vm12".to_string());
    metadata.insert("ip_address".to_string(), "10.0.0.12".to_string());

    let vm_config = iroh_types::VmConfig {
        name: "test-microvm-net".to_string(),
        cpu_cores: 1,
        memory_mb: 512,
        disk_gb: 2,
        owner: "test-user".to_string(),
        metadata,
    };

    // Create and start the VM
    println!("Creating microvm with networking: {}", vm_config.name);
    let create_response = client.create_vm(vm_config).await?;
    assert!(create_response.into_inner().success, "Failed to create VM");

    // Start the VM
    println!("Starting microvm");
    let start_request = iroh_types::StartVmRequest {
        name: "test-microvm-net".to_string(),
    };
    let start_response = client.start_vm(start_request).await?;
    assert!(start_response.into_inner().success, "Failed to start VM");

    // Wait for VM to start (longer timeout for real VMs)
    let start_result = timeout(Duration::from_secs(120), async {
        loop {
            match client.get_vm_status("test-microvm-net".to_string()).await {
                Ok(vm_info) => {
                    if let Some(info) = vm_info {
                        println!("VM state: {} (expecting 3 for Running)", info.state);
                        if info.state == 3 {
                            // VmState::VmStateRunning
                            return Ok(());
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
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

    // Test network connectivity (optional)
    // Note: IP retrieval method needs to be implemented in the client
    println!(
        "VM started successfully - network connectivity test skipped (get_vm_ip not implemented)"
    );

    // Stop and delete the VM
    println!("Stopping microvm");
    let stop_request = iroh_types::StopVmRequest {
        name: "test-microvm-net".to_string(),
    };
    let stop_response = client.stop_vm(stop_request).await?;
    assert!(stop_response.into_inner().success, "Failed to stop VM");

    // Wait for VM to stop
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("Deleting microvm");
    let delete_request = iroh_types::DeleteVmRequest {
        name: "test-microvm-net".to_string(),
    };
    let delete_response = client.delete_vm(delete_request).await?;
    assert!(delete_response.into_inner().success, "Failed to delete VM");

    Ok(())
}

/// Check if we have the necessary permissions for network setup
fn check_network_permissions() -> bool {
    // Check if tap interface vm12 exists
    let result = std::process::Command::new("ip")
        .args(&["link", "show", "vm12"])
        .output();

    match result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Test VM scheduling with real microvm backend
#[tokio::test]
#[ignore] // Requires full system setup
async fn test_microvm_cluster_scheduling() -> BlixardResult<()> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("blixard_core=info")
        .try_init();

    // Create a three-node cluster with microvm backend
    let registry = {
        let mut r = VmBackendRegistry::default();
        r.register(Arc::new(MicrovmBackendFactory));
        r
    };

    let node1 = TestNode::builder()
        .with_id(1)
        .with_port(9021)
        .with_vm_backend("microvm".to_string())
        .with_custom_vm_registry(registry.clone())
        .build()
        .await?;

    // Give node1 time to become leader
    tokio::time::sleep(Duration::from_secs(3)).await;

    let node2 = TestNode::builder()
        .with_id(2)
        .with_port(9022)
        .with_vm_backend("microvm".to_string())
        .with_custom_vm_registry(registry.clone())
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;

    let node3 = TestNode::builder()
        .with_id(3)
        .with_port(9023)
        .with_vm_backend("microvm".to_string())
        .with_custom_vm_registry(registry.clone())
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;

    // Give cluster time to stabilize
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify cluster formation
    let client1 = node1.client().await?;
    let (leader_id, nodes, _term) = client1.get_cluster_status().await?;
    assert_eq!(nodes.len(), 3, "Expected 3 nodes in cluster");
    println!(
        "Cluster formed with leader: {}, nodes: {}",
        leader_id,
        nodes.len()
    );

    // Create small VMs for testing (don't start them)
    for i in 1..=3 {
        let vm_config = iroh_types::VmConfig {
            name: format!("test-cluster-vm-{}", i),
            cpu_cores: 1,
            memory_mb: 128, // Minimal memory
            disk_gb: 1,
            owner: "test-user".to_string(),
            metadata: HashMap::new(),
        };

        println!("Creating VM: {}", vm_config.name);
        let create_response = client1.create_vm(vm_config).await?;
        assert!(create_response.into_inner().success);
    }

    // Wait for VMs to be created
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify all nodes see the VMs
    let vms_from_node1 = client1.list_vms().await?;
    assert_eq!(vms_from_node1.len(), 3, "Node1 should see all 3 VMs");

    let client2 = node2.client().await?;
    let vms_from_node2 = client2.list_vms().await?;
    assert_eq!(vms_from_node2.len(), 3, "Node2 should see all 3 VMs");

    let client3 = node3.client().await?;
    let vms_from_node3 = client3.list_vms().await?;
    assert_eq!(vms_from_node3.len(), 3, "Node3 should see all 3 VMs");

    // Clean up
    for i in 1..=3 {
        let delete_request = iroh_types::DeleteVmRequest {
            name: format!("test-cluster-vm-{}", i),
        };
        let delete_response = client1.delete_vm(delete_request).await?;
        assert!(delete_response.into_inner().success);
    }

    println!("Test completed successfully");
    Ok(())
}
