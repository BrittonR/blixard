use blixard_vm::MicrovmBackend;
use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig, VmStatus},
};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Performance benchmarks for VM operations
#[cfg(test)]
mod performance_benchmarks {
    use super::*;

    /// Benchmark VM creation performance
    #[tokio::test]
    async fn benchmark_vm_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        let vm_config = VmConfig {
            name: "benchmark-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
        };
        
        // Warm up
        backend.create_vm(&vm_config, 1).await.unwrap();
        backend.delete_vm(&vm_config.name).await.unwrap();
        
        // Benchmark multiple creations
        let iterations = 10;
        let start = Instant::now();
        
        for i in 0..iterations {
            let vm_name = format!("benchmark-vm-{}", i);
            let config = VmConfig {
                name: vm_name.clone(),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 512,
            };
            
            backend.create_vm(&config, 1).await.unwrap();
        }
        
        let creation_time = start.elapsed();
        let avg_time = creation_time / iterations;
        
        println!("Created {} VMs in {:?}", iterations, creation_time);
        println!("Average creation time: {:?}", avg_time);
        
        // Performance expectation: Should be under 100ms per VM on average
        assert!(avg_time < Duration::from_millis(100), 
            "VM creation too slow: {:?} per VM", avg_time);
        
        // Cleanup
        for i in 0..iterations {
            let vm_name = format!("benchmark-vm-{}", i);
            backend.delete_vm(&vm_name).await.unwrap();
        }
    }
    
    /// Benchmark concurrent VM operations
    #[tokio::test]
    async fn benchmark_concurrent_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = std::sync::Arc::new(
            MicrovmBackend::new(config_dir, data_dir).unwrap()
        );
        
        let concurrent_vms = 5;
        let start = Instant::now();
        
        // Launch concurrent VM creation
        let mut handles = Vec::new();
        for i in 0..concurrent_vms {
            let backend_clone = backend.clone();
            let handle = tokio::spawn(async move {
                let vm_config = VmConfig {
                    name: format!("concurrent-vm-{}", i),
                    config_path: "".to_string(),
                    vcpus: 1,
                    memory: 256,
                };
                
                let create_start = Instant::now();
                backend_clone.create_vm(&vm_config, 1).await.unwrap();
                let create_time = create_start.elapsed();
                
                (i, vm_config.name, create_time)
            });
            handles.push(handle);
        }
        
        // Wait for all operations
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await.unwrap();
            results.push(result);
        }
        
        let total_time = start.elapsed();
        
        println!("Concurrent creation of {} VMs took: {:?}", concurrent_vms, total_time);
        
        for (i, vm_name, create_time) in &results {
            println!("VM {}: {} created in {:?}", i, vm_name, create_time);
        }
        
        // Concurrent operations should be faster than sequential
        let avg_concurrent_time = total_time / concurrent_vms;
        println!("Average concurrent creation time: {:?}", avg_concurrent_time);
        
        // Verify all VMs were created
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), concurrent_vms as usize);
        
        // Cleanup concurrently too
        let cleanup_start = Instant::now();
        let mut cleanup_handles = Vec::new();
        
        for (_, vm_name, _) in results {
            let backend_clone = backend.clone();
            let handle = tokio::spawn(async move {
                backend_clone.delete_vm(&vm_name).await.unwrap();
            });
            cleanup_handles.push(handle);
        }
        
        for handle in cleanup_handles {
            handle.await.unwrap();
        }
        
        let cleanup_time = cleanup_start.elapsed();
        println!("Concurrent cleanup took: {:?}", cleanup_time);
        
        // Verify cleanup
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
    
    /// Benchmark VM listing performance with many VMs
    #[tokio::test]
    async fn benchmark_vm_listing() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        // Create multiple VMs
        let vm_count = 20;
        for i in 0..vm_count {
            let vm_config = VmConfig {
                name: format!("list-test-vm-{}", i),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 256,
            };
            backend.create_vm(&vm_config, 1).await.unwrap();
        }
        
        // Benchmark listing operations
        let iterations = 10;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let vms = backend.list_vms().await.unwrap();
            assert_eq!(vms.len(), vm_count as usize);
        }
        
        let list_time = start.elapsed();
        let avg_list_time = list_time / iterations;
        
        println!("Listed {} VMs {} times in {:?}", vm_count, iterations, list_time);
        println!("Average list time: {:?}", avg_list_time);
        
        // Listing should be fast even with many VMs
        assert!(avg_list_time < Duration::from_millis(50), 
            "VM listing too slow: {:?}", avg_list_time);
        
        // Cleanup
        for i in 0..vm_count {
            let vm_name = format!("list-test-vm-{}", i);
            backend.delete_vm(&vm_name).await.unwrap();
        }
    }
    
    /// Benchmark memory usage with many VMs
    #[tokio::test]
    async fn benchmark_memory_usage() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        // Create VMs incrementally and measure
        let max_vms = 50;
        let mut creation_times = Vec::new();
        
        for i in 0..max_vms {
            let vm_config = VmConfig {
                name: format!("memory-test-vm-{}", i),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 256,
            };
            
            let start = Instant::now();
            backend.create_vm(&vm_config, 1).await.unwrap();
            let create_time = start.elapsed();
            creation_times.push(create_time);
            
            // Check if creation time is degrading significantly
            if i > 0 && i % 10 == 0 {
                let recent_avg: Duration = creation_times[i-10..].iter().sum::<Duration>() / 10;
                let early_avg: Duration = creation_times[0..10.min(i)].iter().sum::<Duration>() / 10.min(i) as u32;
                
                println!("After {} VMs: recent avg {:?}, early avg {:?}", i + 1, recent_avg, early_avg);
                
                // Performance shouldn't degrade more than 3x
                assert!(recent_avg < early_avg * 3, 
                    "Significant performance degradation detected at {} VMs", i + 1);
            }
        }
        
        println!("Successfully created {} VMs with stable performance", max_vms);
        
        // Final verification
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), max_vms as usize);
        
        // Cleanup
        for i in 0..max_vms {
            let vm_name = format!("memory-test-vm-{}", i);
            backend.delete_vm(&vm_name).await.unwrap();
        }
    }
    
    /// Benchmark flake generation performance
    #[tokio::test] 
    async fn benchmark_flake_generation() {
        use blixard_vm::NixFlakeGenerator;
        use tempfile::TempDir;
        
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("templates");
        let modules_dir = temp_dir.path().join("modules");
        
        // Setup template
        std::fs::create_dir_all(&template_dir).unwrap();
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        std::fs::write(template_dir.join("vm-flake.nix"), template_content).unwrap();
        
        let generator = NixFlakeGenerator::new(template_dir, modules_dir).unwrap();
        
        // Benchmark flake generation
        let iterations = 100;
        let start = Instant::now();
        
        for i in 0..iterations {
            let config = blixard_vm::types::VmConfig {
                name: format!("flake-bench-{}", i),
                hypervisor: blixard_vm::types::Hypervisor::Qemu,
                vcpus: (i % 4) + 1,
                memory: ((i % 4) + 1) * 512,
                networks: vec![blixard_vm::types::NetworkConfig::User { ssh_port: Some(2225 + i as u16) }],
                volumes: vec![blixard_vm::types::VolumeConfig::RootDisk { size: 8192 }],
                nixos_modules: vec![],
                flake_modules: vec![],
                kernel: None,
                init_command: None,
            };
            
            let _flake = generator.generate_vm_flake(&config).unwrap();
        }
        
        let generation_time = start.elapsed();
        let avg_time = generation_time / iterations;
        
        println!("Generated {} flakes in {:?}", iterations, generation_time);
        println!("Average generation time: {:?}", avg_time);
        
        // Flake generation should be very fast
        assert!(avg_time < Duration::from_millis(10), 
            "Flake generation too slow: {:?}", avg_time);
    }
}

