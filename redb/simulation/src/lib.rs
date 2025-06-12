//! Simulation testing library for Blixard
//!
//! This crate provides deterministic testing capabilities using MadSim.

// Include the generated proto code
pub mod proto {
    // Since we output to sim/ directory in build.rs
    tonic::include_proto!("sim/blixard");
}

// Re-export commonly used types for tests
pub use proto::*;