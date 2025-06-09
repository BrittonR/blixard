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
    
    #[error("Cluster error: {0}")]
    ClusterError(String),
    
    #[error("Node error: {0}")]
    NodeError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] tonic::transport::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("System error: {0}")]
    SystemError(String),
}

pub type Result<T> = std::result::Result<T, BlixardError>;