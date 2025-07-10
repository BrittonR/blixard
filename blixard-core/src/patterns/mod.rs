//! Common design patterns and abstractions for Blixard
//!
//! This module provides standardized patterns and traits that are used
//! throughout the Blixard codebase to ensure consistency and reduce
//! code duplication.

pub mod lifecycle;
pub mod builder;
pub mod factory;
pub mod error_context;
pub mod resource_pool;
pub mod retry;

#[cfg(test)]
pub mod resource_pool_examples;
#[cfg(test)]
pub mod retry_migration_example;

pub use lifecycle::{
    LifecycleManager, BackgroundTaskManager, LifecycleState, HealthStatus, 
    LifecycleStats, LifecycleBase
};
pub use builder::{Builder, AsyncBuilder};
pub use factory::{Factory, Registry};
pub use error_context::ErrorContext;
pub use resource_pool::{
    ResourcePool, PoolableResource, ResourceFactory, PooledResource, 
    PoolConfig, PoolStats
};
pub use retry::{
    retry, RetryConfig, RetryBuilder, BackoffStrategy, Retryable
};