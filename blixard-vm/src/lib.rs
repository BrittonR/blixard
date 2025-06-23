pub mod microvm_backend;
pub mod types;
pub mod nix_generator;

pub use microvm_backend::MicrovmBackend;
pub use nix_generator::NixFlakeGenerator;

// Re-export enhanced VM types
pub use types::*;

// Re-export core types for convenience
pub use blixard_core::{
    vm_backend::{VmBackend, VmManager},
    types::VmStatus,
    error::{BlixardError, BlixardResult},
};