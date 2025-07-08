//! Transport-agnostic service implementations
//!
//! This module provides implementations of services that can work
//! over both gRPC and Iroh transports.

pub mod health;
pub mod monitoring;
pub mod nix_vm_image;
pub mod status;
pub mod vm;
pub mod vm_image;

pub use health::{HealthService, HealthServiceImpl};
pub use monitoring::{MonitoringService, MonitoringServiceImpl};
pub use nix_vm_image::NixVmImageServiceImpl;
pub use status::{StatusService, StatusServiceImpl};
pub use vm::{VmService, VmServiceImpl};
pub use vm_image::{VmImageService, VmImageServiceImpl};
