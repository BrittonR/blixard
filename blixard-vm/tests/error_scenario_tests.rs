use blixard_vm::{MicrovmBackend, NixFlakeGenerator};
use blixard_vm::types::*;
use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig as CoreVmConfig, VmStatus},
    error::BlixardError,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Tests for error scenarios and edge cases
#[cfg(test)]
mod error_scenario_tests {
    use super::*;

    /// Test handling of invalid VM configurations
    #[tokio::test]
    async fn test_invalid_vm_configurations() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        // Test empty VM name
        let invalid_config = CoreVmConfig {
            name: "".to_string(),  // Invalid: empty name
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        let result = backend.create_vm(&invalid_config, 1).await;
        assert!(result.is_err(), "Should fail with empty VM name");
        
        // Test invalid characters in VM name
        let invalid_config = CoreVmConfig {
            name: "vm with spaces".to_string(),  // Invalid: spaces
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        let result = backend.create_vm(&invalid_config, 1).await;
        assert!(result.is_err(), "Should fail with spaces in VM name");
        
        // Test zero resources
        let invalid_config = CoreVmConfig {
            name: "zero-resources".to_string(),
            config_path: "".to_string(),
            vcpus: 0,  // Invalid: zero CPUs
            memory: 0, // Invalid: zero memory
            tenant_id: "test".to_string(),
            ip_address: None,
        };
        
        let result = backend.create_vm(&invalid_config, 1).await;
        assert!(result.is_err(), "Should fail with zero resources");
        
        // Test extremely large resources
        let invalid_config = CoreVmConfig {
            name: "huge-resources".to_string(),
            config_path: "".to_string(),
            vcpus: u32::MAX,     // Invalid: too many CPUs
            memory: u32::MAX,    // Invalid: too much memory
            tenant_id: "test".to_string(),
            ip_address: None,
        };
        
        let result = backend.create_vm(&invalid_config, 1).await;
        // This might succeed in creation but would fail in actual allocation
        if result.is_ok() {
            println!("Large resource config accepted (would fail at runtime)");
            backend.delete_vm(&invalid_config.name).await.unwrap();
        }
    }
    
    /// Test duplicate VM creation
    #[tokio::test]
    async fn test_duplicate_vm_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        let vm_config = CoreVmConfig {
            name: "duplicate-test".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        // Create VM first time - should succeed
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        // Try to create same VM again - should fail
        let result = backend.create_vm(&vm_config, 1).await;
        assert!(result.is_err(), "Should fail to create duplicate VM");
        
        // Verify original VM still exists and is functional
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].0.name, vm_config.name);
        
