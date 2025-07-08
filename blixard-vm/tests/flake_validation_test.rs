#[cfg(test)]
mod tests {
    use blixard_vm::{types::*, NixFlakeGenerator};
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    #[ignore = "requires nix to be installed"]
    fn test_generated_flake_passes_nix_check() {
        let temp_dir = TempDir::new().unwrap();
        let modules_dir = temp_dir.path().join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();

        // Create a minimal flake-module.nix in modules directory
        std::fs::write(modules_dir.join("flake-module.nix"), r#"{ ... }: { }"#).unwrap();

        let generator =
            NixFlakeGenerator::new(temp_dir.path().to_path_buf(), modules_dir.clone()).unwrap();

        let config = VmConfig {
            name: "test-vm".to_string(),
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 1,
            memory: 512,
            ..Default::default()
        };

        // Generate and write the flake
        let flake_dir = temp_dir.path().join("vm-flake");
        let flake_path = generator.write_flake(&config, &flake_dir).unwrap();

        assert!(flake_path.exists());
        assert_eq!(flake_path.file_name().unwrap(), "flake.nix");

        // Run nix flake check
        let output = Command::new("nix")
            .args(&["flake", "check", "--no-build"])
            .current_dir(&flake_dir)
            .output()
            .expect("Failed to run nix flake check");

        if !output.status.success() {
            eprintln!("nix flake check failed:");
            eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            eprintln!("Generated flake content:");
            eprintln!("{}", std::fs::read_to_string(&flake_path).unwrap());
        }

        assert!(output.status.success(), "nix flake check should pass");
    }

    #[test]
    fn test_write_flake_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        )
        .unwrap();

        let config = VmConfig {
            name: "dir-test".to_string(),
            ..Default::default()
        };

        let output_dir = temp_dir.path().join("non-existent").join("nested");
        let flake_path = generator.write_flake(&config, &output_dir).unwrap();

        assert!(output_dir.exists());
        assert!(flake_path.exists());
        assert_eq!(flake_path, output_dir.join("flake.nix"));
    }
}
