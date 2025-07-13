//! Error response schemas for OpenAPI specification

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Standard error response for all API endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error code (matches HTTP status code)
    pub code: u16,
    
    /// Error type/category
    pub error: String,
    
    /// Human-readable error message
    pub message: String,
    
    /// Optional detailed error information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ErrorDetails>,
    
    /// Request ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    
    /// Timestamp when error occurred (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Detailed error information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetails {
    /// Field-specific validation errors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<ValidationError>>,
    
    /// Additional context about the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<std::collections::HashMap<String, serde_json::Value>>,
    
    /// Internal error code for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_code: Option<String>,
}

/// Validation error for specific fields
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    /// Field name that failed validation
    pub field: String,
    
    /// Validation rule that was violated
    pub rule: String,
    
    /// Human-readable error message
    pub message: String,
    
    /// Value that failed validation (if safe to include)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_value: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Create a simple error response
    pub fn new(code: u16, error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            error: error.into(),
            message: message.into(),
            details: None,
            request_id: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Create an error response with validation errors
    pub fn validation_error(validation_errors: Vec<ValidationError>) -> Self {
        Self {
            code: 400,
            error: "validation_error".to_string(),
            message: "Request validation failed".to_string(),
            details: Some(ErrorDetails {
                validation_errors: Some(validation_errors),
                context: None,
                internal_code: None,
            }),
            request_id: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Add request ID for tracing
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
    
    /// Add additional context
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        if self.details.is_none() {
            self.details = Some(ErrorDetails {
                validation_errors: None,
                context: Some(std::collections::HashMap::new()),
                internal_code: None,
            });
        }
        
        if let Some(ref mut details) = self.details {
            if details.context.is_none() {
                details.context = Some(std::collections::HashMap::new());
            }
            if let Some(ref mut context) = details.context {
                context.insert(key.into(), value);
            }
        }
        
        self
    }
}

/// Convert BlixardError to ErrorResponse
impl From<crate::error::BlixardError> for ErrorResponse {
    fn from(err: crate::error::BlixardError) -> Self {
        use crate::error::BlixardError;
        
        match err {
            BlixardError::ConfigurationError { component, message } => {
                ErrorResponse::new(400, "configuration_error", format!("Configuration error in {}: {}", component, message))
            }
            BlixardError::NotFound { resource } => {
                ErrorResponse::new(404, "not_found", format!("Resource not found: {}", resource))
            }
            BlixardError::NodeNotFound { node_id } => {
                ErrorResponse::new(404, "node_not_found", format!("Node {} not found", node_id))
            }
            BlixardError::AlreadyExists { resource } => {
                ErrorResponse::new(409, "already_exists", format!("Resource already exists: {}", resource))
            }
            BlixardError::InsufficientResources { requested, available } => {
                ErrorResponse::new(409, "insufficient_resources", format!("Insufficient resources: requested {}, available {}", requested, available))
            }
            BlixardError::AuthorizationError { message } => {
                ErrorResponse::new(403, "authorization_error", message)
            }
            BlixardError::Timeout { operation, duration } => {
                ErrorResponse::new(408, "timeout", format!("Operation '{}' timed out after {:?}", operation, duration))
            }
            BlixardError::NotLeader { operation, leader_id } => {
                ErrorResponse::new(503, "not_leader", format!("Node is not leader for operation '{}', current leader: {:?}", operation, leader_id))
            }
            BlixardError::NotInitialized { component } => {
                ErrorResponse::new(503, "not_initialized", format!("Component not initialized: {}", component))
            }
            BlixardError::TemporaryFailure { details } => {
                ErrorResponse::new(503, "temporary_failure", details)
            }
            // Add specific mappings for deprecated errors for backward compatibility
            #[allow(deprecated)]
            BlixardError::InvalidInput { field, message } => {
                ErrorResponse::validation_error(vec![ValidationError {
                    field,
                    rule: "invalid_input".to_string(),
                    message,
                    rejected_value: None,
                }])
            }
            // Catch-all for other errors
            _ => {
                ErrorResponse::new(500, "internal_error", err.to_string())
            }
        }
    }
}