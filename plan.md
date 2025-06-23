# MicroVM.nix Integration Plan with Flake-Parts

## Overview

This document outlines the implementation plan for integrating microvm.nix control into Blixard using flake-parts for modular, composable VM configurations. The plan focuses on atomic testing, proper abstractions, and leveraging Nix's strengths while maintaining testability.

## Goals

1. **Full microvm.nix integration** - Support for cloud-hypervisor and firecracker
2. **Modular configuration** - Use flake-parts for composable VM definitions
3. **Atomic testing** - Each component testable in isolation
4. **Deterministic behavior** - Reproducible VM configurations and tests
5. **Production readiness** - Proper error handling, monitoring, and recovery

## Architecture

```
┌─────────────────────────────────────────┐
│           BlixardOrchestrator           │
├─────────────────────────────────────────┤
│  • CLI Command Processing              │
│  • Configuration Management            │
│  • Component Lifecycle                 │
└─────────────────────────────────────────┘
         │                      │
         ▼                      ▼
┌─────────────────┐    ┌─────────────────┐
│  blixard-core   │    │   blixard-vm    │
│                 │    │                 │
│ • Raft consensus │    │ • VM lifecycle  │
│ • gRPC server   │    │ • microvm.nix   │
│ • Storage       │    │ • Flake-parts   │
│ • Peer mgmt     │    │ • Monitoring    │
└─────────────────┘    └─────────────────┘
```

## Phase 1: Flake-Parts Infrastructure

### 1.1 Create Flake-Parts Module System

Create a custom flake-parts module system for microvm configurations that provides:
- Reusable modules for common VM patterns
- Type-safe configuration options
- Easy composition and override capabilities

**Module Structure:**
```nix
# blixard-vm/nix/modules/microvm.nix
{ lib, config, ... }: {
  options.blixard.vms = lib.mkOption {
    type = lib.types.attrsOf (lib.types.submodule {
      options = {
        hypervisor = lib.mkOption {
          type = lib.types.enum [ "cloud-hypervisor" "firecracker" ];
          default = "cloud-hypervisor";
          description = "Hypervisor backend to use";
        };
        vcpus = lib.mkOption {
          type = lib.types.int;
          default = 1;
          description = "Number of virtual CPUs";
        };
        memory = lib.mkOption {
          type = lib.types.int;
          default = 512;
          description = "Memory in MB";
        };
        networks = lib.mkOption {
          type = lib.types.listOf networkType;
          default = [];
          description = "Network interfaces";
        };
        volumes = lib.mkOption {
          type = lib.types.listOf volumeType;
          default = [];
          description = "Storage volumes";
        };
        nixosModules = lib.mkOption {
          type = lib.types.listOf lib.types.deferredModule;
          default = [];
          description = "NixOS configuration modules";
        };
      };
    });
  };
}
```

### 1.2 Module Library

Create a library of reusable flake-parts modules:

```
blixard-vm/nix/modules/
├── core/
│   ├── microvm.nix         # Base microvm options
│   ├── networking.nix      # Network configuration types
│   └── storage.nix         # Storage configuration types
├── profiles/
│   ├── webserver.nix       # Nginx/Apache configurations
│   ├── database.nix        # PostgreSQL/MySQL setups
│   ├── container.nix       # Docker/Podman runtime
│   └── monitoring.nix      # Prometheus/Grafana stack
└── flake-module.nix        # Main module entry point
```

### 1.3 Flake Generation Templates

Create templates for generating flake.nix files:

```nix
# Template for generated VM flakes
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    blixard-modules = {
      url = "path:@modulesPath@";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.microvm.flakeModule
        inputs.blixard-modules.flakeModule
      ];
      
      systems = [ "@system@" ];
      
      perSystem = { config, pkgs, ... }: {
        microvm.vms = {
          "@vmName@" = {
            inherit pkgs;
            imports = [ @imports@ ];
            
            config = @vmConfig@;
          };
        };
      };
    };
}
```

## Phase 2: Rust Integration

### 2.1 Enhanced Type System

Update the VM configuration types to support flake-parts:

```rust
// blixard-vm/src/types.rs
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub hypervisor: Hypervisor,
    pub vcpus: u32,
    pub memory: u32, // MB
    pub networks: Vec<NetworkConfig>,
    pub volumes: Vec<VolumeConfig>,
    pub nixos_modules: Vec<NixModule>,
    pub flake_modules: Vec<String>, // References to reusable modules
    pub kernel: Option<KernelConfig>,
    pub init_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hypervisor {
    CloudHypervisor,
    Firecracker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkConfig {
    Tap {
        name: String,
        bridge: Option<String>,
        mac: Option<String>,
    },
    User, // QEMU user networking (not for production)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeConfig {
    RootDisk { size: u64 }, // MB
    DataDisk { 
        path: String,
        size: u64,
        read_only: bool,
    },
    Share {
        tag: String,
        source: PathBuf,
        mount_point: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NixModule {
    Inline(String),
    File(PathBuf),
    FlakePart(String), // Reference to a flake-parts module
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    pub package: Option<String>,
    pub cmdline: Option<String>,
}
```

