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
    
    /// Convert to Status with proper error mapping (for gRPC services when available)
    fn to_status_string(self) -> Result<T, String>;
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
    
    fn to_status_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
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

// =============================================================================
// Domain-Specific Error Context Helpers
// =============================================================================

/// Error context helpers for storage operations
pub trait StorageContext<T> {
    /// Add context for database operations
    fn storage_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for Raft storage operations
    fn raft_storage_context(self, operation: &str) -> raft::Result<T>;
    
    /// Add context for file operations with path information
    fn file_context(self, operation: &str, path: &std::path::Path) -> BlixardResult<T>;
    
    /// Add context for database transactions
    fn transaction_context(self, operation: &str) -> BlixardResult<T>;
}

impl<T, E> StorageContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn storage_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Storage {
            operation: operation.to_string(),
            source: e.into(),
        })
    }
    
    fn raft_storage_context(self, _operation: &str) -> raft::Result<T> {
        self.map_err(|e| {
            let boxed_error = e.into();
            raft::Error::Store(raft::StorageError::Other(boxed_error))
        })
    }
    
    fn file_context(self, operation: &str, path: &std::path::Path) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Storage {
            operation: format!("{} (path: {})", operation, path.display()),
            source: e.into(),
        })
    }
    
    fn transaction_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Storage {
            operation: format!("transaction: {}", operation),
            source: e.into(),
        })
    }
}

/// Error context helpers for serialization operations
pub trait SerializationContext<T> {
    /// Add context for serialization operations
    fn serialize_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for deserialization operations with data type info
    fn deserialize_context(self, operation: &str, data_type: &str) -> BlixardResult<T>;
    
    /// Add context for JSON operations
    fn json_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for binary codec operations
    fn bincode_context(self, operation: &str) -> BlixardResult<T>;
}

impl<T, E> SerializationContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn serialize_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Serialization {
            operation: format!("serialize {}", operation),
            source: e.into(),
        })
    }
    
    fn deserialize_context(self, operation: &str, data_type: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Serialization {
            operation: format!("deserialize {} ({})", operation, data_type),
            source: e.into(),
        })
    }
    
    fn json_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Serialization {
            operation: format!("JSON {}", operation),
            source: e.into(),
        })
    }
    
    fn bincode_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Serialization {
            operation: format!("bincode {}", operation),
            source: e.into(),
        })
    }
}

/// Error context helpers for network operations
pub trait NetworkContext<T> {
    /// Add context for general network operations
    fn network_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for P2P operations with optional peer info
    fn p2p_context(self, operation: &str, peer_id: Option<&str>) -> BlixardResult<T>;
    
    /// Add context for connection operations
    fn connection_context(self, operation: &str, address: &str) -> BlixardResult<T>;
    
    /// Add context for Iroh transport operations
    fn iroh_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for HTTP operations
    fn http_context(self, operation: &str, url: Option<&str>) -> BlixardResult<T>;
}

impl<T, E> NetworkContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn network_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::NetworkError(
            format!("Network {}: {}", operation, e.into())
        ))
    }
    
    fn p2p_context(self, operation: &str, peer_id: Option<&str>) -> BlixardResult<T> {
        let context = match peer_id {
            Some(id) => format!("P2P {} (peer: {})", operation, id),
            None => format!("P2P {}", operation),
        };
        self.map_err(|e| BlixardError::NetworkError(
            format!("{}: {}", context, e.into())
        ))
    }
    
    fn connection_context(self, operation: &str, address: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::NetworkError(
            format!("Connection {} to {}: {}", operation, address, e.into())
        ))
    }
    
    fn iroh_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::NetworkError(
            format!("Iroh {}: {}", operation, e.into())
        ))
    }
    
    fn http_context(self, operation: &str, url: Option<&str>) -> BlixardResult<T> {
        let context = match url {
            Some(u) => format!("HTTP {} ({})", operation, u),
            None => format!("HTTP {}", operation),
        };
        self.map_err(|e| BlixardError::NetworkError(
            format!("{}: {}", context, e.into())
        ))
    }
}

/// Error context helpers for VM operations
pub trait VmContext<T> {
    /// Add context for VM lifecycle operations
    fn vm_context(self, operation: &str, vm_name: &str) -> BlixardResult<T>;
    
