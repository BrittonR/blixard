//! Abstractions for testability and dependency injection
//!
//! This module provides trait-based abstractions for external dependencies,
//! enabling better testing through mocking and dependency injection.

pub mod storage;
pub mod filesystem;
pub mod process;
pub mod config;
// Temporarily disabled: uses gRPC which we're removing
// pub mod network;
pub mod time;
// Temporarily disabled: has dependencies on network module
// pub mod container;

pub use storage::{VmRepository, TaskRepository, NodeRepository};
pub use filesystem::FileSystem;
pub use process::ProcessExecutor;
pub use config::ConfigProvider;
// pub use network::NetworkClient;
pub use time::Clock;
// pub use container::{ServiceContainer, ServiceContainerBuilder};