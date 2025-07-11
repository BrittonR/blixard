use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlixardError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service already exists: {0}")]
    ServiceAlreadyExists(String),

    #[error("Failed to manage service: {0}")]
    ServiceManagementError(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] redb::Error),

    #[error("Storage transaction error: {0}")]
    StorageTransactionError(String),

    #[error("Storage table error: {0}")]
    StorageTableError(String),

    #[error("Raft error: {0}")]
    RaftError(#[from] raft::Error),

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
    SerializationError(#[from] bincode::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

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
    
    #[error("Invalid input for {field}: {message}")]
    InvalidInput { field: String, message: String },
    
    #[error("Validation error for {field}: {message}")]
    Validation { field: String, message: String },

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

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
        message: String 
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
        duration: std::time::Duration 
    },

    #[error("Connection error to {address}: {details}")]
    ConnectionError { 
        address: String, 
        details: String 
    },

    #[error("Temporary failure: {details}")]
    TemporaryFailure { 
        details: String 
    },

    #[error("Database error: {operation} failed")]
    DatabaseError { 
        operation: String, 
        #[source] 
        source: Box<dyn std::error::Error + Send + Sync> 
    },
}

pub type Result<T> = std::result::Result<T, BlixardError>;
pub type BlixardResult<T> = std::result::Result<T, BlixardError>;

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
        BlixardError::StorageError(err.into())
    }
}

impl From<redb::DatabaseError> for BlixardError {
    fn from(err: redb::DatabaseError) -> Self {
        BlixardError::StorageError(err.into())
    }
}

impl From<redb::CommitError> for BlixardError {
    fn from(err: redb::CommitError) -> Self {
        BlixardError::StorageError(err.into())
    }
}
