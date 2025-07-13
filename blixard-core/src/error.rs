//! Error handling and recovery strategies for Blixard
//!
//! This module provides a comprehensive error handling framework designed for
//! distributed systems. It includes structured error types, context propagation,
//! recovery strategies, and observability integration.
//!
//! ## Error Handling Philosophy
//!
//! Blixard follows these principles for robust error handling:
//!
//! ### 1. Structured Error Types
//! All errors are structured with sufficient context for debugging and recovery:
//! - **Operation Context**: What operation was being performed
//! - **Resource Context**: Which resource was involved
//! - **Cause Chain**: Full error chain from root cause to user error
//! - **Actionable Information**: What the user can do to fix the issue
//!
//! ### 2. Error Recovery
//! Different error types have different recovery strategies:
//! - **Transient Errors**: Automatic retry with exponential backoff
//! - **Resource Errors**: Graceful degradation or resource cleanup
//! - **Logic Errors**: Fail fast with detailed context
//! - **System Errors**: Escalate to higher-level recovery mechanisms
//!
//! ### 3. Observability
//! All errors are instrumented for monitoring and alerting:
//! - **Structured Logging**: Consistent error logging with context
//! - **Metrics**: Error rate tracking by type and operation
//! - **Tracing**: Full request traces including error propagation
//! - **Alerting**: Critical errors trigger immediate notifications
//!
//! ## Error Categories
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Blixard Error Taxonomy                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  System Errors          │  Resource Errors     │  Logic Errors  │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ • Storage       │    │  │ • Quota Exceed  │  │  │ • Config  │ │
//! │  │ • Network       │    │  │ • Not Found     │  │  │ • Invalid │ │
//! │  │ • IO            │    │  │ • Unavailable   │  │  │ • Validate│ │
//! │  │ • Internal      │    │  │ • Exhausted     │  │  │ • Auth    │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Distributed Errors     │  Operational Errors  │  Temporary     │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ • Raft          │    │  │ • Timeout       │  │  │ • Lock    │ │
//! │  │ • Cluster       │    │  │ • Not Leader    │  │  │ • Busy    │ │
//! │  │ • P2P           │    │  │ • Not Init      │  │  │ • Retry   │ │
//! │  │ • Consensus     │    │  │ • Already Exist │  │  │ • Circuit │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage Patterns
//!
//! ### Basic Error Handling
//! ```rust,no_run
//! use blixard_core::{BlixardError, BlixardResult};
//!
//! async fn create_vm(name: &str) -> BlixardResult<()> {
//!     // Validate input
//!     if name.is_empty() {
//!         return Err(BlixardError::InvalidInput {
//!             field: "name".to_string(),
//!             message: "VM name cannot be empty".to_string(),
//!         });
//!     }
//!     
//!     // Check resources
//!     if !has_sufficient_resources().await? {
//!         return Err(BlixardError::ResourceExhausted {
//!             resource: "memory".to_string(),
//!         });
//!     }
//!     
//!     // Create VM with proper error context
//!     create_vm_impl(name)
//!         .await
//!         .map_err(|e| BlixardError::VmOperationFailed {
//!             operation: "create".to_string(),
//!             details: e.to_string(),
//!         })
//! }
//! ```
//!
//! ### Error Context Propagation
//! ```rust,no_run
//! use blixard_core::BlixardError;
//!
//! async fn save_vm_config(config: &VmConfig) -> BlixardResult<()> {
//!     let serialized = serde_json::to_vec(config)
//!         .map_err(|e| BlixardError::serialization("serialize VM config", e))?;
//!     
//!     write_to_database("vm_configs", &config.name, &serialized)
//!         .await
//!         .map_err(|e| BlixardError::storage("save VM config", e))?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Retry Patterns
//! ```rust,no_run
//! use blixard_core::{BlixardError, BlixardResult};
//! use tokio::time::{sleep, Duration};
//!
//! async fn with_retry<F, T>(
//!     operation_name: &str,
//!     mut operation: F,
//!     max_retries: u32,
//! ) -> BlixardResult<T>
//! where
//!     F: FnMut() -> BlixardResult<T>,
//! {
//!     let mut last_error = None;
//!     
//!     for attempt in 0..=max_retries {
//!         match operation() {
//!             Ok(result) => return Ok(result),
//!             Err(e) => {
//!                 last_error = Some(e);
//!                 
//!                 // Check if error is retryable
//!                 if !is_retryable_error(&last_error.as_ref().unwrap()) {
//!                     break;
//!                 }
//!                 
//!                 if attempt < max_retries {
//!                     let delay = Duration::from_millis(100 * (1 << attempt));
//!                     sleep(delay).await;
//!                 }
//!             }
//!         }
//!     }
//!     
//!     Err(last_error.unwrap())
//! }
//!
//! fn is_retryable_error(error: &BlixardError) -> bool {
//!     match error {
//!         BlixardError::TemporaryFailure { .. } |
//!         BlixardError::Timeout { .. } |
//!         BlixardError::ConnectionError { .. } => true,
//!         BlixardError::InvalidInput { .. } |
//!         BlixardError::NotFound { .. } |
//!         BlixardError::AlreadyExists { .. } => false,
//!         _ => false, // Conservative: don't retry unknown errors
//!     }
//! }
//! ```
//!
//! ### Circuit Breaker Pattern
//! ```rust,no_run
//! use std::sync::atomic::{AtomicU32, Ordering};
//! use std::sync::Arc;
//! use tokio::time::{Duration, Instant};
//!
//! #[derive(Clone)]
//! pub struct CircuitBreaker {
//!     failure_count: Arc<AtomicU32>,
//!     last_failure: Arc<parking_lot::Mutex<Option<Instant>>>,
//!     threshold: u32,
//!     timeout: Duration,
//! }
//!
//! impl CircuitBreaker {
//!     pub fn new(threshold: u32, timeout: Duration) -> Self {
//!         Self {
//!             failure_count: Arc::new(AtomicU32::new(0)),
//!             last_failure: Arc::new(parking_lot::Mutex::new(None)),
//!             threshold,
//!             timeout,
//!         }
//!     }
//!     
//!     pub async fn call<F, T>(&self, operation: F) -> BlixardResult<T>
//!     where
//!         F: FnOnce() -> BlixardResult<T>,
//!     {
//!         // Check if circuit is open
//!         if self.is_open() {
//!             return Err(BlixardError::TemporaryFailure {
//!                 details: "Circuit breaker is open".to_string(),
//!             });
//!         }
//!         
//!         match operation() {
//!             Ok(result) => {
//!                 self.on_success();
//!                 Ok(result)
//!             }
//!             Err(e) => {
//!                 self.on_failure();
//!                 Err(e)
//!             }
//!         }
//!     }
//!     
//!     fn is_open(&self) -> bool {
//!         let count = self.failure_count.load(Ordering::Relaxed);
//!         if count >= self.threshold {
//!             let last_failure = self.last_failure.lock();
//!             if let Some(time) = *last_failure {
//!                 time.elapsed() < self.timeout
//!             } else {
//!                 true
//!             }
//!         } else {
//!             false
//!         }
//!     }
//!     
//!     fn on_success(&self) {
//!         self.failure_count.store(0, Ordering::Relaxed);
//!     }
//!     
//!     fn on_failure(&self) {
//!         self.failure_count.fetch_add(1, Ordering::Relaxed);
//!         *self.last_failure.lock() = Some(Instant::now());
//!     }
//! }
//! ```
//!
//! ## Error Recovery Strategies
//!
//! ### Graceful Degradation
//! ```rust,no_run
//! use blixard_core::{BlixardError, BlixardResult};
//!
//! async fn get_cluster_status() -> BlixardResult<ClusterStatus> {
//!     // Try to get status from leader
//!     match get_status_from_leader().await {
//!         Ok(status) => Ok(status),
//!         Err(BlixardError::NotLeader { .. }) => {
//!             // Fallback to local status
//!             tracing::warn!("Leader unavailable, returning local status");
//!             get_local_status().await
//!         }
//!         Err(BlixardError::ConnectionError { .. }) => {
//!             // Network issue, use cached status
//!             tracing::warn!("Network error, using cached status");
//!             get_cached_status().await
//!         }
//!         Err(e) => Err(e), // Propagate other errors
//!     }
//! }
//! ```
//!
//! ### Resource Cleanup
//! ```rust,no_run
//! use blixard_core::{BlixardError, BlixardResult};
//!
//! async fn allocate_vm_resources(config: &VmConfig) -> BlixardResult<VmResources> {
//!     let mut allocated_resources = Vec::new();
//!     
//!     // Try to allocate each resource
//!     for resource_type in &config.required_resources {
//!         match allocate_resource(resource_type).await {
//!             Ok(resource) => allocated_resources.push(resource),
//!             Err(e) => {
//!                 // Clean up already allocated resources
//!                 for resource in allocated_resources {
//!                     if let Err(cleanup_error) = release_resource(resource).await {
//!                         tracing::error!("Failed to clean up resource: {}", cleanup_error);
//!                     }
//!                 }
//!                 return Err(e);
//!             }
//!         }
//!     }
//!     
//!     Ok(VmResources { resources: allocated_resources })
//! }
//! ```
//!
//! ## Error Monitoring and Alerting
//!
//! ### Structured Logging
//! ```rust,no_run
//! use tracing::{error, warn, info};
//! use blixard_core::BlixardError;
//!
//! fn log_error_with_context(error: &BlixardError, operation: &str, resource: &str) {
//!     match error {
//!         BlixardError::ResourceExhausted { resource } => {
//!             error!(
//!                 operation = operation,
//!                 resource = resource,
//!                 error_type = "resource_exhausted",
//!                 "Resource exhausted during operation"
//!             );
//!         }
//!         BlixardError::Timeout { operation, duration } => {
//!             warn!(
//!                 operation = operation,
//!                 timeout_ms = duration.as_millis(),
//!                 error_type = "timeout",
//!                 "Operation timed out"
//!             );
//!         }
//!         BlixardError::NotLeader { operation, leader_id } => {
//!             info!(
//!                 operation = operation,
//!                 leader_id = ?leader_id,
//!                 error_type = "not_leader",
//!                 "Operation requires leader"
//!             );
//!         }
//!         _ => {
//!             error!(
//!                 operation = operation,
//!                 resource = resource,
//!                 error = %error,
//!                 error_type = "unknown",
//!                 "Unexpected error during operation"
//!             );
//!         }
//!     }
//! }
//! ```
//!
//! ### Metrics Integration
//! ```rust,no_run
//! use blixard_core::BlixardError;
//! use opentelemetry::metrics::{Counter, Histogram};
//!
//! pub struct ErrorMetrics {
//!     error_count: Counter<u64>,
//!     error_duration: Histogram<f64>,
//! }
//!
//! impl ErrorMetrics {
//!     pub fn record_error(&self, error: &BlixardError, operation: &str, duration: Duration) {
//!         let error_type = match error {
//!             BlixardError::ResourceExhausted { .. } => "resource_exhausted",
//!             BlixardError::Timeout { .. } => "timeout",
//!             BlixardError::NotLeader { .. } => "not_leader",
//!             BlixardError::ValidationError { .. } => "validation",
//!             _ => "other",
//!         };
//!         
//!         self.error_count.add(1, &[
//!             ("operation", operation.into()),
//!             ("error_type", error_type.into()),
//!         ]);
//!         
//!         self.error_duration.record(duration.as_secs_f64(), &[
//!             ("operation", operation.into()),
//!             ("error_type", error_type.into()),
//!         ]);
//!     }
//! }
//! ```
//!
//! ## Testing Error Conditions
//!
//! ### Error Injection
//! ```rust,no_run
//! #[cfg(feature = "failpoints")]
//! use fail::fail_point;
//!
//! async fn save_data_with_failpoint(data: &[u8]) -> BlixardResult<()> {
//!     #[cfg(feature = "failpoints")]
//!     fail_point!("save_data_disk_full", |_| {
//!         Err(BlixardError::ResourceExhausted {
//!             resource: "disk_space".to_string(),
//!         })
//!     });
//!     
//!     // Normal implementation
//!     save_data_impl(data).await
//! }
//! 
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!     
//!     #[tokio::test]
//!     async fn test_disk_full_handling() {
//!         #[cfg(feature = "failpoints")]
//!         {
//!             fail::cfg("save_data_disk_full", "return").unwrap();
//!             
//!             let result = save_data_with_failpoint(b"test data").await;
//!             assert!(matches!(result, Err(BlixardError::ResourceExhausted { .. })));
//!             
//!             fail::cfg("save_data_disk_full", "off").unwrap();
//!         }
//!     }
//! }
//! ```

