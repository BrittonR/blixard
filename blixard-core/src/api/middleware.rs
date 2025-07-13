//! Middleware for REST API authentication and request handling

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    compression::CompressionLayer,
    timeout::TimeoutLayer,
};

use crate::node_shared::SharedNodeState;
use crate::api::schemas::ErrorResponse;

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthState {
    pub shared_state: Arc<SharedNodeState>,
}

/// Authentication middleware that validates tokens
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication for health endpoints and OpenAPI docs
    let path = req.uri().path();
    if is_public_endpoint(path) {
        return Ok(next.run(req).await);
    }
    
    // Extract authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .or_else(|| req.headers().get("X-API-Key"));
    
    if let Some(auth_value) = auth_header {
        if let Ok(auth_str) = auth_value.to_str() {
            let token = if auth_str.starts_with("Bearer ") {
                &auth_str[7..] // Remove "Bearer " prefix
            } else {
                auth_str // Assume API key
            };
            
            // Validate token (placeholder implementation)
            if validate_auth_token(token).await {
                // Add user context to request extensions
                req.extensions_mut().insert(AuthenticatedUser {
                    id: "user_123".to_string(),
                    token_type: if auth_str.starts_with("Bearer ") {
                        TokenType::Bearer
                    } else {
                        TokenType::ApiKey
                    },
                });
                
                return Ok(next.run(req).await);
            }
        }
    }
    
    // Return 401 Unauthorized
    Err(StatusCode::UNAUTHORIZED)
}

/// Check if an endpoint is public (doesn't require authentication)
fn is_public_endpoint(path: &str) -> bool {
    matches!(path, 
        "/health" | "/health/ready" | "/health/live" |
        "/docs" | "/docs/" | "/api-doc/openapi.json" |
        "/swagger-ui" | "/swagger-ui/" |
        path if path.starts_with("/swagger-ui/") ||
                path.starts_with("/docs/") ||
                path.starts_with("/.well-known/")
    )
}

/// Validate authentication token (placeholder)
async fn validate_auth_token(token: &str) -> bool {
    // Placeholder implementation
    // In production, this would:
    // 1. Validate JWT signature and expiration
    // 2. Check API key in secure store
    // 3. Verify token hasn't been revoked
    // 4. Check rate limits
    
    !token.is_empty() && token.len() >= 8
}

/// Authenticated user context
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: String,
    pub token_type: TokenType,
}

/// Token type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Bearer,
    ApiKey,
}

/// Request ID middleware for tracing
pub async fn request_id_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    // Generate or extract request ID
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| {
            // Generate new request ID
            uuid::Uuid::new_v4().to_string().leak()
        });
    
    // Add request ID to extensions
    req.extensions_mut().insert(RequestId(request_id.to_string()));
    
    // Run the request
    let mut response = next.run(req).await;
    
    // Add request ID to response headers
    response.headers_mut().insert(
        "x-request-id",
        request_id.parse().unwrap(),
    );
    
    response
}

/// Request ID wrapper
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Create the middleware stack for the REST API
pub fn create_middleware_stack() -> ServiceBuilder<
    tower::util::Stack<
        tower::util::Stack<
            tower::util::Stack<
                tower::util::Stack<
                    CompressionLayer,
                    TimeoutLayer,
                >,
                CorsLayer,
            >,
            TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>,
        >,
        axum::middleware::FromFnLayer<
            fn(Request, Next) -> impl std::future::Future<Output = Response> + Send,
            Request,
            Response,
        >,
    >,
> {
    ServiceBuilder::new()
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(request_id_middleware))
}

/// Error handler for middleware failures
pub async fn handle_middleware_error(error: axum::BoxError) -> (StatusCode, axum::Json<ErrorResponse>) {
    if error.is::<tower::timeout::error::Elapsed>() {
        (
            StatusCode::REQUEST_TIMEOUT,
            axum::Json(ErrorResponse {
                code: 408,
                error: "Request Timeout".to_string(),
                message: "Request took too long to process".to_string(),
                details: None,
                request_id: None,
                timestamp: chrono::Utc::now(),
            }),
        )
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ErrorResponse {
                code: 500,
                error: "Internal Server Error".to_string(),
                message: "An unexpected error occurred".to_string(),
                details: None,
                request_id: None,
                timestamp: chrono::Utc::now(),
            }),
        )
    }
}