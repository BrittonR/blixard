//! Constructor methods and convenience functions for BlixardError
//!
//! This module provides factory methods and builder patterns for creating
//! structured errors with proper context and error chaining.

use super::types::BlixardError;

impl BlixardError {
    /// Create a configuration error with component and message
    /// 
    /// This is the preferred way to create configuration errors.
    /// 
    /// # Examples
    /// ```rust
    /// use blixard_core::error::BlixardError;
    /// 
    /// let err = BlixardError::configuration("node.bind_address", "Invalid port number");
    /// let err = BlixardError::configuration("vm.memory", "Memory must be at least 512MB");
    /// ```
    pub fn configuration(component: impl Into<String>, message: impl Into<String>) -> Self {
        BlixardError::ConfigurationError {
            component: component.into(),
            message: message.into(),
        }
    }

    /// Create a general configuration error (for backward compatibility)
    /// 
    /// Prefer using `configuration()` with a specific component when possible.
    pub fn config_general(message: impl Into<String>) -> Self {
        BlixardError::ConfigurationError {
            component: "general".to_string(),
            message: message.into(),
        }
    }

    /// Create a validation configuration error (for input validation)
    pub fn config_validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        BlixardError::ConfigurationError {
            component: field.into(),
            message: message.into(),
        }
    }

    /// Create a Storage error with a boxed source
    pub fn storage<E: std::error::Error + Send + Sync + 'static>(
        operation: impl Into<String>,
        source: E,
    ) -> Self {
        BlixardError::Storage {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Create a Raft error with a boxed source
    pub fn raft<E: std::error::Error + Send + Sync + 'static>(
        operation: impl Into<String>,
        source: E,
    ) -> Self {
        BlixardError::Raft {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Create a Serialization error with a boxed source
    pub fn serialization<E: std::error::Error + Send + Sync + 'static>(
        operation: impl Into<String>,
        source: E,
    ) -> Self {
        BlixardError::Serialization {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Create a LockPoisoned error with a boxed source
    pub fn lock_poisoned<E: std::error::Error + Send + Sync + 'static>(
        operation: impl Into<String>,
        source: E,
    ) -> Self {
        BlixardError::LockPoisoned {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Create a DatabaseError with a boxed source
    pub fn database<E: std::error::Error + Send + Sync + 'static>(
        operation: impl Into<String>,
        source: E,
    ) -> Self {
        BlixardError::DatabaseError {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Create a lock poisoned error for internal use
    pub fn lock_poisoned_internal(resource: &str) -> Self {
        BlixardError::LockPoisoned {
            operation: format!("accessing {}", resource),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Lock was poisoned by another thread",
            )),
        }
    }

    /// Create a service unavailable error
    pub fn service_unavailable(service: &str) -> Self {
        BlixardError::NotInitialized {
            component: service.to_string(),
        }
    }

    /// Create an invalid configuration error with detailed context
    pub fn invalid_config<T: std::fmt::Display>(field: &str, value: T, reason: &str) -> Self {
        BlixardError::ConfigurationError {
            component: field.to_string(),
            message: format!("Invalid value '{}': {}", value, reason),
        }
    }

    /// Create a generic error from another error type with context
    pub fn from_other<E: std::error::Error + Send + Sync + 'static>(
        operation: &str,
        source: E,
    ) -> Self {
        BlixardError::Internal {
            message: format!("Operation '{}' failed: {}", operation, source),
        }
    }

    /// Create a connection error with address and source
    pub fn connection_error<E: std::fmt::Display>(address: &str, source: E) -> Self {
        BlixardError::ConnectionError {
            address: address.to_string(),
            details: source.to_string(),
        }
    }

    /// Create a timeout error with operation and duration
    pub fn timeout_error(operation: &str, duration: std::time::Duration) -> Self {
        BlixardError::Timeout {
            operation: operation.to_string(),
            duration,
        }
    }

    /// Create a resource exhausted error
    pub fn resource_exhausted(resource: &str) -> Self {
        BlixardError::ResourceExhausted {
            resource: resource.to_string(),
        }
    }

    /// Create a not found error
    pub fn not_found(resource: &str) -> Self {
        BlixardError::NotFound {
            resource: resource.to_string(),
        }
    }

    /// Create an already exists error
    pub fn already_exists(resource: &str) -> Self {
        BlixardError::AlreadyExists {
            resource: resource.to_string(),
        }
    }

    /// Create an invalid input error
    pub fn invalid_input(field: &str, message: &str) -> Self {
        BlixardError::InvalidInput {
            field: field.to_string(),
            message: message.to_string(),
        }
    }

    /// Create a validation error
    pub fn validation_error(field: &str, message: &str) -> Self {
        BlixardError::Validation {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}