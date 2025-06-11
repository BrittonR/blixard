// Property-based testing for error types and conversions

use proptest::prelude::*;
use blixard::error::{BlixardError, BlixardResult};
use std::error::Error;

mod common;

// Strategy for generating error messages
fn error_message_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_\\s\\-\\.]{1,100}".prop_map(|s| s.trim().to_string())
        .prop_filter("Non-empty message", |s| !s.is_empty())
}

// Strategy for generating feature names
fn feature_name_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_\\-]{1,50}"
}

// Strategy for generating operation names  
fn operation_name_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_\\s]{1,100}"
}

// Property: All error variants should display non-empty messages
proptest! {
    #[test]
    fn test_error_display_not_empty(
        service_name in "[a-zA-Z0-9_\\-]{1,50}",
        message in error_message_strategy(),
        feature in feature_name_strategy(),
        operation in operation_name_strategy()
    ) {
        let errors = vec![
            BlixardError::ServiceNotFound(service_name.clone()),
            BlixardError::ServiceAlreadyExists(service_name.clone()),
            BlixardError::ServiceManagementError(message.clone()),
            BlixardError::ClusterError(message.clone()),
            BlixardError::NodeError(message.clone()),
            BlixardError::ConfigError(message.clone()),
            BlixardError::SystemError(message.clone()),
            BlixardError::Internal { message: message.clone() },
            BlixardError::NotImplemented { feature: feature.clone() },
        ];
        
        for error in errors {
            let display_str = format!("{}", error);
            prop_assert!(!display_str.is_empty());
            prop_assert!(display_str.len() > 5); // Should have meaningful content
        }
    }
}

// Property: NotImplemented errors should contain the feature name
proptest! {
    #[test]
    fn test_not_implemented_contains_feature(feature in feature_name_strategy()) {
        let error = BlixardError::NotImplemented { feature: feature.clone() };
        let error_str = format!("{}", error);
        
        prop_assert!(error_str.contains(&feature));
        prop_assert!(error_str.contains("not implemented"));
    }
}

// Property: Service errors should contain the service name
proptest! {
    #[test]
    fn test_service_errors_contain_name(service_name in "[a-zA-Z0-9_\\-]{1,50}") {
        let not_found = BlixardError::ServiceNotFound(service_name.clone());
        let already_exists = BlixardError::ServiceAlreadyExists(service_name.clone());
        
        let not_found_str = format!("{}", not_found);
        let already_exists_str = format!("{}", already_exists);
        
        prop_assert!(not_found_str.contains(&service_name));
        prop_assert!(already_exists_str.contains(&service_name));
        prop_assert!(not_found_str.contains("not found"));
        prop_assert!(already_exists_str.contains("already exists"));
    }
}

// Property: Structured errors should contain operation names
proptest! {
    #[test]
    fn test_structured_errors_contain_operation(operation in operation_name_strategy()) {
        let storage_error = BlixardError::Storage {
            operation: operation.clone(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "test")),
        };
        
        let raft_error = BlixardError::Raft {
            operation: operation.clone(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "test")),
        };
        
        let serialization_error = BlixardError::Serialization {
            operation: operation.clone(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        };
        
        for error in [storage_error, raft_error, serialization_error] {
            let error_str = format!("{}", error);
            prop_assert!(error_str.contains(&operation));
            prop_assert!(error_str.contains("failed"));
        }
    }
}

// Property: Error conversion preserves information
proptest! {
    #[test]
    fn test_io_error_conversion_preserves_info(
        kind in prop::sample::select(&[
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidData,
            std::io::ErrorKind::Other,
        ]),
        message in error_message_strategy()
    ) {
        let io_error = std::io::Error::new(kind, message.clone());
        let blixard_error: BlixardError = io_error.into();
        
        match blixard_error {
            BlixardError::IoError(inner) => {
                prop_assert_eq!(inner.kind(), kind);
                prop_assert!(inner.to_string().contains(&message));
            },
            _ => prop_assert!(false, "Expected IoError variant"),
        }
    }
}

// Property: Error result types work correctly
proptest! {
    #[test]
    fn test_result_type_usage(
        success_value in "[a-zA-Z0-9]{1,20}",
        error_message in error_message_strategy()
    ) {
        fn test_success(value: String) -> BlixardResult<String> {
            Ok(value)
        }
        
        fn test_failure(msg: String) -> BlixardResult<String> {
            Err(BlixardError::SystemError(msg))
        }
        
        let success_result = test_success(success_value.clone());
        let failure_result = test_failure(error_message.clone());
        
        prop_assert!(success_result.is_ok());
        prop_assert_eq!(success_result.unwrap(), success_value);
        
        prop_assert!(failure_result.is_err());
        let error = failure_result.unwrap_err();
        let error_str = format!("{}", error);
        prop_assert!(error_str.contains(&error_message));
    }
}

// Property: Error debug formatting is comprehensive
proptest! {
    #[test]
    fn test_error_debug_formatting(
        feature in feature_name_strategy(),
        message in error_message_strategy()
    ) {
        let errors = vec![
            BlixardError::NotImplemented { feature: feature.clone() },
            BlixardError::Internal { message: message.clone() },
            BlixardError::SystemError(message.clone()),
        ];
        
        for error in errors {
            let debug_str = format!("{:?}", error);
            prop_assert!(!debug_str.is_empty());
            prop_assert!(debug_str.len() > 10); // Debug should be detailed
        }
    }
}

// Property: Error chaining works for all structured errors
proptest! {
    #[test]
    fn test_error_chaining_properties(operation in operation_name_strategy()) {
        
        let errors = vec![
            BlixardError::Storage {
                operation: operation.clone(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "storage error")),
            },
            BlixardError::Raft {
                operation: operation.clone(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "raft error")),
            },
            BlixardError::Serialization {
                operation: operation.clone(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "ser error")),
            },
        ];
        
        for error in errors {
            prop_assert!(error.source().is_some());
            let source = error.source().unwrap();
            prop_assert!(!source.to_string().is_empty());
        }
    }
}