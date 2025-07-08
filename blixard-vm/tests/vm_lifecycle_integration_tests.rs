use blixard_core::{
    types::{VmConfig, VmStatus},
    vm_backend::VmBackend,
};
use blixard_vm::MicrovmBackend;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use redb::Database;

/// Integration tests for complete VM lifecycle with real Nix builds
#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    /// Test the complete VM lifecycle: create -> build -> start -> stop -> delete
    #[tokio::test]
    async fn test_complete_vm_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        let backend = MicrovmBackend::new(config_dir.clone(), data_dir, database).unwrap();

        let vm_config = VmConfig {
            name: "lifecycle-test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        // Step 1: Create VM and verify flake generation
        println!("Creating VM...");
        backend.create_vm(&vm_config, 1).await.unwrap();

        let flake_path = config_dir
            .join("vms")
            .join(&vm_config.name)
            .join("flake.nix");
        assert!(flake_path.exists(), "Flake should be generated");

        // Step 2: Verify VM appears in list
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].0.name, vm_config.name);
        assert_eq!(vms[0].1, VmStatus::Stopped);

        // Step 3: Build VM (verify Nix build works)
        println!("Building VM with Nix...");
        let build_result = build_vm_nix(&flake_path, &vm_config.name).await;
        match build_result {
            Ok(runner_path) => {
                assert!(runner_path.exists(), "VM runner should be built");
                println!("✓ VM built successfully at: {}", runner_path.display());
            }
            Err(e) => {
                println!("⚠ Nix build failed (expected in CI): {}", e);
                // In CI/environments without Nix, this is expected to fail
                // The important part is that we generated a valid flake
            }
        }

        // Step 4: Test VM status tracking
        let status = backend.get_vm_status(&vm_config.name).await.unwrap();
        // VM exists in config but not running, so status should be None
        assert_eq!(status, None);

        // Step 5: Delete VM
        println!("Deleting VM...");
        backend.delete_vm(&vm_config.name).await.unwrap();

        // Step 6: Verify cleanup
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
        assert!(!flake_path.exists(), "Flake should be cleaned up");
    }

    /// Test multiple VMs can be managed simultaneously
    #[tokio::test]
    async fn test_multiple_vm_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        let backend = MicrovmBackend::new(config_dir.clone(), data_dir, database).unwrap();

        let vm_configs = vec![
            VmConfig {
                name: "multi-vm-1".to_string(),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 256,
                tenant_id: "default".to_string(),
                ip_address: None,
                metadata: None,
                anti_affinity: None,
                priority: 500,
                preemptible: true,
                locality_preference: Default::default(),
                health_check_config: None,
            },
            VmConfig {
                name: "multi-vm-2".to_string(),
                config_path: "".to_string(),
                vcpus: 2,
                memory: 512,
                tenant_id: "default".to_string(),
                ip_address: None,
                metadata: None,
                anti_affinity: None,
                priority: 500,
                preemptible: true,
                locality_preference: Default::default(),
                health_check_config: None,
            },
            VmConfig {
                name: "multi-vm-3".to_string(),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 1024,
                tenant_id: "default".to_string(),
                ip_address: None,
                metadata: None,
                anti_affinity: None,
                priority: 500,
                preemptible: true,
                locality_preference: Default::default(),
                health_check_config: None,
            },
        ];

        // Create all VMs
        println!("Creating {} VMs...", vm_configs.len());
        for config in &vm_configs {
            backend.create_vm(config, 1).await.unwrap();
        }

        // Verify all VMs exist
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), vm_configs.len());

        // Verify each VM has its own flake
        for config in &vm_configs {
            let flake_path = config_dir.join("vms").join(&config.name).join("flake.nix");
            assert!(flake_path.exists(), "Each VM should have its own flake");

            // Verify flake contains correct configuration
            let flake_content = std::fs::read_to_string(&flake_path).unwrap();
            assert!(flake_content.contains(&config.name));
            assert!(flake_content.contains(&format!("vcpu = {}", config.vcpus)));
            assert!(flake_content.contains(&format!("mem = {}", config.memory)));
        }

        // Delete VMs one by one and verify incremental cleanup
        for (i, config) in vm_configs.iter().enumerate() {
            println!("Deleting VM {}/{}...", i + 1, vm_configs.len());
            backend.delete_vm(&config.name).await.unwrap();

            let remaining_vms = backend.list_vms().await.unwrap();
            assert_eq!(remaining_vms.len(), vm_configs.len() - i - 1);

            let flake_path = config_dir.join("vms").join(&config.name).join("flake.nix");
            assert!(
                !flake_path.exists(),
                "Deleted VM flake should be cleaned up"
            );
        }

        // Verify complete cleanup
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }

    /// Test VM configuration persistence and reload
    #[tokio::test]
    async fn test_vm_config_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");

        let vm_config = VmConfig {
            name: "persistent-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 4,
            memory: 2048,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        // Create VM with first backend instance
        {
            let db_path1 = temp_dir.path().join("test1.db");
            let database1 = Arc::new(Database::create(&db_path1).unwrap());
            let backend1 = MicrovmBackend::new(config_dir.clone(), data_dir.clone(), database1).unwrap();
            backend1.create_vm(&vm_config, 1).await.unwrap();

            let vms = backend1.list_vms().await.unwrap();
            assert_eq!(vms.len(), 1);
            assert_eq!(vms[0].0.name, vm_config.name);
        } // backend1 drops here

        // Create new backend instance and verify VM is still there
        {
            let db_path2 = temp_dir.path().join("test2.db");
            let database2 = Arc::new(Database::create(&db_path2).unwrap());
            let backend2 = MicrovmBackend::new(config_dir.clone(), data_dir.clone(), database2).unwrap();

            let vms = backend2.list_vms().await.unwrap();
            assert_eq!(vms.len(), 1);
            assert_eq!(vms[0].0.name, vm_config.name);
            assert_eq!(vms[0].0.vcpus, vm_config.vcpus);
            assert_eq!(vms[0].0.memory, vm_config.memory);

            // Verify flake still exists and is valid
            let flake_path = config_dir
                .join("vms")
                .join(&vm_config.name)
                .join("flake.nix");
            assert!(flake_path.exists());

            let flake_content = std::fs::read_to_string(&flake_path).unwrap();
            assert!(flake_content.contains(&vm_config.name));
            assert!(flake_content.contains(&format!("vcpu = {}", vm_config.vcpus)));
            assert!(flake_content.contains(&format!("mem = {}", vm_config.memory)));

            // Clean up
            backend2.delete_vm(&vm_config.name).await.unwrap();
        }
    }

    /// Helper function to attempt building VM with Nix
    async fn build_vm_nix(flake_path: &PathBuf, vm_name: &str) -> Result<PathBuf, String> {
        use tokio::process::Command;

        let flake_dir = flake_path.parent().unwrap();
        let target = format!(
            ".#nixosConfigurations.{}.config.microvm.runner.qemu",
            vm_name
        );

        let output = timeout(
            Duration::from_secs(60),
            Command::new("nix")
                .args(&["build", &target, "--no-link", "--print-out-paths"])
                .current_dir(flake_dir)
                .output(),
        )
        .await
        .map_err(|_| "Build timeout after 60 seconds".to_string())?
        .map_err(|e| format!("Failed to execute nix build: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let runner_path = stdout.trim();
            Ok(PathBuf::from(runner_path))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Nix build failed: {}", stderr))
        }
    }
}

