// Error type and conversion tests

use blixard::error::{BlixardError, BlixardResult};
use std::error::Error;

mod common;

#[test]
fn test_error_display_formatting() {
    let errors = vec![
        BlixardError::ServiceNotFound("test-service".to_string()),
        BlixardError::ServiceAlreadyExists("existing-service".to_string()),
        BlixardError::ServiceManagementError("management failed".to_string()),
        BlixardError::ClusterError("cluster issue".to_string()),
        BlixardError::NodeError("node problem".to_string()),
        BlixardError::ConfigError("invalid config".to_string()),
        BlixardError::SystemError("system failure".to_string()),
        BlixardError::Internal { message: "internal error".to_string() },
        BlixardError::NotImplemented { feature: "test feature".to_string() },
    ];
    
    for error in errors {
        let error_str = format!("{}", error);
        assert!(!error_str.is_empty());
        assert!(error_str.len() > 5); // Should have meaningful content
    }
}

#[test]
fn test_error_debug_formatting() {
    let error = BlixardError::NotImplemented {
        feature: "debug test".to_string(),
    };
    
    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("NotImplemented"));
    assert!(debug_str.contains("debug test"));
}

#[test]
fn test_io_error_conversion() {
    let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let blixard_error: BlixardError = io_error.into();
    
    match blixard_error {
        BlixardError::IoError(_) => {},
        _ => panic!("Expected IoError variant"),
    }
}

#[test]
fn test_bincode_error_conversion() {
    // Create a serialization error scenario
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize)]
    struct TestStruct {
        value: String,
    }
    
    // Try to deserialize invalid data
    let invalid_data = b"invalid bincode data";
    let result: Result<TestStruct, bincode::Error> = bincode::deserialize(invalid_data);
    
    if let Err(bincode_error) = result {
        let blixard_error: BlixardError = bincode_error.into();
        match blixard_error {
            BlixardError::SerializationError(_) => {},
            _ => panic!("Expected SerializationError variant"),
        }
    }
}

#[test]
fn test_result_type_alias() {
    fn test_function() -> BlixardResult<String> {
        Ok("success".to_string())
    }
    
    let result = test_function();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
}

#[test]
fn test_error_chaining() {
    let root_cause = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let blixard_error: BlixardError = root_cause.into();
    
    // Test that error chaining works
    assert!(blixard_error.source().is_some());
}

#[test]
fn test_structured_errors() {
    let storage_error = BlixardError::Storage {
        operation: "read".to_string(),
        source: Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "file missing")),
    };
    
    let error_str = format!("{}", storage_error);
    assert!(error_str.contains("read"));
    assert!(error_str.contains("failed"));
    
    let raft_error = BlixardError::Raft {
        operation: "append".to_string(),
        source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "corrupt log")),
    };
    
    let error_str = format!("{}", raft_error);
    assert!(error_str.contains("append"));
    assert!(error_str.contains("failed"));
}

#[test]
fn test_serialization_error_structure() {
    let ser_error = BlixardError::Serialization {
        operation: "encode vm state".to_string(),
        source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "encoding failed")),
    };
    
    let error_str = format!("{}", ser_error);
    assert!(error_str.contains("encode vm state"));
    assert!(error_str.contains("failed"));
}

#[test]
fn test_error_variants_coverage() {
    // Test all error variants can be created
    let _errors = vec![
        BlixardError::ServiceNotFound("test".to_string()),
        BlixardError::ServiceAlreadyExists("test".to_string()),
        BlixardError::ServiceManagementError("test".to_string()),
        BlixardError::StorageTransactionError("test".to_string()),
        BlixardError::StorageTableError("test".to_string()),
        BlixardError::ClusterError("test".to_string()),
        BlixardError::NodeError("test".to_string()),
        BlixardError::ConfigError("test".to_string()),
        BlixardError::SystemError("test".to_string()),
        BlixardError::Internal { message: "test".to_string() },
        BlixardError::NotImplemented { feature: "test".to_string() },
        BlixardError::Storage {
            operation: "test".to_string(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        },
        BlixardError::Raft {
            operation: "test".to_string(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        },
        BlixardError::Serialization {
            operation: "test".to_string(),
            source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        },
    ];
}

#[test]
fn test_error_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BlixardError>();
}

#[test]
fn test_not_implemented_error_details() {
    let features = vec![
        "VM creation",
        "cluster joining",
        "raft consensus",
        "storage persistence",
        "network partitioning",
    ];
    
    for feature in features {
        let error = BlixardError::NotImplemented {
            feature: feature.to_string(),
        };
        
        let error_str = format!("{}", error);
        assert!(error_str.contains(feature));
        assert!(error_str.contains("not implemented"));
    }
}