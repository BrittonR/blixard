//! Common design patterns and abstractions for Blixard
//!
//! This module provides standardized patterns and traits that are used
//! throughout the Blixard codebase to ensure consistency and reduce
//! code duplication.

pub mod builder;
// pub mod circuit_breaker;
pub mod error_context;
pub mod factory;
// pub mod graceful_degradation;
pub mod lifecycle;
pub mod resource_pool;
pub mod retry;

#[cfg(test)]
pub mod error_context_examples;
#[cfg(test)]
pub mod resource_pool_examples;
#[cfg(test)]
pub mod retry_migration_example;

pub use builder::{AsyncBuilder, Builder, HasBuilder};
// pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerState, CircuitBreakerStats};
pub use error_context::ErrorContext;
pub use factory::{Creatable, Factory, Registry};
// pub use graceful_degradation::{DegradationLevel, DegradationStats, GracefulDegradation, ServiceLevel};
pub use lifecycle::{
    BackgroundTaskManager, HealthStatus, LifecycleBase, LifecycleManager, LifecycleState,
    LifecycleStats,
};
pub use resource_pool::{
    PoolConfig, PoolStats, PoolableResource, PooledResource, ResourceFactory, ResourcePool,
};
pub use retry::{retry, BackoffStrategy, RetryBuilder, RetryConfig, Retryable};