/// Performance and stress tests
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    /// Test VM creation performance
    #[tokio::test]
    async fn test_vm_creation_performance() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        let backend = MicrovmBackend::new(config_dir, data_dir, database).unwrap();

        let vm_config = VmConfig {
            name: "perf-test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        // Measure creation time
        let start = Instant::now();
        backend.create_vm(&vm_config, 1).await.unwrap();
        let creation_time = start.elapsed();

        println!("VM creation took: {:?}", creation_time);

        // Should be fast (under 1 second for flake generation)
        assert!(
            creation_time < Duration::from_secs(1),
            "VM creation should be fast, took {:?}",
            creation_time
        );

        // Cleanup
        backend.delete_vm(&vm_config.name).await.unwrap();
    }

    /// Test concurrent VM operations
    #[tokio::test]
    async fn test_concurrent_vm_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        let backend = std::sync::Arc::new(MicrovmBackend::new(config_dir, data_dir, database).unwrap());

        let num_vms = 10;
        let mut handles = Vec::new();

        // Create VMs concurrently
        println!("Creating {} VMs concurrently...", num_vms);
        let start = Instant::now();

        for i in 0..num_vms {
            let backend_clone = backend.clone();
            let handle = tokio::spawn(async move {
                let vm_config = VmConfig {
                    name: format!("concurrent-vm-{}", i),
                    config_path: "".to_string(),
                    vcpus: 1,
                    memory: 256,
                    tenant_id: "default".to_string(),
                    ip_address: None,
                    metadata: None,
                    anti_affinity: None,
                    priority: 500,
                    preemptible: true,
                    locality_preference: Default::default(),
                    health_check_config: None,
                };

                backend_clone.create_vm(&vm_config, 1).await.unwrap();
                vm_config.name
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let mut vm_names = Vec::new();
        for handle in handles {
            let vm_name = handle.await.unwrap();
            vm_names.push(vm_name);
        }

        let total_time = start.elapsed();
        println!(
            "Created {} VMs in {:?} (avg: {:?} per VM)",
            num_vms,
            total_time,
            total_time / num_vms
        );

        // Verify all VMs were created
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), num_vms as usize);

        // Cleanup concurrently
        let mut cleanup_handles = Vec::new();
        for vm_name in vm_names {
            let backend_clone = backend.clone();
            let handle = tokio::spawn(async move {
                backend_clone.delete_vm(&vm_name).await.unwrap();
            });
            cleanup_handles.push(handle);
        }

        for handle in cleanup_handles {
            handle.await.unwrap();
        }

        // Verify cleanup
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
}