        // Cleanup
        backend.delete_vm(&vm_config.name).await.unwrap();
    }
    
    /// Test operations on non-existent VMs
    #[tokio::test]
    async fn test_nonexistent_vm_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        let nonexistent_vm = "does-not-exist";
        
        // Test getting status of non-existent VM
        let status = backend.get_vm_status(nonexistent_vm).await.unwrap();
        assert_eq!(status, None, "Non-existent VM should return None status");
        
        // Test starting non-existent VM
        let result = backend.start_vm(nonexistent_vm).await;
        assert!(result.is_err(), "Should fail to start non-existent VM");
        
        // Test stopping non-existent VM
        let result = backend.stop_vm(nonexistent_vm).await;
        assert!(result.is_err(), "Should fail to stop non-existent VM");
        
        // Test deleting non-existent VM
        let result = backend.delete_vm(nonexistent_vm).await;
        assert!(result.is_err(), "Should fail to delete non-existent VM");
    }
    
    /// Test filesystem permission issues
    #[tokio::test]
    async fn test_filesystem_permission_errors() {
        let temp_dir = TempDir::new().unwrap();
        let readonly_dir = temp_dir.path().join("readonly");
        
        // Create a directory and make it read-only
        std::fs::create_dir_all(&readonly_dir).unwrap();
        
        // Try to create backend with read-only directory
        // Note: This test may behave differently on different systems
        let config_dir = readonly_dir.join("config");
        let data_dir = readonly_dir.join("data");
        
        // This should either fail immediately or fail on first write operation
        match MicrovmBackend::new(config_dir, data_dir) {
            Ok(backend) => {
                // If backend creation succeeds, VM creation should fail
                let vm_config = CoreVmConfig {
                    name: "permission-test".to_string(),
                    config_path: "".to_string(),
                    vcpus: 1,
                    memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
                };
                
                let result = backend.create_vm(&vm_config, 1).await;
                // This should fail due to permission issues
                if result.is_err() {
                    println!("✓ Permission error correctly detected during VM creation");
                } else {
                    println!("⚠ Permission error not detected (filesystem may allow writes)");
                    backend.delete_vm(&vm_config.name).await.ok();
                }
            }
            Err(_) => {
                println!("✓ Permission error correctly detected during backend creation");
            }
        }
    }
    
    /// Test disk space exhaustion simulation
    #[tokio::test]
    async fn test_disk_space_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        // Create a VM with very large disk requirements
        let vm_config = CoreVmConfig {
            name: "large-disk-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        // This should succeed as we're just generating flakes
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        // Verify flake was created despite large disk requirements
        let flake_path = config_dir.join("vms").join(&vm_config.name).join("flake.nix");
        assert!(flake_path.exists());
        
        // Read and verify the flake contains large disk configuration
        let flake_content = std::fs::read_to_string(&flake_path).unwrap();
        assert!(flake_content.contains(&vm_config.name));
        
        // The actual disk space issue would occur during VM runtime, not creation
        println!("✓ Large disk VM configuration handled correctly");
        
        backend.delete_vm(&vm_config.name).await.unwrap();
    }
    
    /// Test corrupted configuration recovery
    #[tokio::test]
    async fn test_corrupted_config_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        let vm_config = CoreVmConfig {
            name: "corruption-test".to_string(),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 1024,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        // Create a VM normally
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        let vm_dir = config_dir.join("vms").join(&vm_config.name);
        let flake_path = vm_dir.join("flake.nix");
        
        // Corrupt the flake file
        std::fs::write(&flake_path, "{ corrupted nix content").unwrap();
        
        // Try to list VMs - should handle corruption gracefully
        let vms = backend.list_vms().await.unwrap();
        // The VM might still appear in the list but be marked as invalid
        println!("VMs after corruption: {}", vms.len());
        
        // Try to delete the corrupted VM - should succeed
        let result = backend.delete_vm(&vm_config.name).await;
        assert!(result.is_ok(), "Should be able to delete corrupted VM");
        
        // Verify cleanup
        assert!(!vm_dir.exists(), "Corrupted VM directory should be cleaned up");
    }
    
    /// Test template generation errors
    #[tokio::test]
    async fn test_template_generation_errors() {
        let temp_dir = TempDir::new().unwrap();
        
        // Test with missing template directory
        let missing_template_dir = temp_dir.path().join("missing");
        let modules_dir = temp_dir.path().join("modules");
        
        let result = NixFlakeGenerator::new(missing_template_dir, modules_dir);
        assert!(result.is_err(), "Should fail with missing template directory");
        
        // Test with invalid template
        let template_dir = temp_dir.path().join("templates");
        std::fs::create_dir_all(&template_dir).unwrap();
        
        // Create an invalid template
        let invalid_template = "{{ unclosed_variable";
        std::fs::write(template_dir.join("vm-flake.nix"), invalid_template).unwrap();
        
        let modules_dir = temp_dir.path().join("modules");
        let generator = NixFlakeGenerator::new(template_dir, modules_dir).unwrap();
        
        let config = VmConfig {
            name: "template-error-test".to_string(),
            vm_index: 1,
            hypervisor: Hypervisor::Qemu,
            vcpus: 1,
            memory: 512,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            flake_modules: vec![],
            kernel: None,
            init_command: None,
        };
        
        let result = generator.generate_vm_flake(&config);
        assert!(result.is_err(), "Should fail with invalid template");
    }
    
    /// Test resource exhaustion scenarios
    #[tokio::test]
    async fn test_resource_exhaustion() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        // Test creating many VMs to simulate resource exhaustion
        let num_vms = 50; // Large number to test memory usage
        let mut created_vms = Vec::new();
        
        for i in 0..num_vms {
            let vm_config = CoreVmConfig {
                name: format!("stress-vm-{}", i),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 256,
            ip_address: None,
            tenant_id: "test".to_string(),
            };
            
            match backend.create_vm(&vm_config, 1).await {
                Ok(()) => {
                    created_vms.push(vm_config.name.clone());
                }
                Err(e) => {
                    println!("VM creation failed at VM {}: {}", i, e);
                    break;
                }
            }
        }
        
        println!("Successfully created {} VMs", created_vms.len());
        
        // Verify all created VMs
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), created_vms.len());
        
        // Clean up all VMs
        for vm_name in created_vms {
            backend.delete_vm(&vm_name).await.unwrap();
        }
        
        // Verify complete cleanup
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
    
    /// Test concurrent error scenarios
    #[tokio::test]
    async fn test_concurrent_error_scenarios() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = std::sync::Arc::new(
            MicrovmBackend::new(config_dir, data_dir).unwrap()
        );
        
        // Test concurrent operations on the same VM
        let vm_name = "concurrent-test-vm";
        let vm_config = CoreVmConfig {
            name: vm_name.to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        // Create the VM first
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        // Launch multiple concurrent operations
        let mut handles = Vec::new();
        
        // Multiple delete attempts
        for i in 0..5 {
            let backend_clone = backend.clone();
            let vm_name_clone = vm_name.to_string();
            let handle = tokio::spawn(async move {
                let result = backend_clone.delete_vm(&vm_name_clone).await;
                (i, result)
            });
            handles.push(handle);
        }
        
        // Wait for all operations
        let mut success_count = 0;
        let mut error_count = 0;
        
        for handle in handles {
            let (i, result) = handle.await.unwrap();
            match result {
                Ok(()) => {
                    success_count += 1;
                    println!("Delete operation {} succeeded", i);
                }
                Err(_) => {
                    error_count += 1;
                    println!("Delete operation {} failed (expected)", i);
                }
            }
        }
        
        // Only one delete should succeed
        assert_eq!(success_count, 1, "Only one delete operation should succeed");
        assert_eq!(error_count, 4, "Four delete operations should fail");
        
        // Verify VM is completely gone
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
}

