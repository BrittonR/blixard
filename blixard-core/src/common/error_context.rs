//! Enhanced error handling with context
//!
//! This module provides traits and utilities for adding context to errors,
//! making debugging easier and error messages more informative.

use crate::error::{BlixardError, BlixardResult};
use std::fmt::Display;

/// Trait for adding context to errors
pub trait ErrorContext<T> {
    /// Add a static context message to the error
    fn context(self, msg: &str) -> Result<T, BlixardError>;
    
    /// Add a dynamic context message to the error
    fn with_context<F>(self, f: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> String;
        
    /// Add context with additional details
    fn with_details<F>(self, operation: &str, f: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> String;
}

/// Implementation for Result types
impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: Into<BlixardError>,
{
    fn context(self, msg: &str) -> Result<T, BlixardError> {
        self.map_err(|e| {
            let base_error = e.into();
            BlixardError::Internal {
                message: format!("{}: {}", msg, base_error),
            }
        })
    }
    
    fn with_context<F>(self, f: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            BlixardError::Internal {
                message: format!("{}: {}", f(), base_error),
            }
        })
    }
    
    fn with_details<F>(self, operation: &str, f: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            match base_error {
                BlixardError::Storage { source, .. } => BlixardError::Storage {
                    operation: operation.to_string(),
                    source,
                },
                BlixardError::Raft { source, .. } => BlixardError::Raft {
                    operation: operation.to_string(),
                    source,
                },
                BlixardError::Serialization { source, .. } => BlixardError::Serialization {
                    operation: operation.to_string(),
                    source,
                },
                _ => BlixardError::Internal {
                    message: format!("{} ({}): {}", operation, f(), base_error),
                },
            }
        })
    }
}

/// Trait for adding context to BlixardResult
pub trait ResultContext<T> {
    /// Log error with context and return it
    fn log_error(self, context: &str) -> BlixardResult<T>;
    
    /// Log error if it occurs but continue
    fn log_if_error(self, context: &str) -> BlixardResult<T>;
    
    /// Convert to Status with proper error mapping
    fn to_status(self) -> Result<T, tonic::Status>;
}

impl<T> ResultContext<T> for BlixardResult<T> {
    fn log_error(self, context: &str) -> BlixardResult<T> {
        if let Err(ref e) = self {
            tracing::error!("{}: {}", context, e);
        }
        self
    }
    
    fn log_if_error(self, context: &str) -> BlixardResult<T> {
        if let Err(ref e) = self {
            tracing::warn!("{}: {}", context, e);
        }
        self
    }
    
    fn to_status(self) -> Result<T, tonic::Status> {
        self.map_err(crate::grpc_server::common::conversions::error_to_status)
    }
}

/// Extension trait for Options to convert to errors with context
pub trait OptionContext<T> {
    /// Convert None to an error with context
    fn context(self, msg: &str) -> BlixardResult<T>;
    
    /// Convert None to a NotFound error
    fn not_found(self, resource: &str) -> BlixardResult<T>;
}

impl<T> OptionContext<T> for Option<T> {
    fn context(self, msg: &str) -> BlixardResult<T> {
        self.ok_or_else(|| BlixardError::Internal {
            message: msg.to_string(),
        })
    }
    
    fn not_found(self, resource: &str) -> BlixardResult<T> {
        self.ok_or_else(|| BlixardError::NotFound {
            resource: resource.to_string(),
        })
    }
}

/// Helper for creating errors with structured context
pub struct ErrorBuilder {
    operation: String,
    details: Vec<(String, String)>,
}

impl ErrorBuilder {
    /// Create a new error builder for an operation
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            details: Vec::new(),
        }
    }
    
    /// Add a detail to the error context
    pub fn detail(mut self, key: impl Display, value: impl Display) -> Self {
        self.details.push((key.to_string(), value.to_string()));
        self
    }
    
    /// Build an Internal error
    pub fn internal(self) -> BlixardError {
        let details_str = self.details
            .into_iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
            
        BlixardError::Internal {
            message: format!("{} ({})", self.operation, details_str),
        }
    }
    
    /// Build a NotFound error
    pub fn not_found(self, resource: impl Display) -> BlixardError {
        BlixardError::NotFound {
            resource: format!("{} in {} operation", resource, self.operation),
        }
    }
    
    /// Build a VmOperationFailed error
    pub fn vm_operation_failed(self) -> BlixardError {
        let details_str = self.details
            .into_iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
            
        BlixardError::VmOperationFailed {
            operation: self.operation,
            details: details_str,
        }
    }
}

/// Convenience function to create an error builder
pub fn error_for(operation: impl Into<String>) -> ErrorBuilder {
    ErrorBuilder::new(operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_context() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        
        let with_context = result.context("Failed to read config");
        assert!(with_context.is_err());
        let err = with_context.unwrap_err();
        match err {
            BlixardError::Internal { message } => {
                assert!(message.contains("Failed to read config"));
                assert!(message.contains("file not found"));
            }
            _ => panic!("Expected Internal error"),
        }
    }
    
    #[test]
    fn test_option_context() {
        let opt: Option<i32> = None;
        let result = opt.not_found("user");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::NotFound { resource } => {
                assert_eq!(resource, "user");
            }
            _ => panic!("Expected NotFound error"),
        }
    }
    
    #[test]
    fn test_error_builder() {
        let error = error_for("create_vm")
            .detail("vm_name", "test-vm")
            .detail("vcpus", 4)
            .detail("memory", "8GB")
            .vm_operation_failed();
            
        match error {
            BlixardError::VmOperationFailed { operation, details } => {
                assert_eq!(operation, "create_vm");
                assert!(details.contains("vm_name: test-vm"));
                assert!(details.contains("vcpus: 4"));
                assert!(details.contains("memory: 8GB"));
            }
            _ => panic!("Expected VmOperationFailed error"),
        }
    }
}