### 2.2 Nix Flake Generator

Implement the flake generation logic:

```rust
// blixard-vm/src/nix_generator.rs
use crate::types::*;
use crate::error::{BlixardResult, BlixardError};
use std::path::{Path, PathBuf};
use tera::{Tera, Context};

pub struct NixFlakeGenerator {
    template_dir: PathBuf,
    modules_dir: PathBuf,
    tera: Tera,
}

impl NixFlakeGenerator {
    pub fn new(template_dir: PathBuf, modules_dir: PathBuf) -> BlixardResult<Self> {
        let tera = Tera::new(template_dir.join("*.nix").to_str().unwrap())
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to load templates: {}", e),
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
        context.insert("vmName", &config.name);
        context.insert("system", "x86_64-linux");
        context.insert("modulesPath", &self.modules_dir.to_string_lossy());
        
        // Generate imports list
        let imports = self.generate_imports(config)?;
        context.insert("imports", &imports);
        
        // Generate VM configuration
        let vm_config = self.generate_vm_config(config)?;
        context.insert("vmConfig", &vm_config);
        
        self.tera.render("vm-flake.nix", &context)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to render flake template: {}", e),
            })
    }
    
    fn generate_imports(&self, config: &VmConfig) -> BlixardResult<String> {
        let mut imports = vec![];
        
        // Add flake-parts module references
        for module in &config.flake_modules {
            imports.push(format!("inputs.blixard-modules.{}", module));
        }
        
        // Add file-based modules
        for module in &config.nixos_modules {
            match module {
                NixModule::File(path) => {
                    imports.push(format!("./{}", path.display()));
                }
                NixModule::FlakePart(name) => {
                    imports.push(format!("inputs.blixard-modules.{}", name));
                }
                _ => {} // Inline modules handled separately
            }
        }
        
        Ok(imports.join(" "))
    }
    
    fn generate_vm_config(&self, config: &VmConfig) -> BlixardResult<String> {
        // Generate Nix expression for VM configuration
        // This would use a Nix builder library or template system
        todo!("Implement Nix config generation")
    }
}
```

### 2.3 Process Management

Implement proper VM process lifecycle management:

```rust
// blixard-vm/src/process_manager.rs
use tokio::process::{Command, Child};
use std::process::Stdio;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct VmProcessManager {
    processes: Arc<RwLock<HashMap<String, VmProcess>>>,
    runtime_dir: PathBuf,
}

pub struct VmProcess {
    name: String,
    child: Child,
    pid: u32,
    started_at: SystemTime,
    hypervisor: Hypervisor,
}

impl VmProcessManager {
    pub async fn start_vm(&self, name: &str, flake_path: &Path) -> BlixardResult<()> {
        // Build the VM runner
        let runner_path = self.build_vm_runner(name, flake_path).await?;
        
        // Start the VM process
        let child = Command::new(&runner_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BlixardError::VmOperationFailed {
                operation: "start".to_string(),
                details: format!("Failed to spawn VM process: {}", e),
            })?;
        
        let pid = child.id().ok_or_else(|| BlixardError::Internal {
            message: "Failed to get VM process ID".to_string(),
        })?;
        
        // Track the process
        let vm_process = VmProcess {
            name: name.to_string(),
            child,
            pid,
            started_at: SystemTime::now(),
            hypervisor: self.detect_hypervisor(&runner_path).await?,
        };
        
        self.processes.write().await.insert(name.to_string(), vm_process);
        
        Ok(())
    }
    
    async fn build_vm_runner(&self, name: &str, flake_path: &Path) -> BlixardResult<PathBuf> {
        let output = Command::new("nix")
            .args(&[
                "build",
                &format!("{}#nixosConfigurations.{}.config.microvm.runner", 
                    flake_path.display(), name),
                "--out-link",
                &self.runtime_dir.join(format!("{}-runner", name)).to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to build VM runner: {}", e),
            })?;
        
        if !output.status.success() {
            return Err(BlixardError::VmOperationFailed {
                operation: "build".to_string(),
                details: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
        
        Ok(self.runtime_dir.join(format!("{}-runner", name)))
    }
}
```

## Phase 3: Testing Infrastructure

### 3.1 Unit Tests

Test each component in isolation:

```rust
// blixard-vm/tests/nix_generation_tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_basic_vm_flake_generation() {
        let temp_dir = TempDir::new().unwrap();
        let generator = NixFlakeGenerator::new(
            PathBuf::from("templates"),
            PathBuf::from("modules"),
        ).unwrap();
        
        let config = VmConfig {
            name: "test-vm".to_string(),
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 2,
            memory: 1024,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            flake_modules: vec!["webserver".to_string()],
            kernel: None,
            init_command: None,
        };
        
        let flake = generator.generate_vm_flake(&config).unwrap();
        
        // Verify the generated flake
        assert!(flake.contains("microvm.vms.test-vm"));
        assert!(flake.contains("inputs.blixard-modules.webserver"));
        assert!(flake.contains("cloud-hypervisor"));
    }
    
    #[test]
    fn test_flake_validation() {
        // Test that generated flakes pass `nix flake check`
        todo!("Implement flake validation test")
    }
}
```