    /// Add context for VM backend operations
    fn vm_backend_context(self, operation: &str, backend: &str) -> BlixardResult<T>;
    
    /// Add context for VM process operations
    fn vm_process_context(self, operation: &str, vm_name: &str) -> BlixardResult<T>;
    
    /// Add context for VM configuration operations
    fn vm_config_context(self, operation: &str, vm_name: &str) -> BlixardResult<T>;
    
    /// Add context for VM scheduling operations
    fn vm_scheduling_context(self, operation: &str, strategy: Option<&str>) -> BlixardResult<T>;
}

impl<T, E> VmContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn vm_context(self, operation: &str, vm_name: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::VmOperationFailed {
            operation: operation.to_string(),
            details: format!("VM '{}': {}", vm_name, e.into()),
        })
    }
    
    fn vm_backend_context(self, operation: &str, backend: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::VmOperationFailed {
            operation: format!("{} ({})", operation, backend),
            details: format!("Backend error: {}", e.into()),
        })
    }
    
    fn vm_process_context(self, operation: &str, vm_name: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::VmOperationFailed {
            operation: format!("process {}", operation),
            details: format!("VM '{}': {}", vm_name, e.into()),
        })
    }
    
    fn vm_config_context(self, operation: &str, vm_name: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::VmOperationFailed {
            operation: format!("config {}", operation),
            details: format!("VM '{}': {}", vm_name, e.into()),
        })
    }
    
    fn vm_scheduling_context(self, operation: &str, strategy: Option<&str>) -> BlixardResult<T> {
        self.map_err(|e| {
            let details = match strategy {
                Some(s) => format!("Scheduling {} (strategy: {}): {}", operation, s, e.into()),
                None => format!("Scheduling {}: {}", operation, e.into()),
            };
            BlixardError::SchedulingError {
                message: details,
            }
        })
    }
}

/// Error context helpers for Raft operations
pub trait RaftContext<T> {
    /// Add context for Raft consensus operations
    fn raft_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for Raft proposal operations
    fn raft_proposal_context(self, operation: &str, proposal_type: &str) -> BlixardResult<T>;
    
    /// Add context for Raft configuration changes
    fn raft_config_context(self, operation: &str, node_id: Option<u64>) -> BlixardResult<T>;
}

impl<T, E> RaftContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn raft_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Raft {
            operation: operation.to_string(),
            source: e.into(),
        })
    }
    
    fn raft_proposal_context(self, operation: &str, proposal_type: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Raft {
            operation: format!("{} ({})", operation, proposal_type),
            source: e.into(),
        })
    }
    
    fn raft_config_context(self, operation: &str, node_id: Option<u64>) -> BlixardResult<T> {
        let op_desc = match node_id {
            Some(id) => format!("config {} (node: {})", operation, id),
            None => format!("config {}", operation),
        };
        self.map_err(|e| BlixardError::Raft {
            operation: op_desc,
            source: e.into(),
        })
    }
}

/// Error context helpers for authentication and security operations
pub trait SecurityContext<T> {
    /// Add context for authentication operations
    fn auth_context(self, operation: &str, user: Option<&str>) -> BlixardResult<T>;
    
    /// Add context for authorization operations  
    fn authz_context(self, operation: &str, resource: &str) -> BlixardResult<T>;
    
    /// Add context for certificate operations
    fn cert_context(self, operation: &str) -> BlixardResult<T>;
    
    /// Add context for token operations
    fn token_context(self, operation: &str) -> BlixardResult<T>;
}

