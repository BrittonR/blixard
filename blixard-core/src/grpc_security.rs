//! gRPC security middleware for authentication
//!
//! This module provides middleware to integrate security features with gRPC:
//! - Authentication via tokens or mTLS
//! - Security context propagation
//! - Authorization is handled by Cedar policies

use crate::security::{SecurityManager, AuthResult, extract_auth_token};
use crate::error::{BlixardError, BlixardResult};
use std::sync::Arc;
use tonic::{Request, Status};
use tonic::metadata::MetadataMap;
use tracing::{debug, warn};

/// Security middleware for gRPC services
#[derive(Clone)]
pub struct GrpcSecurityMiddleware {
    security_manager: Arc<SecurityManager>,
}

/// Security context for authenticated requests
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Whether the request is authenticated
    pub authenticated: bool,
    
    /// User identity (if authenticated)
    pub user: Option<String>,
    
    /// Granted permissions
    pub permissions: Vec<Permission>,
    
    /// Authentication method used
    pub auth_method: String,
}

impl GrpcSecurityMiddleware {
    /// Create a new security middleware
    pub fn new(security_manager: Arc<SecurityManager>) -> Self {
        Self {
            security_manager,
        }
    }
    
    /// Authenticate and authorize a gRPC request
    pub async fn authenticate_request<T>(&self, request: &Request<T>) -> BlixardResult<SecurityContext> {
        // Extract authentication token from request
        if let Some(token) = extract_auth_token(request) {
            debug!("Authenticating request with token");
            let auth_result = self.security_manager.authenticate_token(&token).await?;
            
            Ok(SecurityContext {
                authenticated: auth_result.authenticated,
                user: auth_result.user,
                auth_method: auth_result.auth_method,
            })
        } else {
            // Check if authentication is required
            // For now, if no token is provided, try to authenticate anonymously
            debug!("No authentication token provided, checking anonymous access");
            let auth_result = self.security_manager.authenticate_token("").await?;
            
            Ok(SecurityContext {
                authenticated: auth_result.authenticated,
                user: auth_result.user,
                auth_method: auth_result.auth_method,
            })
        }
    }
    
    /// Convert security errors to gRPC status codes
    pub fn security_error_to_status(error: BlixardError) -> Status {
        match error {
            BlixardError::Security { message } => {
                warn!("Security error: {}", message);
                Status::unauthenticated(format!("Authentication failed: {}", message))
            }
            _ => Status::internal(format!("Internal security error: {}", error)),
        }
    }
}

/// Macro to add authentication to gRPC methods
/// Note: Authorization is now handled by Cedar policies, not by this macro
#[macro_export]
macro_rules! authenticate_grpc {
    ($middleware:expr, $request:expr) => {{
        let security_context = match $middleware.authenticate_request(&$request).await {
            Ok(context) => context,
            Err(e) => {
                return Err($crate::grpc_security::GrpcSecurityMiddleware::security_error_to_status(e));
            }
        };
        
        if !security_context.authenticated {
            return Err(tonic::Status::unauthenticated("Authentication required"));
        }
        
        Ok(security_context)
    }};
}

/// Macro to add optional authentication to gRPC methods
#[macro_export]
macro_rules! optional_authenticate_grpc {
    ($middleware:expr, $request:expr) => {{
        match $middleware.authenticate_request(&$request).await {
            Ok(context) => Some(context),
            Err(e) => {
                tracing::debug!("Optional authentication failed: {}", e);
                None
            }
        }
    }};
}

impl From<AuthResult> for SecurityContext {
    fn from(auth_result: AuthResult) -> Self {
        Self {
            authenticated: auth_result.authenticated,
            user: auth_result.user,
            auth_method: auth_result.auth_method,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::default_dev_security_config;
    use tonic::metadata::MetadataMap;
    
    #[tokio::test]
    async fn test_authentication_middleware() {
        let config = default_dev_security_config();
        let security_manager = Arc::new(SecurityManager::new(config).await.unwrap());
        let middleware = GrpcSecurityMiddleware::new(security_manager);
        
        // Create a mock request without authentication
        let request = Request::new(());
        
        // Should succeed with disabled authentication
        let context = middleware.authenticate_request(&request).await.unwrap();
        assert!(context.authenticated);
        assert_eq!(context.user, Some("anonymous".to_string()));
    }
}