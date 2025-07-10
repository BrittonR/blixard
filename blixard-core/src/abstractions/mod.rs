//! Abstractions for testability and dependency injection
//!
//! This module provides trait-based abstractions for external dependencies,
//! enabling better testing through mocking and dependency injection.

pub mod config;
pub mod filesystem;
pub mod command;
pub mod storage;
// Temporarily disabled: uses gRPC which we're removing
// pub mod network;
pub mod time;
// Temporarily disabled: has dependencies on network module
// pub mod container;

pub use config::ConfigProvider;
pub use filesystem::FileSystem;
pub use command::{CommandExecutor, CommandOptions, CommandOutput, TokioCommandExecutor, MockCommandExecutor};
pub use storage::{NodeRepository, TaskRepository, VmRepository};
// pub use network::NetworkClient;
pub use time::Clock;
// pub use container::{ServiceContainer, ServiceContainerBuilder};
