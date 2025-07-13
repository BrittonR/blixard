//! Authentication and authorization API schemas

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// Authentication request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct AuthRequest {
    /// API key or token
    #[validate(length(min = 1, max = 1024))]
    pub token: String,
    
    /// Optional scope for the token
    #[validate(length(min = 1, max = 255))]
    pub scope: Option<String>,
}

/// Authentication response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthResponse {
    /// Whether authentication was successful
    pub authenticated: bool,
    
    /// User/service identity (if authenticated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    
    /// Authentication message
    pub message: String,
    
    /// Token expiration time (ISO 8601, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// User/service identity information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Identity {
    /// Unique identifier
    pub id: String,
    
    /// Display name
    pub name: String,
    
    /// Identity type
    pub identity_type: IdentityType,
    
    /// Granted permissions
    pub permissions: Vec<Permission>,
    
    /// Optional tenant/organization ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    
    /// Optional metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Type of identity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityType {
    /// Human user
    User,
    /// Service account
    Service,
    /// System/admin account
    System,
}

/// Permission/authorization information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Permission {
    /// Resource type (e.g., "vm", "cluster", "node")
    pub resource: String,
    
    /// Allowed actions (e.g., "read", "write", "delete")
    pub actions: Vec<String>,
    
    /// Optional resource scope/filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Authorization check request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct AuthorizeRequest {
    /// Resource being accessed
    #[validate(length(min = 1, max = 255))]
    pub resource: String,
    
    /// Action being performed
    #[validate(length(min = 1, max = 100))]
    pub action: String,
    
    /// Optional resource context
    #[serde(default)]
    pub context: std::collections::HashMap<String, serde_json::Value>,
}

/// Authorization check response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthorizeResponse {
    /// Whether action is authorized
    pub authorized: bool,
    
    /// Authorization decision reason
    pub reason: String,
    
    /// Applied policy (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    
    /// Decision timestamp (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Token validation request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct ValidateTokenRequest {
    /// Token to validate
    #[validate(length(min = 1, max = 1024))]
    pub token: String,
    
    /// Optional required scope
    #[validate(length(min = 1, max = 255))]
    pub required_scope: Option<String>,
}

/// Token validation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateTokenResponse {
    /// Whether token is valid
    pub valid: bool,
    
    /// Validation message
    pub message: String,
    
    /// Token information (if valid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_info: Option<TokenInfo>,
    
    /// Validation timestamp (ISO 8601)
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Token information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenInfo {
    /// Token identifier (not the token itself)
    pub token_id: String,
    
    /// Token type
    pub token_type: TokenType,
    
    /// Associated identity
    pub identity: Identity,
    
    /// Token issued at (ISO 8601)
    pub issued_at: chrono::DateTime<chrono::Utc>,
    
    /// Token expires at (ISO 8601, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Token scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Type of authentication token
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// API key (long-lived)
    ApiKey,
    /// Bearer token (short-lived)
    Bearer,
    /// Service token (for service-to-service)
    Service,
}

/// Create API key request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateApiKeyRequest {
    /// API key name/description
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    
    /// Optional expiration time (ISO 8601)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Optional scope for the API key
    #[validate(length(min = 1, max = 255))]
    pub scope: Option<String>,
    
    /// Permissions to grant
    pub permissions: Vec<Permission>,
}

/// Create API key response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateApiKeyResponse {
    /// Generated API key (only returned once!)
    pub api_key: String,
    
    /// API key metadata
    pub key_info: ApiKeyInfo,
    
    /// Creation timestamp (ISO 8601)
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// API key information (without the actual key)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiKeyInfo {
    /// API key ID
    pub id: String,
    
    /// API key name/description
    pub name: String,
    
    /// Associated identity
    pub identity_id: String,
    
    /// API key scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    
    /// Whether key is active
    pub active: bool,
    
    /// Last used timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Expiration timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AuthResponse {
    /// Create successful authentication response
    pub fn success(identity: Identity) -> Self {
        Self {
            authenticated: true,
            identity: Some(identity),
            message: "Authentication successful".to_string(),
            expires_at: None,
        }
    }
    
    /// Create failed authentication response
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            authenticated: false,
            identity: None,
            message: message.into(),
            expires_at: None,
        }
    }
}

impl AuthorizeResponse {
    /// Create authorized response
    pub fn authorized(reason: impl Into<String>) -> Self {
        Self {
            authorized: true,
            reason: reason.into(),
            policy: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Create denied response
    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            authorized: false,
            reason: reason.into(),
            policy: None,
            timestamp: chrono::Utc::now(),
        }
    }
}