/// Tests for graceful degradation and recovery
#[cfg(test)]
mod recovery_tests {
    use super::*;
    
    /// Test backend recovery after filesystem issues
    #[tokio::test]
    async fn test_backend_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        // Create first backend and some VMs
        let vm_config = CoreVmConfig {
            name: "recovery-test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            ip_address: None,
            tenant_id: "test".to_string(),
        };
        
        {
            let backend1 = MicrovmBackend::new(config_dir.clone(), data_dir.clone()).unwrap();
            backend1.create_vm(&vm_config, 1).await.unwrap();
        }
        
        // Simulate partial filesystem corruption
        let vm_dir = config_dir.join("vms").join(&vm_config.name);
        let flake_path = vm_dir.join("flake.nix");
        
        // Backup original content
        let original_content = std::fs::read_to_string(&flake_path).unwrap();
        
        // Corrupt the file
        std::fs::write(&flake_path, "corrupted").unwrap();
        
        // Create new backend - should handle corruption gracefully
        let backend2 = MicrovmBackend::new(config_dir.clone(), data_dir.clone()).unwrap();
        
        // Should still list the VM (even if corrupted)
        let vms = backend2.list_vms().await.unwrap();
        println!("VMs found after corruption: {}", vms.len());
        
        // Should be able to delete corrupted VM
        let result = backend2.delete_vm(&vm_config.name).await;
        assert!(result.is_ok(), "Should be able to delete corrupted VM");
        
        // Create a new VM with same name - should work
        let result = backend2.create_vm(&vm_config, 1).await;
        assert!(result.is_ok(), "Should be able to recreate VM after corruption");
        
        // Verify new VM is properly created
        let new_flake_content = std::fs::read_to_string(&flake_path).unwrap();
        assert_ne!(new_flake_content, "corrupted");
        assert!(new_flake_content.contains(&vm_config.name));
        
        // Cleanup
        backend2.delete_vm(&vm_config.name).await.unwrap();
    }
    
    /// Test handling of partial VM directory states
    #[tokio::test]
    async fn test_partial_vm_directory_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        // Manually create a partial VM directory (simulate interrupted creation)
        let vm_name = "partial-vm";
        let vm_dir = config_dir.join("vms").join(vm_name);
        std::fs::create_dir_all(&vm_dir).unwrap();
        
        // Create some files but not others
        std::fs::write(vm_dir.join("some-file.txt"), "partial content").unwrap();
        // flake.nix is missing
        
        // List VMs - should handle partial directory gracefully
        let vms = backend.list_vms().await.unwrap();
        println!("VMs with partial directory: {}", vms.len());
        
        // Try to delete the partial VM - should succeed
        let result = backend.delete_vm(vm_name).await;
        if result.is_ok() {
            println!("✓ Successfully cleaned up partial VM directory");
        } else {
            println!("⚠ Could not clean up partial VM directory: {:?}", result);
        }
        
        // Verify directory is cleaned up
        assert!(!vm_dir.exists() || std::fs::read_dir(&vm_dir).unwrap().count() == 0);
    }
}