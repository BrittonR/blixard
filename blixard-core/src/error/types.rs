//! Core error types for Blixard
//!
//! This module contains the main BlixardError enum with all error variants
//! and associated type aliases for Result types.

use thiserror::Error;

/// Comprehensive error type for Blixard operations
///
/// BlixardError provides structured error information designed for distributed systems,
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
    // Service Management
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service already exists: {0}")]
    ServiceAlreadyExists(String),

    #[error("Failed to manage service: {0}")]
    ServiceManagementError(String),

    // Storage Errors
    #[error("Storage error: {0}")]
    StorageError(#[from] Box<redb::Error>),

    #[error("Storage transaction error: {0}")]
    StorageTransactionError(String),

    #[error("Storage table error: {0}")]
    StorageTableError(String),

    // Raft & Cluster Errors
    #[error("Raft error: {0}")]
    RaftError(#[from] Box<raft::Error>),

    #[error("Cluster error: {0}")]
    ClusterError(String),

    #[error("Failed to join cluster: {reason}")]
    ClusterJoin { reason: String },

    #[error("Node error: {0}")]
    NodeError(String),

    // Configuration Errors
    #[error("Configuration error in {component}: {message}")]
    ConfigurationError {
        component: String,
        message: String,
    },

    #[deprecated(since = "0.1.0", note = "Use ConfigurationError instead")]
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[deprecated(since = "0.1.0", note = "Use ConfigurationError instead")]
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    // Network & Connection Errors
    #[error("Connection error: {message}")]
    Connection { message: String },

    #[error("Network error: {0}")]
    NetworkError(String),

    // Serialization & IO Errors
    #[error("Serialization error: {0}")]
    SerializationError(#[from] Box<bincode::Error>),

    #[error("IO error: {0}")]
    IoError(#[from] Box<std::io::Error>),

    #[error("System error: {0}")]
    SystemError(String),

    // Boxed Error Variants
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

    // Internal & Implementation Errors
    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("VM operation '{operation}' failed: {details}")]
    VmOperationFailed { operation: String, details: String },

    #[error("Scheduling error: {message}")]
    SchedulingError { message: String },

    // Security & Authorization
    #[error("Security error: {message}")]
    Security { message: String },

    #[error("Authorization error: {message}")]
    AuthorizationError { message: String },

    // Resource Management
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Not initialized: {component}")]
    NotInitialized { component: String },

    #[error("Not leader for operation '{operation}', current leader: {leader_id:?}")]
    NotLeader {
        operation: String,
        leader_id: Option<u64>,
    },

    #[deprecated(since = "0.1.0", note = "Use ConfigurationError instead")]
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

    #[deprecated(since = "0.1.0", note = "Use ConfigurationError instead")]
    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },

    // Operational Errors
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

    // Multiple Errors
    #[error("Multiple errors in {context}: {}", format_errors(.errors))]
    Multiple {
        context: String,
        errors: Vec<BlixardError>,
    },
}

// Helper function to format multiple errors
pub fn format_errors(errors: &[BlixardError]) -> String {
    errors
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{}. {}", i + 1, e))
        .collect::<Vec<_>>()
        .join("; ")
}

pub type Result<T> = std::result::Result<T, BlixardError>;
pub type BlixardResult<T> = std::result::Result<T, BlixardError>;