impl<T, E> SecurityContext<T> for Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn auth_context(self, operation: &str, user: Option<&str>) -> BlixardResult<T> {
        let context = match user {
            Some(u) => format!("Authentication {} (user: {})", operation, u),
            None => format!("Authentication {}", operation),
        };
        self.map_err(|e| BlixardError::Security {
            message: format!("{}: {}", context, e.into()),
        })
    }
    
    fn authz_context(self, operation: &str, resource: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Security {
            message: format!("Authorization {} (resource: {}): {}", operation, resource, e.into()),
        })
    }
    
    fn cert_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Security {
            message: format!("Certificate {}: {}", operation, e.into()),
        })
    }
    
    fn token_context(self, operation: &str) -> BlixardResult<T> {
        self.map_err(|e| BlixardError::Security {
            message: format!("Token {}: {}", operation, e.into()),
        })
    }
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
    
    #[test]
    fn test_storage_context() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        
        let with_context = result.storage_context("create database");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Storage { operation, .. } => {
                assert_eq!(operation, "create database");
            }
            _ => panic!("Expected Storage error"),
        }
    }
    
    #[test]
    fn test_file_context() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        
        let path = std::path::Path::new("/tmp/test.db");
        let with_context = result.file_context("read file", path);
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Storage { operation, .. } => {
                assert!(operation.contains("read file"));
                assert!(operation.contains("/tmp/test.db"));
            }
            _ => panic!("Expected Storage error"),
        }
    }
    
    #[test]
    fn test_serialization_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("serialization failed".into());
        
        let with_context = result.serialize_context("vm_config");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Serialization { operation, .. } => {
                assert_eq!(operation, "serialize vm_config");
            }
            _ => panic!("Expected Serialization error"),
        }
    }
    
    #[test]
    fn test_deserialize_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("deserialization failed".into());
        
        let with_context = result.deserialize_context("vm_state", "VmState");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Serialization { operation, .. } => {
                assert_eq!(operation, "deserialize vm_state (VmState)");
            }
            _ => panic!("Expected Serialization error"),
        }
    }
    
    #[test]
    fn test_network_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("connection failed".into());
        
        let with_context = result.network_context("connect to peer");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::NetworkError(msg) => {
                assert!(msg.contains("Network connect to peer"));
                assert!(msg.contains("connection failed"));
            }
            _ => panic!("Expected NetworkError"),
        }
    }
    
    #[test]
    fn test_p2p_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("peer unreachable".into());
        
        let with_context = result.p2p_context("send message", Some("node123"));
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::NetworkError(msg) => {
                assert!(msg.contains("P2P send message"));
                assert!(msg.contains("peer: node123"));
                assert!(msg.contains("peer unreachable"));
            }
            _ => panic!("Expected NetworkError"),
        }
    }
    
    #[test]
    fn test_vm_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("start failed".into());
        
        let with_context = result.vm_context("start", "test-vm");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::VmOperationFailed { operation, details } => {
                assert_eq!(operation, "start");
                assert!(details.contains("VM 'test-vm'"));
                assert!(details.contains("start failed"));
            }
            _ => panic!("Expected VmOperationFailed error"),
        }
    }
    
    #[test]
    fn test_vm_backend_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("backend error".into());
        
        let with_context = result.vm_backend_context("create", "microvm");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::VmOperationFailed { operation, details } => {
                assert_eq!(operation, "create (microvm)");
                assert!(details.contains("Backend error"));
            }
            _ => panic!("Expected VmOperationFailed error"),
        }
    }
    
    #[test]
    fn test_raft_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("consensus failed".into());
        
        let with_context = result.raft_context("propose entry");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Raft { operation, .. } => {
                assert_eq!(operation, "propose entry");
            }
            _ => panic!("Expected Raft error"),
        }
    }
    
    #[test]
    fn test_raft_proposal_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("proposal rejected".into());
        
        let with_context = result.raft_proposal_context("submit", "add_vm");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Raft { operation, .. } => {
                assert_eq!(operation, "submit (add_vm)");
            }
            _ => panic!("Expected Raft error"),
        }
    }
    
    #[test]
    fn test_security_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("invalid credentials".into());
        
        let with_context = result.auth_context("login", Some("user123"));
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::Security { message } => {
                assert!(message.contains("Authentication login"));
                assert!(message.contains("user: user123"));
                assert!(message.contains("invalid credentials"));
            }
            _ => panic!("Expected Security error"),
        }
    }
    
    #[test]
    fn test_connection_context() {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = 
            Err("timeout".into());
        
        let with_context = result.connection_context("establish", "127.0.0.1:8080");
        assert!(with_context.is_err());
        match with_context.unwrap_err() {
            BlixardError::NetworkError(msg) => {
                assert!(msg.contains("Connection establish to 127.0.0.1:8080"));
                assert!(msg.contains("timeout"));
            }
            _ => panic!("Expected NetworkError"),
        }
    }
}