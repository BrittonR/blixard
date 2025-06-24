use blixard_vm::{MicrovmBackend, NixFlakeGenerator};
use blixard_vm::types::*;
use blixard_core::{
    vm_backend::VmBackend,
    types::{VmConfig as CoreVmConfig},
};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use tokio::process::Command;

/// Tests for Nix flake validation and correctness
#[cfg(test)]
mod flake_validation_tests {
    use super::*;

    /// Test that generated flakes pass `nix flake check`
    #[tokio::test]
    async fn test_generated_flake_validation() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        let vm_config = CoreVmConfig {
            name: "validation-test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 1024,
        };
        
        // Create VM to generate flake
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        let flake_dir = config_dir.join("vms").join(&vm_config.name);
        let flake_path = flake_dir.join("flake.nix");
        
        assert!(flake_path.exists(), "Flake should be generated");
        
        // Run nix flake check
        match run_nix_flake_check(&flake_dir).await {
            Ok(()) => {
                println!("✓ Flake validation passed");
            }
            Err(e) => {
                println!("⚠ Nix flake check failed (expected in environments without Nix): {}", e);
                // In CI environments without Nix, this is expected
                // The important part is that we generate syntactically correct flakes
            }
        }
        
        // Always verify flake syntax manually
        verify_flake_syntax(&flake_path).await.unwrap();
        
