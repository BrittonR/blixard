use crate::types::*;
use blixard_core::error::{BlixardResult, BlixardError};
use std::path::{Path, PathBuf};
use std::fs;
use tera::{Tera, Context};
use serde_json::json;

pub struct NixFlakeGenerator {
    template_dir: PathBuf,
    modules_dir: PathBuf,
    tera: Tera,
}

impl NixFlakeGenerator {
    pub fn new(template_dir: PathBuf, modules_dir: PathBuf) -> BlixardResult<Self> {
        // Initialize Tera with the template directory
        let mut tera = Tera::default();
        
        // For now, use inline template instead of loading from file
        // This ensures the test works without filesystem dependencies
        let template_content = include_str!("../nix/templates/vm-flake.nix");
        tera.add_raw_template("vm-flake.nix", template_content)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to add template: {}", e),
            })?;
        
        Ok(Self {
            template_dir,
            modules_dir,
            tera,
        })
    }
    
    pub fn generate_vm_flake(&self, config: &VmConfig) -> BlixardResult<String> {
        let mut context = Context::new();
        
        // Basic configuration
        context.insert("vm_name", &config.name);
        context.insert("system", "x86_64-linux");
        
        // Use absolute path for modules
        let modules_path = std::fs::canonicalize(&self.modules_dir)
            .unwrap_or_else(|_| self.modules_dir.clone())
            .to_string_lossy()
            .into_owned();
        context.insert("modules_path", &modules_path);
        context.insert("hypervisor", &config.hypervisor.to_string());
        context.insert("vcpus", &config.vcpus);
        context.insert("memory", &config.memory);
        
        // Generate imports list
        let imports = self.generate_imports(config)?;
        context.insert("imports", &imports);
        
        // Add networks
        let networks: Vec<_> = config.networks.iter().map(|net| {
            match net {
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
                },
                NetworkConfig::Routed { id, mac, ip, gateway, subnet } => {
                    let mut network_obj = serde_json::Map::new();
                    network_obj.insert("type".to_string(), json!("routed"));
                    network_obj.insert("id".to_string(), json!(id));
                    network_obj.insert("mac".to_string(), json!(mac));
                    network_obj.insert("ip".to_string(), json!(ip));
                    network_obj.insert("gateway".to_string(), json!(gateway));
                    network_obj.insert("subnet".to_string(), json!(subnet));
                    json!(network_obj)
                },
            }
        }).collect();
        context.insert("networks", &networks);
        
        // Add volumes
        let volumes: Vec<_> = config.volumes.iter().map(|vol| {
            match vol {
                VolumeConfig::RootDisk { size } => {
                    json!({
                        "type": "rootDisk",
                        "size": size,
                    })
                },
                VolumeConfig::DataDisk { path, size, read_only } => {
                    json!({
                        "type": "dataDisk",
                        "path": path,
                        "size": size,
                        "readOnly": read_only,
                    })
                },
                VolumeConfig::Share { tag, source, mount_point } => {
                    json!({
                        "type": "virtiofs",
                        "tag": tag,
                        "path": source.to_string_lossy(),
                        "mountPoint": mount_point.to_string_lossy(),
                    })
                },
            }
        }).collect();
        context.insert("volumes", &volumes);
        
        // Add init command if present
        if let Some(init_cmd) = &config.init_command {
            context.insert("init_command", init_cmd);
        }
        
        self.tera.render("vm-flake.nix", &context)
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
    
    /// Write a generated flake to a directory
    pub fn write_flake(&self, config: &VmConfig, output_dir: &Path) -> BlixardResult<PathBuf> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)?;
        
        // Generate the flake content
        let flake_content = self.generate_vm_flake(config)?;
        
        // Write to flake.nix
        let flake_path = output_dir.join("flake.nix");
        fs::write(&flake_path, flake_content)?;
        
        Ok(flake_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_nix_generator_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let generator = NixFlakeGenerator::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("modules"),
        ).unwrap();
        
        assert_eq!(generator.template_dir, temp_dir.path());
        assert_eq!(generator.modules_dir, temp_dir.path().join("modules"));
    }
    
    #[test]
    fn test_generate_imports() {
        let generator = NixFlakeGenerator::new(
            PathBuf::from("templates"),
            PathBuf::from("modules"),
        ).unwrap();
        
        let mut config = VmConfig::default();
        config.flake_modules = vec!["webserver".to_string(), "monitoring".to_string()];
        config.nixos_modules = vec![
            NixModule::File(PathBuf::from("custom.nix")),
            NixModule::FlakePart("database".to_string()),
        ];
        
        let imports = generator.generate_imports(&config).unwrap();
        
        assert_eq!(imports.len(), 4);
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.webserver".to_string()));
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.monitoring".to_string()));
        assert!(imports.contains(&"./custom.nix".to_string()));
        assert!(imports.contains(&"inputs.blixard-modules.nixosModules.database".to_string()));
    }
}