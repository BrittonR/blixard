//! Unified error context patterns for consistent error handling
//!
//! This module provides standardized error context traits and utilities
//! that are used throughout the Blixard codebase for consistent error
//! handling and rich error context.

use crate::error::{BlixardError, BlixardResult};
use std::fmt::Debug;

/// Trait for adding context to error results
pub trait ErrorContext<T> {
    /// Add general context to an error
    fn with_context<F>(self, f: F) -> BlixardResult<T>
    where
        F: FnOnce() -> String;

    /// Add context with a static message
    fn context(self, msg: &'static str) -> BlixardResult<T>;

    /// Add context with operation name and details
    fn with_operation_context(self, operation: &str, details: &str) -> BlixardResult<T>;
}

/// Specialized error context for different domains
pub trait DomainErrorContext<T> {
    /// Add storage operation context
    fn with_storage_context(self, operation: &str) -> BlixardResult<T>;

    /// Add VM operation context
    fn with_vm_context(self, vm_name: &str, operation: &str) -> BlixardResult<T>;

    /// Add network operation context
    fn with_network_context(self, endpoint: &str, operation: &str) -> BlixardResult<T>;

    /// Add Raft operation context
    fn with_raft_context(self, node_id: u64, operation: &str) -> BlixardResult<T>;

    /// Add P2P operation context
    fn with_p2p_context(self, peer_id: &str, operation: &str) -> BlixardResult<T>;

    /// Add configuration context
    fn with_config_context(self, config_key: &str) -> BlixardResult<T>;

    /// Add security operation context
    fn with_security_context(self, operation: &str, resource: &str) -> BlixardResult<T>;
}

/// Implementation for any Result type with std::error::Error
impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_context<F>(self, f: F) -> BlixardResult<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|_e| BlixardError::Internal {
            message: f(),
        })
    }

    fn context(self, msg: &'static str) -> BlixardResult<T> {
        self.with_context(|| msg.to_string())
    }

    fn with_operation_context(self, operation: &str, details: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Failed to {}: {}", operation, details))
    }
}

/// Implementation for domain-specific error contexts
impl<T, E> DomainErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_storage_context(self, operation: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Storage operation '{}' failed", operation))
    }

    fn with_vm_context(self, vm_name: &str, operation: &str) -> BlixardResult<T> {
        self.with_context(|| format!("VM operation '{}' failed for VM '{}'", operation, vm_name))
    }

    fn with_network_context(self, endpoint: &str, operation: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Network operation '{}' failed for endpoint '{}'", operation, endpoint))
    }

    fn with_raft_context(self, node_id: u64, operation: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Raft operation '{}' failed for node {}", operation, node_id))
    }

    fn with_p2p_context(self, peer_id: &str, operation: &str) -> BlixardResult<T> {
        self.with_context(|| format!("P2P operation '{}' failed for peer '{}'", operation, peer_id))
    }

    fn with_config_context(self, config_key: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Configuration error for key '{}'", config_key))
    }

    fn with_security_context(self, operation: &str, resource: &str) -> BlixardResult<T> {
        self.with_context(|| format!("Security operation '{}' failed for resource '{}'", operation, resource))
    }
}


/// Error aggregation for collecting multiple errors
#[derive(Debug)]
pub struct ErrorCollector {
    errors: Vec<BlixardError>,
    context: String,
}

impl ErrorCollector {
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            errors: Vec::new(),
            context: context.into(),
        }
    }

    pub fn add_error(&mut self, error: BlixardError) {
        self.errors.push(error);
    }

    pub fn add_result<T>(&mut self, result: BlixardResult<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.add_error(error);
                None
            }
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    pub fn into_result<T>(self, success_value: T) -> BlixardResult<T> {
        if self.errors.is_empty() {
            Ok(success_value)
        } else {
            Err(BlixardError::Multiple {
                context: self.context,
                errors: self.errors,
            })
        }
    }

    pub fn into_result_with<T, F>(self, f: F) -> BlixardResult<T>
    where
        F: FnOnce() -> T,
    {
        self.into_result(f())
    }

    pub fn collect_results<T, I>(context: impl Into<String>, results: I) -> BlixardResult<Vec<T>>
    where
        I: IntoIterator<Item = BlixardResult<T>>,
    {
        let mut collector = ErrorCollector::new(context);
        let mut values = Vec::new();

        for result in results {
            if let Some(value) = collector.add_result(result) {
                values.push(value);
            }
        }

        collector.into_result(values)
    }
}

/// Utilities for error handling patterns
pub struct ErrorUtils;

impl ErrorUtils {
    /// Retry an operation with exponential backoff
    pub async fn retry_with_backoff<T, F, Fut>(
        operation: F,
        max_attempts: u32,
        base_delay: std::time::Duration,
        context: &str,
    ) -> BlixardResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = BlixardResult<T>>,
    {
        let mut last_error = None;
        let mut delay = base_delay;

        for attempt in 1..=max_attempts {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    last_error = Some(error);
                    
                    if attempt < max_attempts {
                        tokio::time::sleep(delay).await;
                        delay = delay.mul_f64(1.5); // Exponential backoff
                    }
                }
            }
        }

        Err(BlixardError::Internal {
            message: format!(
                "Operation '{}' failed after {} attempts: {}",
                context, 
                max_attempts,
                last_error.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "unknown error".to_string())
            ),
        })
    }

    /// Convert a panic to a BlixardError
    pub fn catch_panic<T, F>(f: F, context: &str) -> BlixardResult<T>
    where
        F: FnOnce() -> T + std::panic::UnwindSafe,
    {
        std::panic::catch_unwind(f).map_err(|panic| {
            let panic_msg = if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic".to_string()
            };

            BlixardError::Internal {
                message: format!("Panic in '{}': {}", context, panic_msg),
            }
        })
    }

    /// Convert an async panic to a BlixardError
    pub async fn catch_async_panic<T, F, Fut>(f: F, context: &str) -> BlixardResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn(f())
            .await
            .map_err(|join_error| {
                if join_error.is_panic() {
                    BlixardError::Internal {
                        message: format!("Async panic in '{}': {:?}", context, join_error),
                    }
                } else {
                    BlixardError::Internal {
                        message: format!("Async task cancelled in '{}'", context),
                    }
                }
            })
    }

    /// Timeout an operation
    pub async fn with_timeout<T, F, Fut>(
        operation: F,
        timeout: std::time::Duration,
        context: &str,
    ) -> BlixardResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = BlixardResult<T>>,
    {
        tokio::time::timeout(timeout, operation())
            .await
            .map_err(|_| BlixardError::Timeout {
                operation: context.to_string(),
                duration: timeout,
            })?
    }
}

