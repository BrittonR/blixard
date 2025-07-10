//! Common design patterns and abstractions for Blixard
//!
//! This module provides standardized patterns and traits that are used
//! throughout the Blixard codebase to ensure consistency and reduce
//! code duplication.

pub mod lifecycle;
pub mod builder;
pub mod factory;
pub mod error_context;

pub use lifecycle::{
    LifecycleManager, BackgroundTaskManager, LifecycleState, HealthStatus, 
    LifecycleStats, LifecycleBase
};
pub use builder::{Builder, AsyncBuilder};
pub use factory::{Factory, Registry};
pub use error_context::ErrorContext;