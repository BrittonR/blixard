//! Common utilities and patterns used throughout the codebase
//!
//! This module provides shared functionality to reduce duplication
//! and ensure consistency across the application.

pub mod error_context;
// Temporarily disabled due to tonic dependencies
// pub mod conversions;
pub mod metrics;
// pub mod rate_limiting;

#[cfg(test)]
// mod example_usage;

pub use error_context::{ErrorContext, ResultContext};
// pub use conversions::{ToProto, FromProto};
pub use metrics::{MetricsRecorder, MetricTimer};
// pub use rate_limiting::RateLimiter;