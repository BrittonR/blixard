//! Tests for error conversions
//!
//! This module tests that all the From trait implementations work correctly
//! and that the `?` operator can be used seamlessly with various error types.

#[cfg(test)]
mod tests {
    use crate::error::BlixardError;
    use std::sync::mpsc;
    use tokio::sync::oneshot;
    
    /// Test that JoinError can be converted using the ? operator
    #[tokio::test]
    async fn test_join_error_conversion() {
        async fn task_that_panics() -> Result<(), &'static str> {
            panic!("test panic");
        }
        
        async fn test_function() -> Result<(), BlixardError> {
            let handle = tokio::spawn(task_that_panics());
            let _result = handle.await?; // This should use our From<JoinError> impl
            Ok(())
        }
        
        let result = test_function().await;
        assert!(result.is_err());
        if let Err(BlixardError::Internal { message }) = result {
            assert!(message.contains("Task panicked"));
        } else {
            panic!("Expected Internal error with panic message");
        }
    }
    
    /// Test that timeout errors can be converted
    #[tokio::test]
    async fn test_timeout_error_conversion() {
        use tokio::time::{Duration, timeout};
        
        async fn test_function() -> Result<(), BlixardError> {
            timeout(Duration::from_millis(1), tokio::time::sleep(Duration::from_secs(1))).await?;
            Ok(())
        }
        
        let result = test_function().await;
        assert!(result.is_err());
        if let Err(BlixardError::Timeout { operation, .. }) = result {
            assert_eq!(operation, "async_operation");
        } else {
            panic!("Expected Timeout error");
        }
    }
    
    /// Test that oneshot channel errors can be converted
    #[tokio::test]
    async fn test_oneshot_recv_error_conversion() {
        async fn test_function() -> Result<(), BlixardError> {
            let (_tx, rx) = oneshot::channel::<String>();
            drop(_tx); // Drop sender to cause RecvError
            let _result = rx.await?; // This should use our From<RecvError> impl
            Ok(())
        }
        
        let result = test_function().await;
        assert!(result.is_err());
        if let Err(BlixardError::Internal { message }) = result {
            assert!(message.contains("Channel receive failed"));
        } else {
            panic!("Expected Internal error with receive message");
        }
    }
    
    /// Test that mpsc channel errors can be converted
    #[tokio::test]
    async fn test_mpsc_send_error_conversion() {
        async fn test_function() -> Result<(), BlixardError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
            drop(rx); // Drop receiver to cause SendError
            tx.send("test".to_string()).await?; // This should use our From<SendError> impl
            Ok(())
        }
        
        let result = test_function().await;
        assert!(result.is_err());
        if let Err(BlixardError::Internal { message }) = result {
            assert!(message.contains("Channel send failed"));
        } else {
            panic!("Expected Internal error with send message");
        }
    }
    
    /// Test that UTF-8 errors can be converted
    #[test]
    fn test_utf8_error_conversion() {
        fn test_function() -> Result<String, BlixardError> {
            let invalid_utf8 = vec![0, 159, 146, 150]; // Invalid UTF-8 sequence
            let s = String::from_utf8(invalid_utf8)?; // This should use our From<FromUtf8Error> impl
            Ok(s)
        }
        
        let result = test_function();
        assert!(result.is_err());
        if let Err(BlixardError::Serialization { operation, .. }) = result {
            assert_eq!(operation, "utf8_conversion");
        } else {
            panic!("Expected Serialization error");
        }
    }
    
    /// Test that reqwest errors can be converted (if the feature is available)
    #[cfg(feature = "test-helpers")]
    #[tokio::test]
    async fn test_reqwest_error_conversion() {
        async fn test_function() -> Result<(), BlixardError> {
            // This will fail with a connection error to an invalid URL
            let _response = reqwest::get("http://invalid-domain-that-does-not-exist-12345.com").await?;
            Ok(())
        }
        
        let result = test_function().await;
        assert!(result.is_err());
        // The error type will depend on what kind of reqwest error occurs
        // (could be NetworkError or ConnectionError)
        assert!(matches!(result, Err(BlixardError::NetworkError(_)) | Err(BlixardError::ConnectionError { .. })));
    }
    
    /// Test that error conversion preserves error information
    #[test]
    fn test_error_information_preservation() {
        // Test that our conversions preserve useful error information
        let parse_err = "not_a_number".parse::<i32>().unwrap_err();
        let blixard_err: BlixardError = parse_err.into();
        
        if let BlixardError::ConfigurationError { component, message } = blixard_err {
            assert_eq!(component, "numeric_value");
            assert!(message.contains("Invalid numeric value"));
        } else {
            panic!("Expected ConfigurationError for ParseIntError");
        }
    }
}