use thiserror::Error;

/// Comprehensive error type for all Blixard operations
///
/// This enum covers all possible error conditions in the Blixard system,
/// providing structured error information with proper context and recovery hints.
///
/// # Error Categories
///
/// - **System Errors**: Infrastructure failures (storage, network, IO)
/// - **Resource Errors**: Resource management issues (quotas, exhaustion, unavailability)
/// - **Logic Errors**: Application logic issues (validation, configuration, authorization)
/// - **Distributed Errors**: Consensus and cluster coordination failures
/// - **Operational Errors**: Runtime operational issues (timeouts, leadership, initialization)
/// - **Temporary Errors**: Transient failures that may succeed on retry
#[derive(Error, Debug)]
pub enum BlixardError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service already exists: {0}")]
    ServiceAlreadyExists(String),

    #[error("Failed to manage service: {0}")]
    ServiceManagementError(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] Box<redb::Error>),

    #[error("Storage transaction error: {0}")]
    StorageTransactionError(String),

    #[error("Storage table error: {0}")]
    StorageTableError(String),

    #[error("Raft error: {0}")]
    RaftError(#[from] Box<raft::Error>),

    #[error("Cluster error: {0}")]
    ClusterError(String),

    #[error("Failed to join cluster: {reason}")]
    ClusterJoin { reason: String },

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Connection error: {message}")]
    Connection { message: String },

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] Box<bincode::Error>),

    #[error("IO error: {0}")]
    IoError(#[from] Box<std::io::Error>),

    #[error("System error: {0}")]
    SystemError(String),

    #[error("Storage operation '{operation}' failed")]
    Storage {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Raft operation '{operation}' failed")]
    Raft {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Serialization operation '{operation}' failed")]
    Serialization {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("VM operation '{operation}' failed: {details}")]
    VmOperationFailed { operation: String, details: String },

    #[error("Scheduling error: {message}")]
    SchedulingError { message: String },

    #[error("Security error: {message}")]
    Security { message: String },

    #[error("Authorization error: {message}")]
    AuthorizationError { message: String },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Not initialized: {component}")]
    NotInitialized { component: String },

    #[error("Not leader for operation '{operation}', current leader: {leader_id:?}")]
    NotLeader {
        operation: String,
        leader_id: Option<u64>,
    },

    #[error("Invalid input for {field}: {message}")]
    InvalidInput { field: String, message: String },

    #[error("Validation error for {field}: {message}")]
    Validation { field: String, message: String },

    #[error("JSON error: {0}")]
    JsonError(#[from] Box<serde_json::Error>),

    #[error("gRPC error: {0}")]
    GrpcError(String),

    #[error("P2P error: {0}")]
    P2PError(String),

    #[error("Quota exceeded for {resource}: limit {limit}, requested {requested}")]
    QuotaExceeded {
        resource: String,
        limit: u64,
        requested: u64,
    },

    #[error("Node not found: {node_id}")]
    NodeNotFound { node_id: u64 },

    #[error("Insufficient resources: requested {requested}, available {available}")]
    InsufficientResources {
        requested: String,
        available: String,
    },

    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    #[error("Resource unavailable: {resource_type} - {message}")]
    ResourceUnavailable {
        resource_type: String,
        message: String,
    },

    #[error("Lock poisoned during {operation}")]
    LockPoisoned {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Already exists: {resource}")]
    AlreadyExists { resource: String },

    #[error("Invalid operation '{operation}': {reason}")]
    InvalidOperation { operation: String, reason: String },

    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },

    #[error("Operation timed out: {operation} after {duration:?}")]
    Timeout {
        operation: String,
        duration: std::time::Duration,
    },

    #[error("Connection error to {address}: {details}")]
    ConnectionError { address: String, details: String },

    #[error("Temporary failure: {details}")]
    TemporaryFailure { details: String },

    #[error("Database error: {operation} failed")]
    DatabaseError {
        operation: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Multiple errors in {context}: {}", format_errors(.errors))]
    Multiple {
        context: String,
        errors: Vec<BlixardError>,
    },
}

// Helper function to format multiple errors
fn format_errors(errors: &[BlixardError]) -> String {
    errors
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{}. {}", i + 1, e))
        .collect::<Vec<_>>()
        .join("; ")
}

pub type Result<T> = std::result::Result<T, BlixardError>;
pub type BlixardResult<T> = std::result::Result<T, BlixardError>;

impl BlixardError {
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
}

impl From<redb::TransactionError> for BlixardError {
    fn from(err: redb::TransactionError) -> Self {
        BlixardError::StorageTransactionError(err.to_string())
    }
}

impl From<redb::TableError> for BlixardError {
    fn from(err: redb::TableError) -> Self {
        BlixardError::StorageTableError(err.to_string())
    }
}

impl From<redb::StorageError> for BlixardError {
    fn from(err: redb::StorageError) -> Self {
        BlixardError::StorageError(Box::new(err.into()))
    }
}

impl From<redb::DatabaseError> for BlixardError {
    fn from(err: redb::DatabaseError) -> Self {
        BlixardError::StorageError(Box::new(err.into()))
    }
}

impl From<redb::CommitError> for BlixardError {
    fn from(err: redb::CommitError) -> Self {
        BlixardError::StorageError(Box::new(err.into()))
    }
}

// Additional From implementations for unboxed types
impl From<redb::Error> for BlixardError {
    fn from(err: redb::Error) -> Self {
        BlixardError::StorageError(Box::new(err))
    }
}

impl From<raft::Error> for BlixardError {
    fn from(err: raft::Error) -> Self {
        BlixardError::RaftError(Box::new(err))
    }
}

impl From<bincode::Error> for BlixardError {
    fn from(err: bincode::Error) -> Self {
        BlixardError::SerializationError(Box::new(err))
    }
}

impl From<std::io::Error> for BlixardError {
    fn from(err: std::io::Error) -> Self {
        BlixardError::IoError(Box::new(err))
    }
}

impl From<serde_json::Error> for BlixardError {
    fn from(err: serde_json::Error) -> Self {
        BlixardError::JsonError(Box::new(err))
    }
}

/// Helper functions for common error scenarios
impl BlixardError {
    /// Create an internal error for lock poisoning scenarios
    /// 
    /// This helper provides a consistent error message for lock poisoning
    /// without requiring Send/Sync bounds on the poison error.
    pub fn lock_poisoned_internal(resource: &str) -> Self {
        BlixardError::Internal {
            message: format!("Lock poisoned: {}", resource),
        }
    }

    /// Create an error for unavailable services or components
    pub fn service_unavailable(service: &str) -> Self {
        BlixardError::NotInitialized {
            component: service.to_string(),
        }
    }

    /// Create an error for invalid configuration values
    pub fn invalid_config<T: std::fmt::Display>(field: &str, value: T, reason: &str) -> Self {
        BlixardError::InvalidConfiguration {
            message: format!("Invalid {} '{}': {}", field, value, reason),
        }
    }
}