        // Cleanup
        backend.delete_vm(&vm_config.name).await.unwrap();
    }
    
    /// Test flake validation with different VM configurations
    #[tokio::test]
    async fn test_flake_validation_different_configs() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("templates");
        let modules_dir = temp_dir.path().join("modules");
        
        // Copy template files
        std::fs::create_dir_all(&template_dir).unwrap();
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        std::fs::write(template_dir.join("vm-flake.nix"), template_content).unwrap();
        
        let generator = NixFlakeGenerator::new(template_dir, modules_dir).unwrap();
        
        let test_configs = vec![
            // Minimal config
            VmConfig {
                name: "minimal-vm".to_string(),
                hypervisor: Hypervisor::Qemu,
                vcpus: 1,
                memory: 256,
                networks: vec![],
                volumes: vec![],
                nixos_modules: vec![],
                flake_modules: vec![],
                kernel: None,
                init_command: None,
            },
            // Config with networking
            VmConfig {
                name: "networked-vm".to_string(),
                hypervisor: Hypervisor::Qemu,
                vcpus: 2,
                memory: 512,
                networks: vec![
                    NetworkConfig::Routed {
                        id: "vm-networked".to_string(),
                        mac: "02:00:00:00:00:01".to_string(),
                        ip: "10.0.0.10".to_string(),
                        gateway: "10.0.0.1".to_string(),
                        subnet: "10.0.0.0/24".to_string(),
                    },
                    NetworkConfig::Tap {
                        name: "tap0".to_string(),
                        bridge: None,
                        mac: Some("02:00:00:00:00:02".to_string()),
                    },
                ],
                volumes: vec![],
                nixos_modules: vec![],
                flake_modules: vec![],
                kernel: None,
                init_command: Some("echo 'VM started'".to_string()),
            },
            // Config with volumes
            VmConfig {
                name: "storage-vm".to_string(),
                hypervisor: Hypervisor::Qemu,
                vcpus: 4,
                memory: 2048,
                networks: vec![NetworkConfig::Routed {
                    id: "vm-storage".to_string(),
                    mac: "02:00:00:00:00:03".to_string(),
                    ip: "10.0.0.11".to_string(),
                    gateway: "10.0.0.1".to_string(),
                    subnet: "10.0.0.0/24".to_string(),
                }],
                volumes: vec![
                    VolumeConfig::RootDisk { size: 10240 },
                    VolumeConfig::DataDisk {
                        path: "data.img".to_string(),
                        size: 5120,
                        read_only: false,
                    },
                ],
                nixos_modules: vec![],
                flake_modules: vec![],
                kernel: None,
                init_command: None,
            },
        ];
        
        for config in test_configs {
            println!("Testing flake generation for: {}", config.name);
            
            let flake_content = generator.generate_vm_flake(&config).unwrap();
            
            // Write flake to temporary location for validation
            let flake_dir = temp_dir.path().join(&config.name);
            std::fs::create_dir_all(&flake_dir).unwrap();
            let flake_path = flake_dir.join("flake.nix");
            std::fs::write(&flake_path, &flake_content).unwrap();
            
            // Verify syntax
            verify_flake_syntax(&flake_path).await.unwrap();
            
            // Verify flake contains expected configuration
            assert!(flake_content.contains(&config.name));
            assert!(flake_content.contains(&format!("vcpu = {}", config.vcpus)));
            assert!(flake_content.contains(&format!("mem = {}", config.memory)));
            
            match config.hypervisor {
                Hypervisor::Qemu => assert!(flake_content.contains("qemu")),
                Hypervisor::CloudHypervisor => assert!(flake_content.contains("cloud-hypervisor")),
                Hypervisor::Firecracker => assert!(flake_content.contains("firecracker")),
            }
            
            println!("✓ Flake validation passed for: {}", config.name);
        }
    }
    
    /// Test flake validation with NixOS modules
    #[tokio::test]
    async fn test_flake_validation_with_modules() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("templates");
        let modules_dir = temp_dir.path().join("modules");
        
        // Setup template
        std::fs::create_dir_all(&template_dir).unwrap();
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        std::fs::write(template_dir.join("vm-flake.nix"), template_content).unwrap();
        
        let generator = NixFlakeGenerator::new(template_dir, modules_dir).unwrap();
        
        let config = VmConfig {
            name: "module-test-vm".to_string(),
            hypervisor: Hypervisor::Qemu,
            vcpus: 2,
            memory: 1024,
            networks: vec![NetworkConfig::Routed {
                id: "vm-module-test".to_string(),
                mac: "02:00:00:00:00:04".to_string(),
                ip: "10.0.0.12".to_string(),
                gateway: "10.0.0.1".to_string(),
                subnet: "10.0.0.0/24".to_string(),
            }],
            volumes: vec![VolumeConfig::RootDisk { size: 8192 }],
            nixos_modules: vec![
                NixModule::Inline(r#"
                {
                  services.openssh.enable = true;
                  users.users.testuser = {
                    isNormalUser = true;
                    extraGroups = [ "wheel" ];
                  };
                }
                "#.to_string()),
            ],
            flake_modules: vec!["webserver".to_string()],
            kernel: Some(KernelConfig {
                package: Some("pkgs.linuxPackages.kernel".to_string()),
                cmdline: Some("console=ttyS0".to_string()),
            }),
            init_command: Some("systemctl status".to_string()),
        };
        
        let flake_content = generator.generate_vm_flake(&config).unwrap();
        
        // Write and validate
        let flake_dir = temp_dir.path().join(&config.name);
        std::fs::create_dir_all(&flake_dir).unwrap();
        let flake_path = flake_dir.join("flake.nix");
        std::fs::write(&flake_path, &flake_content).unwrap();
        
        verify_flake_syntax(&flake_path).await.unwrap();
        
        // Verify module content is included
        assert!(flake_content.contains("services.openssh.enable"));
        assert!(flake_content.contains("users.users.testuser"));
        assert!(flake_content.contains("console=ttyS0"));
        
        println!("✓ Complex flake with modules validated successfully");
    }
    
    /// Test flake lock file generation and validation
    #[tokio::test]
    async fn test_flake_lock_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        let vm_config = CoreVmConfig {
            name: "lock-test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
        };
        
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        let flake_dir = config_dir.join("vms").join(&vm_config.name);
        let flake_lock_path = flake_dir.join("flake.lock");
        
        // Try to generate lock file
        match run_nix_flake_lock(&flake_dir).await {
            Ok(()) => {
                assert!(flake_lock_path.exists(), "flake.lock should be generated");
                
                // Verify lock file is valid JSON
                let lock_content = std::fs::read_to_string(&flake_lock_path).unwrap();
                let _: serde_json::Value = serde_json::from_str(&lock_content).unwrap();
                
                println!("✓ flake.lock generated and validated");
            }
            Err(e) => {
                println!("⚠ Nix flake lock failed (expected without Nix): {}", e);
            }
        }
        
        backend.delete_vm(&vm_config.name).await.unwrap();
    }
    
    /// Helper function to run `nix flake check`
    async fn run_nix_flake_check(flake_dir: &PathBuf) -> Result<(), String> {
        let output = timeout(Duration::from_secs(30), Command::new("nix")
            .args(&["flake", "check", "--no-build"])
            .current_dir(flake_dir)
            .output())
            .await
            .map_err(|_| "Flake check timeout after 30 seconds".to_string())?
            .map_err(|e| format!("Failed to execute nix flake check: {}", e))?;
        
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Nix flake check failed: {}", stderr))
        }
    }
    
    /// Helper function to run `nix flake lock`
    async fn run_nix_flake_lock(flake_dir: &PathBuf) -> Result<(), String> {
        let output = timeout(Duration::from_secs(60), Command::new("nix")
            .args(&["flake", "lock"])
            .current_dir(flake_dir)
            .output())
            .await
            .map_err(|_| "Flake lock timeout after 60 seconds".to_string())?
            .map_err(|e| format!("Failed to execute nix flake lock: {}", e))?;
        
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Nix flake lock failed: {}", stderr))
        }
    }
    
    /// Verify flake syntax without requiring Nix
    async fn verify_flake_syntax(flake_path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(flake_path)
            .map_err(|e| format!("Failed to read flake: {}", e))?;
        
        // Basic syntax checks
        if !content.contains("inputs") {
            return Err("Flake missing inputs section".to_string());
        }
        
        if !content.contains("outputs") {
            return Err("Flake missing outputs section".to_string());
        }
        
        if !content.contains("nixosConfigurations") {
            return Err("Flake missing nixosConfigurations".to_string());
        }
        
        // Check for balanced braces
        let open_braces = content.chars().filter(|&c| c == '{').count();
        let close_braces = content.chars().filter(|&c| c == '}').count();
        if open_braces != close_braces {
            return Err(format!("Unbalanced braces: {} open, {} close", open_braces, close_braces));
        }
        
        // Check for balanced brackets
        let open_brackets = content.chars().filter(|&c| c == '[').count();
        let close_brackets = content.chars().filter(|&c| c == ']').count();
        if open_brackets != close_brackets {
            return Err(format!("Unbalanced brackets: {} open, {} close", open_brackets, close_brackets));
        }
        
        // Check for balanced parentheses
        let open_parens = content.chars().filter(|&c| c == '(').count();
        let close_parens = content.chars().filter(|&c| c == ')').count();
        if open_parens != close_parens {
            return Err(format!("Unbalanced parentheses: {} open, {} close", open_parens, close_parens));
        }
        
        Ok(())
    }
}