### 3.2 Integration Tests

Test VM lifecycle with mock environments:

```rust
// blixard-vm/tests/lifecycle_tests.rs
#[tokio::test]
async fn test_vm_lifecycle() {
    let backend = create_test_backend().await;
    
    let config = VmConfig {
        name: "lifecycle-test".to_string(),
        hypervisor: Hypervisor::CloudHypervisor,
        vcpus: 1,
        memory: 256,
        // ... minimal config
    };
    
    // Create VM
    backend.create_vm(&config, 1).await.unwrap();
    
    // Start VM
    backend.start_vm(&config.name).await.unwrap();
    
    // Verify running
    let status = backend.get_vm_status(&config.name).await.unwrap();
    assert_eq!(status, Some(VmStatus::Running));
    
    // Stop VM
    backend.stop_vm(&config.name).await.unwrap();
    
    // Verify stopped
    let status = backend.get_vm_status(&config.name).await.unwrap();
    assert_eq!(status, Some(VmStatus::Stopped));
    
    // Clean up
    backend.delete_vm(&config.name).await.unwrap();
}
```

### 3.3 Simulation Tests

Add MadSim tests for distributed VM management:

```rust
// simulation/tests/vm_orchestration_tests.rs
#[cfg(madsim)]
#[madsim::test]
async fn test_distributed_vm_placement() {
    let cluster = TestCluster::new(3).await;
    
    // Submit VM creation requests
    for i in 0..5 {
        let config = VmConfig {
            name: format!("vm-{}", i),
            vcpus: 2,
            memory: 1024,
            // ...
        };
        
        cluster.submit_vm_create(config).await.unwrap();
    }
    
    // Wait for placement
    cluster.wait_for_vms_running(5).await;
    
    // Verify distribution across nodes
    let distribution = cluster.get_vm_distribution().await;
    assert!(distribution.values().all(|&count| count > 0));
}
```

## Phase 4: Production Features

### 4.1 Resource Management

Implement resource tracking and constraints:

```rust
pub struct ResourceManager {
    total_cpus: u32,
    total_memory: u64,
    allocated: Arc<RwLock<ResourceAllocation>>,
}

struct ResourceAllocation {
    cpu_allocated: HashMap<String, u32>,
    memory_allocated: HashMap<String, u64>,
}

impl ResourceManager {
    pub async fn can_allocate(&self, config: &VmConfig) -> bool {
        let allocated = self.allocated.read().await;
        let used_cpu: u32 = allocated.cpu_allocated.values().sum();
        let used_memory: u64 = allocated.memory_allocated.values().sum();
        
        used_cpu + config.vcpus <= self.total_cpus &&
        used_memory + config.memory as u64 <= self.total_memory
    }
}
```

### 4.2 Health Monitoring

Add VM health checking:

```rust
pub struct HealthMonitor {
    check_interval: Duration,
    manager: Arc<VmProcessManager>,
}

impl HealthMonitor {
    pub async fn start_monitoring(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                self.check_all_vms().await;
                tokio::time::sleep(self.check_interval).await;
            }
        });
    }
    
    async fn check_all_vms(&self) {
        // Check process status
        // Monitor resource usage
        // Detect crashes and restart if needed
    }
}
```

## Implementation Timeline

1. **Week 1-2**: Flake-parts infrastructure and module system
2. **Week 3-4**: Rust integration and flake generation
3. **Week 5-6**: Process management and lifecycle operations
4. **Week 7-8**: Testing infrastructure and simulation tests
5. **Week 9-10**: Resource management and monitoring
6. **Week 11-12**: Integration testing and documentation

## Success Criteria

1. **Functional VM Management**
   - Create, start, stop, delete VMs via microvm.nix
   - Support both cloud-hypervisor and firecracker
   - Proper error handling and recovery

2. **Modular Configuration**
   - Flake-parts modules for common VM patterns
   - Easy composition and customization
   - Type-safe configuration

3. **Comprehensive Testing**
   - Unit tests for all components
   - Integration tests with mock VMs
   - Simulation tests for distributed scenarios
   - CI-compatible test suite

4. **Production Readiness**
   - Resource management and constraints
   - Health monitoring and auto-recovery
   - Performance metrics and observability

## Risks and Mitigations

1. **Risk**: Nix evaluation performance
   - **Mitigation**: Cache evaluated configurations, use lazy evaluation

2. **Risk**: VM startup time
   - **Mitigation**: Pre-build common configurations, optimize Nix store

3. **Risk**: Resource contention
   - **Mitigation**: Implement strict resource accounting and limits

4. **Risk**: Network complexity
   - **Mitigation**: Start with simple TAP networking, iterate on advanced features

## Future Enhancements

1. **VM Migration**: Live migration between nodes
2. **GPU Support**: Pass-through for ML workloads
3. **Network Overlays**: Advanced SDN integration
4. **Image Registry**: Central VM image management
5. **Backup/Restore**: VM state snapshots and recovery