#[cfg(test)]
mod tests {
    use blixard_vm::types::*;
    use blixard_vm::{BlixardResult, NixFlakeGenerator};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_basic_vm_flake_generation() {
        let temp_dir = TempDir::new().unwrap();
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        ).unwrap();
        
        let config = VmConfig {
            name: "test-vm".to_string(),
            vm_index: 5,
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 2,
            memory: 1024,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            flake_modules: vec![],
            kernel: None,
            init_command: None,
        };
        
        let flake = generator.generate_vm_flake(&config).unwrap();
        
        // Verify the generated flake contains expected content
        assert!(flake.contains(r#""test-vm""#), "Flake should contain VM name");
        assert!(flake.contains("cloud-hypervisor"));
        assert!(flake.contains("vcpu = 2"));
        assert!(flake.contains("mem = 1024"));
        assert!(flake.contains("microvm.nixosModules.microvm"));
        assert!(flake.contains("nixosConfigurations"));
    }
    
    #[test]
    fn test_vm_flake_with_modules() {
        let temp_dir = TempDir::new().unwrap();
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        ).unwrap();
        
        let config = VmConfig {
            name: "modular-vm".to_string(),
            vm_index: 10,
            hypervisor: Hypervisor::Firecracker,
            vcpus: 4,
            memory: 2048,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            flake_modules: vec!["webserver".to_string()],
            kernel: None,
            init_command: None,
        };
        
        let flake = generator.generate_vm_flake(&config).unwrap();
        
        // Basic assertions for now
        assert!(flake.contains("modular-vm"));
        assert!(flake.contains("firecracker"));
        assert!(flake.contains("vcpu = 4"));
        assert!(flake.contains("mem = 2048"));
    }
    
    #[test]
    fn test_minimal_vm_config() {
        let config = VmConfig {
            name: "minimal".to_string(),
            ..Default::default()
        };
        
        assert_eq!(config.hypervisor, Hypervisor::CloudHypervisor);
        assert_eq!(config.vcpus, 1);
        assert_eq!(config.memory, 512);
        assert!(config.networks.is_empty());
        assert!(config.volumes.is_empty());
    }
}