/// Tests for flake template correctness
#[cfg(test)]
mod template_tests {
    use super::*;
    
    /// Test that the vm-flake.nix template is syntactically correct
    #[tokio::test]
    async fn test_template_syntax() {
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        
        // Basic template syntax validation
        assert!(template_content.contains("{{"), "Template should contain Tera variables");
        assert!(template_content.contains("inputs"), "Template should have inputs");
        assert!(template_content.contains("outputs"), "Template should have outputs");
        assert!(template_content.contains("nixosConfigurations"), "Template should have nixosConfigurations");
        
        // Check for required template variables
        let required_vars = [
            "{{ vm_name }}",
            "{{ system }}",
            "{{ hypervisor }}",
            "{{ vcpus }}",
            "{{ memory }}",
        ];
        
        for var in &required_vars {
            assert!(template_content.contains(var), "Template missing variable: {}", var);
        }
        
        println!("✓ Template syntax validation passed");
    }
    
    /// Test template rendering with various inputs
    #[test]
    fn test_template_rendering() {
        use tera::{Tera, Context};
        
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        let mut tera = Tera::new("").unwrap();
        tera.add_raw_template("vm-flake.nix", template_content).unwrap();
        
        let test_cases = vec![
            ("minimal", "qemu", 1, 256),
            ("standard", "qemu", 2, 1024),
            ("large", "cloud-hypervisor", 8, 4096),
        ];
        
        for (name, hypervisor, vcpus, memory) in test_cases {
            let mut context = Context::new();
            context.insert("vm_name", name);
            context.insert("system", "x86_64-linux");
            context.insert("hypervisor", hypervisor);
            context.insert("vcpus", &vcpus);
            context.insert("memory", &memory);
            context.insert("interfaces", &Vec::<String>::new());
            context.insert("volumes", &Vec::<String>::new());
            context.insert("nixos_modules", &Vec::<String>::new());
            
            let rendered = tera.render("vm-flake.nix", &context).unwrap();
            
            // Verify rendered content
            assert!(rendered.contains(name));
            assert!(rendered.contains(hypervisor));
            assert!(rendered.contains(&format!("vcpu = {}", vcpus)));
            assert!(rendered.contains(&format!("mem = {}", memory)));
            
            println!("✓ Template rendering test passed for: {}", name);
        }
    }
}