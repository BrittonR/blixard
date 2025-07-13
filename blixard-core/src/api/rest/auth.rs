//! Authentication and authorization REST endpoints

use axum::{extract::Extension, response::Json};
use utoipa::path;

use crate::api::schemas::{
    AuthRequest, AuthResponse, AuthorizeRequest, AuthorizeResponse,
    ValidateTokenRequest, ValidateTokenResponse, Identity, IdentityType,
    Permission, TokenInfo, TokenType,
};
use super::{AppState, json_error, handle_blixard_error};

/// Authenticate a user or service
#[utoipa::path(
    post,
    path = "/auth/authenticate",
    tag = "auth",
    summary = "Authenticate with token",
    description = "Authenticate using an API key or bearer token",
    request_body = AuthRequest,
    responses(
        (status = 200, description = "Authentication successful", body = AuthResponse),
        (status = 401, description = "Authentication failed", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn authenticate(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Validate token length
    if request.token.is_empty() || request.token.len() > 1024 {
        return Ok(Json(AuthResponse::failure("Invalid token format")));
    }
    
    // Check if authentication is enabled
    if !shared_state.is_initialized() {
        return Err(json_error(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Authentication service not initialized"
        ));
    }
    
    // For now, implement basic token validation
    // In a real implementation, this would validate against a secure token store
    let is_valid = validate_token(&request.token);
    
    if is_valid {
        // Create a basic identity for successful authentication
        let identity = Identity {
            id: "user_123".to_string(),
            name: "API User".to_string(),
            identity_type: IdentityType::User,
            permissions: vec![
                Permission {
                    resource: "vm".to_string(),
                    actions: vec!["read".to_string(), "write".to_string()],
                    scope: None,
                },
                Permission {
                    resource: "cluster".to_string(),
                    actions: vec!["read".to_string()],
                    scope: None,
                },
            ],
            tenant_id: None,
            metadata: std::collections::HashMap::new(),
        };
        
        Ok(Json(AuthResponse::success(identity)))
    } else {
        Ok(Json(AuthResponse::failure("Invalid token")))
    }
}

/// Validate a token
#[utoipa::path(
    post,
    path = "/auth/validate",
    tag = "auth",
    summary = "Validate token",
    description = "Validate a token and return token information",
    request_body = ValidateTokenRequest,
    responses(
        (status = 200, description = "Token validation result", body = ValidateTokenResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn validate_token(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<ValidateTokenRequest>,
) -> Result<Json<ValidateTokenResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    // Validate token
    let is_valid = validate_token(&request.token);
    
    let response = if is_valid {
        let identity = Identity {
            id: "user_123".to_string(),
            name: "API User".to_string(),
            identity_type: IdentityType::User,
            permissions: vec![
                Permission {
                    resource: "vm".to_string(),
                    actions: vec!["read".to_string(), "write".to_string()],
                    scope: None,
                },
            ],
            tenant_id: None,
            metadata: std::collections::HashMap::new(),
        };
        
        let token_info = TokenInfo {
            token_id: "token_123".to_string(),
            token_type: TokenType::ApiKey,
            identity,
            issued_at: chrono::Utc::now() - chrono::Duration::hours(24),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(30)),
            scope: request.required_scope.clone(),
        };
        
        ValidateTokenResponse {
            valid: true,
            message: "Token is valid".to_string(),
            token_info: Some(token_info),
            timestamp: chrono::Utc::now(),
        }
    } else {
        ValidateTokenResponse {
            valid: false,
            message: "Token is invalid or expired".to_string(),
            token_info: None,
            timestamp: chrono::Utc::now(),
        }
    };
    
    Ok(Json(response))
}

/// Authorize an action
#[utoipa::path(
    post,
    path = "/auth/authorize",
    tag = "auth",
    summary = "Authorize action",
    description = "Check if the current user is authorized to perform an action",
    request_body = AuthorizeRequest,
    responses(
        (status = 200, description = "Authorization result", body = AuthorizeResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
pub async fn authorize_action(
    Extension(app_state): Extension<AppState>,
    Json(request): Json<AuthorizeRequest>,
) -> Result<Json<AuthorizeResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    // For now, implement basic authorization logic
    // In a real implementation, this would use a policy engine like Cedar
    
    let authorized = match (request.resource.as_str(), request.action.as_str()) {
        ("vm", "read") | ("vm", "write") => true,
        ("cluster", "read") => true,
        ("cluster", "write") => false, // Require admin privileges
        _ => false,
    };
    
    let response = if authorized {
        AuthorizeResponse::authorized("Permission granted based on user roles")
    } else {
        AuthorizeResponse::denied("Insufficient permissions for this action")
    };
    
    Ok(Json(response))
}

/// Basic token validation (placeholder implementation)
fn validate_token(token: &str) -> bool {
    // This is a placeholder implementation
    // In production, this would:
    // 1. Verify token signature (for JWTs)
    // 2. Check token expiration
    // 3. Validate against a secure token store
    // 4. Check token revocation status
    
    // For demo purposes, accept tokens that start with "blixard_"
    token.starts_with("blixard_") && token.len() >= 16
}