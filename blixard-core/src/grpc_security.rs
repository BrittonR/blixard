//! gRPC security middleware for authentication and authorization
//!
//! This module provides middleware to integrate security features with gRPC:
//! - Authentication via tokens or mTLS
//! - Authorization via RBAC permissions
//! - Security context propagation

use crate::security::{SecurityManager, AuthResult, Permission, extract_auth_token};
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
                permissions: auth_result.permissions,
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
                permissions: auth_result.permissions,
                auth_method: auth_result.auth_method,
            })
        }
    }
    
    /// Check if the security context has the required permission
    pub async fn check_permission(&self, context: &SecurityContext, permission: &Permission) -> BlixardResult<bool> {
        // Admin permission bypasses all checks
        if context.permissions.contains(&Permission::Admin) {
            return Ok(true);
        }
        
        // Check if the specific permission is granted
        if context.permissions.contains(permission) {
            return Ok(true);
        }
        
        // If user is authenticated, check role-based permissions
        if let Some(ref user) = context.user {
            return self.security_manager.check_permission(user, permission).await;
        }
        
        Ok(false)
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
#[macro_export]
macro_rules! authenticate_grpc {
    ($middleware:expr, $request:expr, $permission:expr) => {{
        let security_context = match $middleware.authenticate_request(&$request).await {
            Ok(context) => context,
            Err(e) => {
                return Err($crate::grpc_security::GrpcSecurityMiddleware::security_error_to_status(e));
            }
        };
        
        if !security_context.authenticated {
            return Err(tonic::Status::unauthenticated("Authentication required"));
        }
        
        let has_permission = match $middleware.check_permission(&security_context, &$permission).await {
            Ok(allowed) => allowed,
            Err(e) => {
                return Err($crate::grpc_security::GrpcSecurityMiddleware::security_error_to_status(e));
            }
        };
        
        if !has_permission {
            return Err(tonic::Status::permission_denied(format!("Permission denied: {:?}", $permission)));
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
            permissions: auth_result.permissions,
            auth_method: auth_result.auth_method,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{default_dev_security_config, Permission};
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
        assert!(context.permissions.contains(&Permission::Admin));
    }
    
    #[tokio::test]
    async fn test_permission_checking() {
        let config = default_dev_security_config();
        let security_manager = Arc::new(SecurityManager::new(config).await.unwrap());
        let middleware = GrpcSecurityMiddleware::new(security_manager);
        
        let context = SecurityContext {
            authenticated: true,
            user: Some("test-user".to_string()),
            permissions: vec![Permission::VmRead],
            auth_method: "token".to_string(),
        };
        
        // Should have VmRead permission
        let has_read = middleware.check_permission(&context, &Permission::VmRead).await.unwrap();
        assert!(has_read);
        
        // Should not have VmWrite permission
        let has_write = middleware.check_permission(&context, &Permission::VmWrite).await.unwrap();
        assert!(!has_write);
    }
    
    #[tokio::test]
    async fn test_admin_permission_bypass() {
        let config = default_dev_security_config();
        let security_manager = Arc::new(SecurityManager::new(config).await.unwrap());
        let middleware = GrpcSecurityMiddleware::new(security_manager);
        
        let admin_context = SecurityContext {
            authenticated: true,
            user: Some("admin".to_string()),
            permissions: vec![Permission::Admin],
            auth_method: "token".to_string(),
        };
        
        // Admin should have all permissions
        let has_vm_read = middleware.check_permission(&admin_context, &Permission::VmRead).await.unwrap();
        let has_vm_write = middleware.check_permission(&admin_context, &Permission::VmWrite).await.unwrap();
        let has_cluster_write = middleware.check_permission(&admin_context, &Permission::ClusterWrite).await.unwrap();
        
        assert!(has_vm_read);
        assert!(has_vm_write);
        assert!(has_cluster_write);
    }
}