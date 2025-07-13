//! Convenience macros for error handling
//!
//! These macros provide shorthand for common error conversion patterns,
//! building on top of the error_context traits.

/// Convert an error to Internal with a formatted message
#[macro_export]
macro_rules! internal_err {
    ($result:expr, $($arg:tt)*) => {
        $result.map_err(|e| $crate::error::BlixardError::Internal {
            message: format!($($arg)*, e),
        })
    };
}

/// Convert an error to Storage error with operation context
#[macro_export]
macro_rules! storage_err {
    ($result:expr, $operation:expr) => {
        $result.map_err(|e| $crate::error::BlixardError::Storage {
            operation: $operation.to_string(),
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })
    };
}

/// Convert an error to NetworkError with formatted message
#[macro_export]
macro_rules! network_err {
    ($result:expr, $($arg:tt)*) => {
        $result.map_err(|e| $crate::error::BlixardError::NetworkError(
            format!($($arg)*, e)
        ))
    };
}

/// Convert an error to VmOperationFailed
#[macro_export]
macro_rules! vm_err {
    ($result:expr, $operation:expr, $vm_name:expr) => {
        $result.map_err(|e| $crate::error::BlixardError::VmOperationFailed {
            operation: $operation.to_string(),
            details: format!("VM '{}': {}", $vm_name, e),
        })
    };
    ($result:expr, $operation:expr) => {
        $result.map_err(|e| $crate::error::BlixardError::VmOperationFailed {
            operation: $operation.to_string(),
            details: format!("{}", e),
        })
    };
}

/// Convert an error to ConfigError with formatted message
#[macro_export]
macro_rules! config_err {
    ($result:expr, $($arg:tt)*) => {
        $result.map_err(|e| $crate::error::BlixardError::ConfigError(
            format!($($arg)*, e)
        ))
    };
}

/// Convert an error to Security error
#[macro_export]
macro_rules! security_err {
    ($result:expr, $($arg:tt)*) => {
        $result.map_err(|e| $crate::error::BlixardError::Security {
            message: format!($($arg)*, e),
        })
    };
}

/// Convert None to NotFound error
#[macro_export]
macro_rules! or_not_found {
    ($option:expr, $resource:expr) => {
        $option.ok_or_else(|| $crate::error::BlixardError::NotFound {
            resource: $resource.to_string(),
        })
    };
}

/// Convert None to Internal error with message
#[macro_export]
macro_rules! or_internal {
    ($option:expr, $($arg:tt)*) => {
        $option.ok_or_else(|| $crate::error::BlixardError::Internal {
            message: format!($($arg)*),
        })
    };
}

/// Propagate error with additional context logging
#[macro_export]
macro_rules! context_bail {
    ($result:expr, $context:expr) => {
        match $result {
            Ok(val) => val,
            Err(e) => {
                tracing::error!("{}: {}", $context, e);
                return Err(e.into());
            }
        }
    };
}

/// Log and ignore error
#[macro_export]
macro_rules! log_error {
    ($result:expr, $($arg:tt)*) => {
        if let Err(e) = $result {
            tracing::error!($($arg)*, error = %e);
        }
    };
}

/// Log warning and ignore error
#[macro_export]
macro_rules! log_warn {
    ($result:expr, $($arg:tt)*) => {
        if let Err(e) = $result {
            tracing::warn!($($arg)*, error = %e);
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::error::{BlixardError, BlixardResult};
    
    fn failing_operation() -> Result<String, std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "test error",
        ))
    }
    
    #[test]
    fn test_internal_err_macro() {
        let result = internal_err!(failing_operation(), "Failed to read file: {}");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::Internal { message } => {
                assert!(message.contains("Failed to read file"));
                assert!(message.contains("test error"));
            }
            _ => panic!("Expected Internal error"),
        }
    }
    
    #[test]
    fn test_storage_err_macro() {
        let result = storage_err!(failing_operation(), "create_database");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::Storage { operation, .. } => {
                assert_eq!(operation, "create_database");
            }
            _ => panic!("Expected Storage error"),
        }
    }
    
    #[test]
    fn test_network_err_macro() {
        let result = network_err!(failing_operation(), "Connection failed: {}");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::NetworkError(msg) => {
                assert!(msg.contains("Connection failed"));
                assert!(msg.contains("test error"));
            }
            _ => panic!("Expected NetworkError"),
        }
    }
    
    #[test]
    fn test_vm_err_macro() {
        let result = vm_err!(failing_operation(), "start", "test-vm");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::VmOperationFailed { operation, details } => {
                assert_eq!(operation, "start");
                assert!(details.contains("VM 'test-vm'"));
                assert!(details.contains("test error"));
            }
            _ => panic!("Expected VmOperationFailed error"),
        }
    }
    
    #[test]
    fn test_or_not_found_macro() {
        let option: Option<String> = None;
        let result = or_not_found!(option, "user");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::NotFound { resource } => {
                assert_eq!(resource, "user");
            }
            _ => panic!("Expected NotFound error"),
        }
    }
    
    #[test]
    fn test_or_internal_macro() {
        let option: Option<String> = None;
        let result = or_internal!(option, "Database not initialized");
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::Internal { message } => {
                assert_eq!(message, "Database not initialized");
            }
            _ => panic!("Expected Internal error"),
        }
    }
}