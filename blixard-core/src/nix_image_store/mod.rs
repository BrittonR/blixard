//! Nix-aware P2P VM Image Store
//!
//! This module provides specialized support for distributing Nix-built
//! microVM images, containers, and store paths across the cluster using Iroh.
//! It handles content-addressed storage, derivation tracking, and efficient
//! deduplication of Nix store paths.

pub mod types;
pub mod store;

// Re-export all types for convenience
pub use types::*;
// Re-export the main store implementation
pub use store::NixImageStore;