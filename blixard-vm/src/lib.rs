pub mod microvm_backend;

pub use microvm_backend::MicrovmBackend;

// Re-export core types for convenience
pub use blixard_core::{
    vm_backend::{VmBackend, VmManager},
    types::{VmConfig, VmStatus},
    error::{BlixardError, BlixardResult},
};