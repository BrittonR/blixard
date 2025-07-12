use crate::types::*;
use blixard_core::error::{BlixardError, BlixardResult};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};

pub struct NixFlakeGenerator {
    /// Template directory path (used in tests and constructor but not in generation)
    #[allow(dead_code)]
    template_dir: PathBuf,
    modules_dir: PathBuf,
    tera: Tera,
}

impl NixFlakeGenerator {
    pub fn new(template_dir: PathBuf, modules_dir: PathBuf) -> BlixardResult<Self> {
        // Initialize Tera with the template directory
        let mut tera = Tera::default();

        // Load both standard and flake-parts templates
        let standard_template = include_str!("../nix/templates/vm-flake.nix");
        tera.add_raw_template("vm-flake.nix", standard_template)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to add standard template: {}", e),
            })?;

        let flake_parts_template = include_str!("../nix/templates/vm-flake-parts.nix");
        tera.add_raw_template("vm-flake-parts.nix", flake_parts_template)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to add flake-parts template: {}", e),
            })?;

        Ok(Self {
            template_dir,
            modules_dir,
            tera,
        })
    }

    pub fn generate_vm_flake(&self, config: &VmConfig) -> BlixardResult<String> {
        self.generate_vm_flake_with_template(config, "vm-flake.nix")
    }

    pub fn generate_vm_flake_parts(&self, config: &VmConfig) -> BlixardResult<String> {
        self.generate_vm_flake_with_template(config, "vm-flake-parts.nix")
    }

    fn generate_vm_flake_with_template(
        &self,
        config: &VmConfig,
        template_name: &str,
    ) -> BlixardResult<String> {
        let mut context = Context::new();

        // Basic configuration
        context.insert("vm_name", &config.name);
        context.insert("vm_index", &config.vm_index);
        context.insert("vm_index_hex", &format!("{:x}", config.vm_index));
        context.insert("vm_mac", &format!("02:00:00:00:{:02x}:01", config.vm_index));
        context.insert("system", "x86_64-linux");

        // Use absolute path for modules
        let modules_path = std::fs::canonicalize(&self.modules_dir)
            .unwrap_or_else(|_| self.modules_dir.clone())
            .to_string_lossy()
            .into_owned();
        context.insert("blixard_modules_path", &modules_path);
        context.insert("modules_path", &modules_path); // Keep for compatibility
        context.insert("hypervisor", &config.hypervisor.to_string());
        context.insert("vcpus", &config.vcpus);
        context.insert("memory", &config.memory);

        // Generate imports list
        let imports = self.generate_imports(config)?;
        context.insert("imports", &imports);

        // Add networks
        let networks: Vec<_> = config
            .networks
            .iter()
            .map(|net| match net {
                NetworkConfig::Tap { name, bridge, mac } => {
                    let mut network_obj = serde_json::Map::new();
                    network_obj.insert("type".to_string(), json!("tap"));
                    network_obj.insert("name".to_string(), json!(name));
                    if let Some(bridge) = bridge {
                        network_obj.insert("bridge".to_string(), json!(bridge));
                    }
                    if let Some(mac) = mac {
                        network_obj.insert("mac".to_string(), json!(mac));
                    }
                    json!(network_obj)
                }
                NetworkConfig::Routed {
                    id,
                    mac,
                    ip,
                    gateway,
                    subnet,
                } => {
                    let mut network_obj = serde_json::Map::new();
                    network_obj.insert("type".to_string(), json!("routed"));
                    network_obj.insert("id".to_string(), json!(id));
                    network_obj.insert("mac".to_string(), json!(mac));
                    network_obj.insert("ip".to_string(), json!(ip));
                    network_obj.insert("gateway".to_string(), json!(gateway));
                    network_obj.insert("subnet".to_string(), json!(subnet));
                    json!(network_obj)
                }
            })
            .collect();
        context.insert("networks", &networks);

        // Add volumes
        let volumes: Vec<_> = config
            .volumes
            .iter()
            .map(|vol| match vol {
                VolumeConfig::RootDisk { size } => {
                    json!({
                        "type": "rootDisk",
                        "size": size,
                    })
                }
                VolumeConfig::DataDisk {
                    path,
                    size,
                    read_only,
                } => {
                    json!({
                        "type": "dataDisk",
                        "path": path,
                        "size": size,
                        "readOnly": read_only,
                    })
                }
                VolumeConfig::Share {
                    tag,
                    source,
                    mount_point,
                } => {
                    json!({
                        "type": "virtiofs",
                        "tag": tag,
                        "path": source.to_string_lossy(),
                        "mountPoint": mount_point.to_string_lossy(),
                    })
                }
            })
            .collect();
        context.insert("volumes", &volumes);

        // Add init command if present
        if let Some(init_cmd) = &config.init_command {
            context.insert("init_command", init_cmd);
        }

        // Add SSH keys (for flake-parts template)
        let ssh_keys = vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILYzh3yIsSTOYXkJMFHBKzkakoDfonm3/RED5rqMqhIO britton@framework"];
        context.insert("ssh_keys", &ssh_keys);

        // Separate module types for flake-parts template
        let (flake_modules, file_modules, inline_modules) = self.categorize_modules(config)?;
        context.insert("flake_modules", &flake_modules);
        context.insert("file_modules", &file_modules);
        context.insert("inline_modules", &inline_modules);

        self.tera
            .render(template_name, &context)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to render flake template: {}", e),
            })
    }

    fn generate_imports(&self, config: &VmConfig) -> BlixardResult<Vec<String>> {
        let mut imports = vec![];

        // Add flake-parts module references
        for module in &config.flake_modules {
            imports.push(format!("inputs.blixard-modules.nixosModules.{}", module));
        }

        // Add file-based modules
        for module in &config.nixos_modules {
            match module {
                NixModule::File(path) => {
                    imports.push(format!("./{}", path.display()));
                }
                NixModule::FlakePart(name) => {
                    imports.push(format!("inputs.blixard-modules.nixosModules.{}", name));
                }
                NixModule::Inline(_) => {
                    // Inline modules will be handled separately in the template
                }
            }
        }

        Ok(imports)
    }

    fn categorize_modules(
        &self,
        config: &VmConfig,
    ) -> BlixardResult<(Vec<String>, Vec<String>, Vec<String>)> {
        let mut flake_modules = vec![];
        let mut file_modules = vec![];
        let mut inline_modules = vec![];

        // Add flake_modules field
        for module in &config.flake_modules {
            flake_modules.push(module.clone());
        }

        // Categorize nixos_modules
        for module in &config.nixos_modules {
            match module {
                NixModule::File(path) => {
                    file_modules.push(format!("./{}", path.display()));
                }
                NixModule::FlakePart(name) => {
                    flake_modules.push(name.clone());
                }
                NixModule::Inline(content) => {
                    inline_modules.push(content.clone());
                }
            }
        }

        Ok((flake_modules, file_modules, inline_modules))
    }

    /// Write a generated flake to a directory (standard template)
    pub fn write_flake(&self, config: &VmConfig, output_dir: &Path) -> BlixardResult<PathBuf> {
        self.write_flake_with_template(config, output_dir, false)
    }

    /// Write a generated flake to a directory (flake-parts template)
    pub fn write_flake_parts(
        &self,
        config: &VmConfig,
        output_dir: &Path,
    ) -> BlixardResult<PathBuf> {
        self.write_flake_with_template(config, output_dir, true)
    }

    fn write_flake_with_template(
        &self,
        config: &VmConfig,
        output_dir: &Path,
        use_flake_parts: bool,
    ) -> BlixardResult<PathBuf> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)?;

        // Generate the flake content
        let flake_content = if use_flake_parts {
            self.generate_vm_flake_parts(config)?
        } else {
            self.generate_vm_flake(config)?
        };

        // Write to flake.nix
        let flake_path = output_dir.join("flake.nix");
        fs::write(&flake_path, flake_content)?;

        // Note: flake.lock will be created automatically by Nix when building the VM
        // The systemd service uses --impure flag to handle untracked files

        Ok(flake_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_nix_generator_initialization() -> BlixardResult<()> {
        let temp_dir = TempDir::new().map_err(|e| BlixardError::IoError(e))?;
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        )?;

        assert_eq!(generator.template_dir, temp_dir.path());
        assert_eq!(generator.modules_dir, temp_dir.path().join("modules"));
        Ok(())
    }

    #[test]
    fn test_generate_imports() -> BlixardResult<()> {
        let generator =
            NixFlakeGenerator::new(PathBuf::from("templates"), PathBuf::from("modules"))?;

        let mut config = VmConfig::default();
        config.flake_modules = vec!["webserver".to_string(), "monitoring".to_string()];
        config.nixos_modules = vec![
            NixModule::File(PathBuf::from("custom.nix")),
            NixModule::FlakePart("database".to_string()),
        ];

        let imports = generator.generate_imports(&config)?;

        assert_eq!(imports.len(), 4);
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.webserver".to_string()));
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.monitoring".to_string()));
        assert!(imports.contains(&"./custom.nix".to_string()));
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.database".to_string()));
        Ok(())
    }

    #[test]
    fn test_flake_parts_generation() -> BlixardResult<()> {
        let temp_dir = TempDir::new().map_err(|e| BlixardError::IoError(e))?;
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        )?;

        let config = VmConfig {
            name: "test-vm".to_string(),
            vm_index: 42,
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 4,
            memory: 2048,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![
                NixModule::FlakePart("database".to_string()),
                NixModule::Inline("{ services.nginx.enable = true; }".to_string()),
            ],
            flake_modules: vec!["webserver".to_string(), "monitoring".to_string()],
            kernel: None,
            init_command: Some("/run/current-system/sw/bin/echo 'Hello World'".to_string()),
        };

        let flake = generator.generate_vm_flake_parts(&config)?;

        // Verify key elements of flake-parts template
        assert!(flake.contains("flake-parts.lib.mkFlake"));
        assert!(flake.contains("perSystem"));
        assert!(flake.contains("nixosConfigurations.\"test-vm\""));
        assert!(flake.contains("inputs.blixard-modules.nixosModules.webserver"));
        assert!(flake.contains("inputs.blixard-modules.nixosModules.monitoring"));
        assert!(flake.contains("inputs.blixard-modules.nixosModules.database"));
        assert!(flake.contains("{ services.nginx.enable = true; }"));
        assert!(flake.contains("Hello World"));
        assert!(flake.contains("vcpu = 4"));
        assert!(flake.contains("mem = 2048"));
        Ok(())
    }
}