/// Stress tests for extreme scenarios
#[cfg(test)]
mod stress_tests {
    use super::*;
    
    /// Stress test with rapid creation/deletion cycles
    #[tokio::test]
    async fn stress_rapid_cycles() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        let cycles = 20;
        let start = Instant::now();
        
        for i in 0..cycles {
            let vm_config = VmConfig {
                name: format!("cycle-vm-{}", i),
                config_path: "".to_string(),
                vcpus: 1,
                memory: 256,
            };
            
            // Create and immediately delete
            backend.create_vm(&vm_config, 1).await.unwrap();
            
            // Verify it exists
            let vms = backend.list_vms().await.unwrap();
            assert!(vms.iter().any(|(vm, _)| vm.name == vm_config.name));
            
            // Delete it
            backend.delete_vm(&vm_config.name).await.unwrap();
            
            // Verify it's gone
            let vms = backend.list_vms().await.unwrap();
            assert!(!vms.iter().any(|(vm, _)| vm.name == vm_config.name));
        }
        
        let total_time = start.elapsed();
        let avg_cycle_time = total_time / cycles;
        
        println!("Completed {} rapid cycles in {:?}", cycles, total_time);
        println!("Average cycle time: {:?}", avg_cycle_time);
        
        // Rapid cycles should complete reasonably fast
        assert!(avg_cycle_time < Duration::from_millis(200), 
            "Rapid cycles too slow: {:?}", avg_cycle_time);
        
        // Final verification - no VMs should remain
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
}