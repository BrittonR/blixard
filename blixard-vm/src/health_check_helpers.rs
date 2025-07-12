//! Health check helper utilities
//!
//! This module provides helper functions for creating HealthCheckResult objects
//! and reducing code duplication in health check implementations.

use std::time::{Instant, SystemTime};
use blixard_core::vm_health_types::HealthCheckResult;

/// Builder for HealthCheckResult to reduce code duplication
pub struct HealthCheckResultBuilder {
    check_name: String,
    start_time: Instant,
    timestamp: SystemTime,
}

impl HealthCheckResultBuilder {
    /// Create a new health check result builder
    pub fn new(check_name: &str, start_time: Instant, timestamp: SystemTime) -> Self {
        Self {
            check_name: check_name.to_string(),
            start_time,
            timestamp,
        }
    }

    /// Build a successful health check result
    pub fn success(self, message: String) -> HealthCheckResult {
        HealthCheckResult {
            check_name: self.check_name,
            success: true,
            message,
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            timestamp_secs: self.timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: None,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: false,
            recovery_action: None,
        }
    }

    /// Build a failed health check result
    pub fn failure(self, message: String, error: Option<String>) -> HealthCheckResult {
        HealthCheckResult {
            check_name: self.check_name,
            success: false,
            message,
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            timestamp_secs: self.timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: false,
            recovery_action: None,
        }
    }

    /// Build a health check result with custom success status
    pub fn result(self, success: bool, message: String, error: Option<String>) -> HealthCheckResult {
        HealthCheckResult {
            check_name: self.check_name,
            success,
            message,
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            timestamp_secs: self.timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: false,
            recovery_action: None,
        }
    }
}

/// Helper function to create a HealthCheckResult builder
pub fn health_check_result(
    check_name: &str,
    start_time: Instant,
    timestamp: SystemTime,
) -> HealthCheckResultBuilder {
    HealthCheckResultBuilder::new(check_name, start_time, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, Instant};

    #[test]
    fn test_success_result() {
        let start_time = Instant::now();
        let timestamp = SystemTime::now();
        
        let result = health_check_result("test_check", start_time, timestamp)
            .success("Check passed".to_string());
        
        assert_eq!(result.check_name, "test_check");
        assert!(result.success);
        assert_eq!(result.message, "Check passed");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_failure_result() {
        let start_time = Instant::now();
        let timestamp = SystemTime::now();
        
        let result = health_check_result("test_check", start_time, timestamp)
            .failure("Check failed".to_string(), Some("Error details".to_string()));
        
        assert_eq!(result.check_name, "test_check");
        assert!(!result.success);
        assert_eq!(result.message, "Check failed");
        assert_eq!(result.error, Some("Error details".to_string()));
    }

    #[test]
    fn test_custom_result() {
        let start_time = Instant::now();
        let timestamp = SystemTime::now();
        
        let result = health_check_result("test_check", start_time, timestamp)
            .result(true, "Custom message".to_string(), None);
        
        assert_eq!(result.check_name, "test_check");
        assert!(result.success);
        assert_eq!(result.message, "Custom message");
        assert!(result.error.is_none());
    }
}