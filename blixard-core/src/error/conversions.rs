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

// Async/Task error conversions
impl From<tokio::task::JoinError> for BlixardError {
    fn from(err: tokio::task::JoinError) -> Self {
        if err.is_cancelled() {
            BlixardError::Internal {
                message: "Task was cancelled".to_string(),
            }
        } else if err.is_panic() {
            BlixardError::Internal {
                message: "Task panicked".to_string(),
            }
        } else {
            BlixardError::Internal {
                message: format!("Task join failed: {}", err),
            }
        }
    }
}

impl From<tokio::time::error::Elapsed> for BlixardError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        BlixardError::Timeout {
            operation: "async_operation".to_string(),
            duration: std::time::Duration::from_secs(0), // Duration not available from Elapsed
        }
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for BlixardError {
    fn from(err: tokio::sync::oneshot::error::RecvError) -> Self {
        BlixardError::Internal {
            message: format!("Channel receive failed: {}", err),
        }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for BlixardError
where
    T: std::fmt::Debug,
{
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        BlixardError::Internal {
            message: format!("Channel send failed: {:?}", err),
        }
    }
}

impl From<tokio::sync::broadcast::error::SendError<String>> for BlixardError {
    fn from(err: tokio::sync::broadcast::error::SendError<String>) -> Self {
        BlixardError::Internal {
            message: format!("Broadcast send failed: {}", err),
        }
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for BlixardError {
    fn from(err: tokio::sync::broadcast::error::RecvError) -> Self {
        use tokio::sync::broadcast::error::RecvError;
        match err {
            RecvError::Closed => BlixardError::Internal {
                message: "Broadcast channel closed".to_string(),
            },
            RecvError::Lagged(count) => BlixardError::Internal {
                message: format!("Broadcast channel lagged by {} messages", count),
            },
        }
    }
}

impl From<tokio::sync::watch::error::SendError<String>> for BlixardError {
    fn from(err: tokio::sync::watch::error::SendError<String>) -> Self {
        BlixardError::Internal {
            message: format!("Watch send failed: {}", err),
        }
    }
}

impl From<tokio::sync::AcquireError> for BlixardError {
    fn from(err: tokio::sync::AcquireError) -> Self {
        BlixardError::Internal {
            message: format!("Semaphore acquire failed: {}", err),
        }
    }
}

// HTTP/Network error conversions
impl From<reqwest::Error> for BlixardError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            BlixardError::Timeout {
                operation: "http_request".to_string(),
                duration: std::time::Duration::from_secs(30), // Default timeout assumption
            }
        } else if err.is_connect() {
            BlixardError::ConnectionError {
                address: err.url().map(|u| u.to_string()).unwrap_or_else(|| "unknown".to_string()),
                details: format!("Connection failed: {}", err),
            }
        } else {
            BlixardError::NetworkError(format!("HTTP request failed: {}", err))
        }
    }
}

impl From<hyper::Error> for BlixardError {
    fn from(err: hyper::Error) -> Self {
        if err.is_timeout() {
            BlixardError::Timeout {
                operation: "hyper_request".to_string(),
                duration: std::time::Duration::from_secs(30), // Default timeout assumption
            }
        } else if err.is_connect() {
            BlixardError::ConnectionError {
                address: "unknown".to_string(),
                details: format!("Hyper connection failed: {}", err),
            }
        } else {
            BlixardError::NetworkError(format!("Hyper error: {}", err))
        }
    }
}

// Tracing error conversions
impl From<tracing::dispatcher::SetGlobalDefaultError> for BlixardError {
    fn from(err: tracing::dispatcher::SetGlobalDefaultError) -> Self {
        BlixardError::ConfigurationError {
            component: "tracing".to_string(),
            message: format!("Failed to set global tracing subscriber: {}", err),
        }
    }
}

// String parsing and format error conversions
impl From<std::string::FromUtf8Error> for BlixardError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        BlixardError::Serialization {
            operation: "utf8_conversion".to_string(),
            source: Box::new(err),
        }
    }
}

impl From<std::str::Utf8Error> for BlixardError {
    fn from(err: std::str::Utf8Error) -> Self {
        BlixardError::ConfigurationError {
            component: "utf8_parsing".to_string(),
            message: format!("Invalid UTF-8 data: {}", err),
        }
    }
}