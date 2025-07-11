//! # Blixard VM - MicroVM.nix Integration
//!
//! This crate provides VM management capabilities for Blixard using microvm.nix.
//!
//! ## Features
//!
//! - **MicroVM.nix Integration**: Full support for cloud-hypervisor and firecracker
//! - **Nix Flake Generation**: Dynamic generation of NixOS configurations
//! - **Process Management**: Lifecycle management for VM processes
//! - **Template Systems**: Both standard and flake-parts templates
//! - **Modular Configuration**: Reusable VM profiles via flake-parts
//!
//! ## Template Systems
//!
//! Blixard VM supports two template systems:
//!
//! ### Standard Template
//! Simple, direct NixOS configuration for single VMs:
//! ```rust,no_run
//! use blixard_vm::{NixFlakeGenerator, types::VmConfig};
//! # use std::path::PathBuf;
//! # fn example() -> blixard_vm::BlixardResult<()> {
//! let generator = NixFlakeGenerator::new(
//!     PathBuf::from("templates"),
//!     PathBuf::from("modules")
//! )?;
//! let config = VmConfig::default();
//! generator.write_flake(&config, &PathBuf::from("output"))?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Flake-Parts Template
//! Modular, composable configuration for complex deployments:
//! ```rust,no_run
//! use blixard_vm::{NixFlakeGenerator, types::VmConfig};
//! # use std::path::PathBuf;
//! # fn example() -> blixard_vm::BlixardResult<()> {
//! let generator = NixFlakeGenerator::new(
//!     PathBuf::from("templates"),
//!     PathBuf::from("modules")
//! )?;
//! let mut config = VmConfig::default();
//! config.flake_modules = vec!["webserver".to_string(), "monitoring".to_string()];
//! generator.write_flake_parts(&config, &PathBuf::from("output"))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Available Modules (flake-parts)
//!
//! - `webserver` - Nginx web server
//! - `database` - PostgreSQL database
//! - `monitoring` - Prometheus + Grafana
//! - `containerRuntime` - Docker runtime
//!
//! See `docs/FLAKE_PARTS_GUIDE.md` for detailed documentation.

pub mod config_converter;
pub mod console_reader;
pub mod health_check_helpers;
pub mod ip_pool;
pub mod microvm_backend;
pub mod nix_generator;
pub mod process_manager;
pub mod types;

pub use microvm_backend::{MicrovmBackend, MicrovmBackendFactory};
pub use nix_generator::NixFlakeGenerator;
pub use process_manager::{VmProcess, VmProcessManager};

// Re-export enhanced VM types
pub use types::*;

// Re-export core types for convenience
pub use blixard_core::{
    error::{BlixardError, BlixardResult},
    types::VmStatus,
    vm_backend::{VmBackend, VmManager},
};
