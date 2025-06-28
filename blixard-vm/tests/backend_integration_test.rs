#[cfg(test)]
mod tests {
    use blixard_vm::MicrovmBackend;
    use blixard_core::{
        vm_backend::VmBackend,
        types::{VmConfig, VmStatus},
    };
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_microvm_backend_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir, data_dir).unwrap();
        
        // Create a VM
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 2,
            memory: 1024,
            tenant_id: "test-tenant".to_string(),
            ip_address: Some("10.0.0.10".to_string()),
        };
        
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        // List VMs
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].0.name, "test-vm");
        assert_eq!(vms[0].1, VmStatus::Stopped);
        
        // Get status - VM exists in config but process manager returns None since it was never started
        let status = backend.get_vm_status("test-vm").await.unwrap();
        assert_eq!(status, None);
        
        // Delete VM
        backend.delete_vm("test-vm").await.unwrap();
        
        // Verify it's gone
        let vms = backend.list_vms().await.unwrap();
        assert_eq!(vms.len(), 0);
    }
    
    #[tokio::test]
    async fn test_microvm_backend_creates_flake() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        
        let backend = MicrovmBackend::new(config_dir.clone(), data_dir).unwrap();
        
        let vm_config = VmConfig {
            name: "flake-test".to_string(),
            config_path: "".to_string(),
            vcpus: 1,
            memory: 512,
            tenant_id: "test-tenant".to_string(),
            ip_address: Some("10.0.0.11".to_string()),
        };
        
        backend.create_vm(&vm_config, 1).await.unwrap();
        
        // Check that flake was created
        let flake_path = config_dir.join("vms").join("flake-test").join("flake.nix");
        assert!(flake_path.exists());
        
        // Read and verify flake content
        let flake_content = std::fs::read_to_string(flake_path).unwrap();
        assert!(flake_content.contains("flake-test"));
        assert!(flake_content.contains("nixosConfigurations"));
        assert!(flake_content.contains("microvm.nixosModules.microvm"));
        assert!(flake_content.contains("qemu"));  // Default hypervisor from backend
    }
}