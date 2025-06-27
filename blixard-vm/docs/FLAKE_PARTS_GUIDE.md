# Flake-Parts Integration Guide

This guide explains how to use the flake-parts template system for creating modular, composable VM configurations in Blixard.

## Overview

Blixard supports two template systems for generating Nix flakes:

1. **Standard Template** - Simple, direct NixOS configuration
2. **Flake-Parts Template** - Modular, composable configuration using the flake-parts framework

## When to Use Flake-Parts

Use the flake-parts template when you need:

- **Multiple VMs** with shared configuration
- **Reusable modules** across different VM types
- **Complex service compositions** (e.g., web + database + monitoring)
- **Type-safe configuration** with validation
- **Per-system outputs** (packages, apps, shells)

Use the standard template when you need:

- **Single VM** with simple configuration
- **Quick prototyping** without complex requirements
- **Minimal Nix evaluation overhead**
- **Direct control** over NixOS configuration

## Template Comparison

### Standard Template Structure
```nix
{
  inputs = {
    nixpkgs.url = "...";
    microvm.url = "...";
  };
  
  outputs = { self, nixpkgs, microvm }:
    let system = "x86_64-linux";
    in {
      nixosConfigurations.myvm = nixpkgs.lib.nixosSystem {
        # Direct NixOS configuration
      };
    };
}
```

### Flake-Parts Template Structure
```nix
{
  inputs = {
    nixpkgs.url = "...";
    flake-parts.url = "...";
    microvm.url = "...";
    blixard-modules.url = "...";
  };
  
  outputs = inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];
      
      perSystem = { config, ... }: {
        # Modular configuration with blixardVMs option
      };
    };
}
```

## Using Flake-Parts Templates

### 1. Basic Usage in Rust

```rust
use blixard_vm::{
    nix_generator::NixFlakeGenerator,
    types::{VmConfig, Hypervisor},
};

// Create generator
let generator = NixFlakeGenerator::new(template_dir, modules_dir)?;

// Create VM config with modules
let config = VmConfig {
    name: "my-vm".to_string(),
    flake_modules: vec!["webserver", "monitoring"],
    // ... other config
};

// Generate flake-parts based flake
let flake_path = generator.write_flake_parts(&config, output_dir)?;
```

### 2. Available Modules

Blixard provides these pre-built modules:

- **`webserver`** - Nginx web server with sensible defaults
- **`database`** - PostgreSQL database server
- **`monitoring`** - Prometheus + Grafana stack
- **`containerRuntime`** - Docker container runtime
- **`blixardVM`** - Base VM configuration (auto-included)

### 3. Module Composition

Modules can be combined to create complex VMs:

```rust
let config = VmConfig {
    name: "fullstack-vm".to_string(),
    flake_modules: vec![
        "webserver",      // Frontend
        "database",       // Backend storage
        "monitoring",     // Observability
        "containerRuntime" // For microservices
    ],
    nixos_modules: vec![
        NixModule::Inline(r#"{
            # Custom configuration
            services.nginx.virtualHosts."api.local" = {
                locations."/".proxyPass = "http://localhost:3000";
            };
        }"#.to_string()),
    ],
    // ...
};
```

### 4. Custom Modules

Add custom modules using the `nixos_modules` field:

```rust
use blixard_vm::types::NixModule;

let config = VmConfig {
    nixos_modules: vec![
        // Inline module
        NixModule::Inline(r#"{
            services.myapp = {
                enable = true;
                port = 8080;
            };
        }"#.to_string()),
        
        // File-based module
        NixModule::File(PathBuf::from("./my-module.nix")),
        
        // Reference to flake-parts module
        NixModule::FlakePart("my-custom-module".to_string()),
    ],
    // ...
};
```

## Direct Flake-Parts Usage

You can also write flake-parts configurations directly:

```nix
# flake.nix
{
  inputs = {
    # ... inputs
    blixard-modules.url = "github:your-org/blixard/main?dir=blixard-vm/nix/modules";
  };

  outputs = inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.blixard-modules.flakeModule
      ];
      
      systems = [ "x86_64-linux" ];
      
      perSystem = { config, pkgs, ... }: {
        blixardVMs = {
          web = {
            hypervisor = "cloud-hypervisor";
            vcpus = 2;
            memory = 1024;
            modules = [ "webserver" ];
            extraConfig = {
              # VM-specific config
            };
          };
          
          db = {
            hypervisor = "cloud-hypervisor";
            vcpus = 4;
            memory = 4096;
            modules = [ "database" ];
            volumes = [{
              type = "data";
              path = "/var/lib/postgresql";
              size = 10240;
            }];
          };
        };
      };
    };
}
```

## Benefits of Flake-Parts

1. **Type Safety**: Options are validated at evaluation time
2. **Composition**: Modules can be mixed and matched
3. **Reusability**: Common patterns are encapsulated in modules
4. **Per-System**: Automatically handles multi-architecture builds
5. **Integration**: Works with existing flake-parts modules
6. **Outputs**: Generates packages, apps, and shells automatically

## Migration from Standard Templates

To migrate existing VMs to flake-parts:

1. Identify common patterns across VMs
2. Extract patterns into reusable modules
3. Update VM configs to use `flake_modules`
4. Test with `nix flake check`

Example migration:

```rust
// Before (standard template)
let config = VmConfig {
    nixos_modules: vec![
        NixModule::Inline(r#"{
            services.nginx.enable = true;
            services.nginx.virtualHosts.default = {
                root = "/var/www";
            };
            networking.firewall.allowedTCPPorts = [ 80 443 ];
        }"#.to_string()),
    ],
    // ...
};

// After (flake-parts)
let config = VmConfig {
    flake_modules: vec!["webserver"],
    nixos_modules: vec![
        NixModule::Inline(r#"{
            # Only VM-specific overrides
            services.nginx.virtualHosts.default.root = "/var/www";
        }"#.to_string()),
    ],
    // ...
};
```

## Best Practices

1. **Use modules for common patterns**: Don't repeat configuration
2. **Keep modules focused**: One concern per module
3. **Document module options**: Use descriptions in module definitions
4. **Test modules independently**: Use `nix flake check`
5. **Version modules**: Pin blixard-modules input to specific revisions

## Troubleshooting

### Module not found
```
error: attribute 'mymodule' missing
```
Ensure the module is defined in `flake.nixosModules` and referenced correctly.

### Type errors
```
error: value is a string while a list was expected
```
Check the module option types in `flake-module.nix`.

### Evaluation performance
If evaluation is slow, consider:
- Reducing the number of modules
- Using the standard template for simple VMs
- Caching evaluated configurations

## Examples

See the `examples/` directory for complete examples:
- `flake-parts-vm.nix` - Comprehensive multi-VM setup
- `vm_lifecycle_flake_parts.rs` - Rust API usage
- `examples/minimal-flake-parts.nix` - Minimal configuration

## Future Enhancements

Planned improvements for flake-parts integration:
- Module dependency resolution
- Automatic module discovery
- GUI/TUI for module selection
- Module marketplace/registry
- Performance optimizations