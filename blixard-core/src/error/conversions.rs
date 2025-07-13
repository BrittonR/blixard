//! Error conversion implementations for BlixardError
//!
//! This module provides From trait implementations for converting
//! external error types into BlixardError variants.

use super::types::BlixardError;

// Storage error conversions
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
        BlixardError::Storage {
            operation: "storage_operation".to_string(),
            source: Box::new(err),
        }
    }
}

impl From<redb::DatabaseError> for BlixardError {
    fn from(err: redb::DatabaseError) -> Self {
        BlixardError::DatabaseError {
            operation: "database_operation".to_string(),
            source: Box::new(err),
        }
    }
}

impl From<redb::CommitError> for BlixardError {
    fn from(err: redb::CommitError) -> Self {
        BlixardError::Storage {
            operation: "commit".to_string(),
            source: Box::new(err),
        }
    }
}

impl From<redb::Error> for BlixardError {
    fn from(err: redb::Error) -> Self {
        BlixardError::StorageError(Box::new(err))
    }
}

// Consensus error conversions
impl From<raft::Error> for BlixardError {
    fn from(err: raft::Error) -> Self {
        BlixardError::RaftError(Box::new(err))
    }
}

// Serialization error conversions
impl From<bincode::Error> for BlixardError {
    fn from(err: bincode::Error) -> Self {
        BlixardError::SerializationError(Box::new(err))
    }
}

impl From<serde_json::Error> for BlixardError {
    fn from(err: serde_json::Error) -> Self {
        BlixardError::JsonError(Box::new(err))
    }
}

// System error conversions
impl From<std::io::Error> for BlixardError {
    fn from(err: std::io::Error) -> Self {
        BlixardError::IoError(Box::new(err))
    }
}

impl From<std::net::AddrParseError> for BlixardError {
    fn from(err: std::net::AddrParseError) -> Self {
        BlixardError::ConfigurationError {
            component: "network_address".to_string(),
            message: format!("Invalid network address: {}", err),
        }
    }
}

impl From<std::num::ParseIntError> for BlixardError {
    fn from(err: std::num::ParseIntError) -> Self {
        BlixardError::ConfigurationError {
            component: "numeric_value".to_string(),
            message: format!("Invalid numeric value: {}", err),
        }
    }
}

impl From<uuid::Error> for BlixardError {
    fn from(err: uuid::Error) -> Self {
        BlixardError::ConfigurationError {
            component: "uuid".to_string(),
            message: format!("Invalid UUID: {}", err),
        }
    }
}