/// Macro for adding context to error results
#[macro_export]
macro_rules! context {
    ($result:expr, $msg:expr) => {
        $result.with_context(|| $msg.to_string())
    };
    ($result:expr, $fmt:expr, $($arg:tt)*) => {
        $result.with_context(|| format!($fmt, $($arg)*))
    };
}

/// Macro for adding domain-specific context
#[macro_export]
macro_rules! vm_context {
    ($result:expr, $vm_name:expr, $operation:expr) => {
        $result.with_vm_context($vm_name, $operation)
    };
}

#[macro_export]
macro_rules! storage_context {
    ($result:expr, $operation:expr) => {
        $result.with_storage_context($operation)
    };
}

#[macro_export]
macro_rules! network_context {
    ($result:expr, $endpoint:expr, $operation:expr) => {
        $result.with_network_context($endpoint, $operation)
    };
}

#[macro_export]
macro_rules! raft_context {
    ($result:expr, $node_id:expr, $operation:expr) => {
        $result.with_raft_context($node_id, $operation)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_context() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let result: Result<(), _> = Err(io_error);
        
        let blixard_result = result.with_context(|| "Failed to read configuration file".to_string());
        
        assert!(blixard_result.is_err());
        if let Err(BlixardError::Internal { message, source }) = blixard_result {
            assert_eq!(message, "Failed to read configuration file");
            assert!(source.is_some());
        } else {
            panic!("Expected Internal error");
        }
    }

    #[test]
    fn test_domain_context() {
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "Access denied");
        let result: Result<(), _> = Err(io_error);
        
        let blixard_result = result.with_vm_context("test-vm", "start");
        
        assert!(blixard_result.is_err());
        if let Err(BlixardError::Internal { message, .. }) = blixard_result {
            assert!(message.contains("VM operation 'start' failed for VM 'test-vm'"));
        } else {
            panic!("Expected Internal error");
        }
    }

    #[test]
    fn test_error_collector() {
        let mut collector = ErrorCollector::new("Test operations");
        
        // Add some successful results
        assert_eq!(collector.add_result(Ok::<i32, BlixardError>(1)), Some(1));
        assert_eq!(collector.add_result(Ok::<i32, BlixardError>(2)), Some(2));
        
        // Add some errors
        collector.add_result(Err::<i32, _>(BlixardError::Internal {
            message: "Error 1".to_string(),
            source: None,
        }));
        collector.add_result(Err::<i32, _>(BlixardError::Internal {
            message: "Error 2".to_string(),
            source: None,
        }));
        
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 2);
        
        let result = collector.into_result(());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_retry_with_backoff() {
        let mut attempt_count = 0;
        
        let operation = || {
            attempt_count += 1;
            async move {
                if attempt_count < 3 {
                    Err(BlixardError::Internal {
                        message: "Temporary failure".to_string(),
                        source: None,
                    })
                } else {
                    Ok("Success")
                }
            }
        };
        
        let result = ErrorUtils::retry_with_backoff(
            operation,
            5,
            std::time::Duration::from_millis(10),
            "test operation",
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(attempt_count, 3);
    }

    #[test]
    fn test_context_macro() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let result: Result<(), _> = Err(io_error);
        
        let blixard_result = context!(result, "Reading config file");
        
        assert!(blixard_result.is_err());
    }

    #[test]
    fn test_vm_context_macro() {
        let io_error = io::Error::new(io::ErrorKind::TimedOut, "Operation timed out");
        let result: Result<(), _> = Err(io_error);
        
        let blixard_result = vm_context!(result, "test-vm", "start");
        
        assert!(blixard_result.is_err());
    }

    #[test]
    fn test_error_utils_catch_panic() {
        let result = ErrorUtils::catch_panic(|| {
            panic!("Test panic");
        }, "test operation");
        
        assert!(result.is_err());
        if let Err(BlixardError::Internal { message, .. }) = result {
            assert!(message.contains("Panic in 'test operation'"));
            assert!(message.contains("Test panic"));
        } else {
            panic!("Expected Internal error");
        }
    }

    #[tokio::test]
    async fn test_error_utils_with_timeout() {
        // Test successful operation within timeout
        let result = ErrorUtils::with_timeout(
            || async { Ok::<_, BlixardError>("success") },
            std::time::Duration::from_millis(100),
            "fast operation",
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        
        // Test operation that times out
        let result = ErrorUtils::with_timeout(
            || async {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                Ok::<_, BlixardError>("success")
            },
            std::time::Duration::from_millis(50),
            "slow operation",
        ).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BlixardError::Timeout { .. }));
    }
}