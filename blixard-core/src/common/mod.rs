//! Common utilities and patterns used throughout the codebase
//!
//! This module provides shared functionality to reduce duplication
//! and ensure consistency across the application.

pub mod async_utils;
pub mod error_context;
pub mod error_macros;
pub mod file_io;
pub mod p2p_utils;
// Temporarily disabled due to tonic dependencies
// pub mod conversions;
pub mod metrics;
// pub mod rate_limiting;

#[cfg(test)]
// mod example_usage;
pub use async_utils::{
    quick_read, quick_read_extract, conditional_update, with_timeout,
    BatchProcessor, AtomicCounter, OptionalArc, retry_with_backoff, concurrent_map,
};
pub use error_context::{ErrorContext, ResultContext};
pub use file_io::{
    ensure_directory, file_exists, read_binary_file_with_context, read_config_file,
    read_directory_with_context, read_files_with_extension, read_text_file_with_context,
    write_binary_file_with_context, write_config_file, write_text_file_with_context,
};
// pub use conversions::{ToProto, FromProto};
pub use metrics::{MetricTimer, MetricsRecorder};
// pub use rate_limiting::RateLimiter;
