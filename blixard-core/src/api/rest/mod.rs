//! REST API endpoints for Blixard
//!
//! This module provides REST endpoints that bridge to the existing Iroh P2P services,
//! offering a standards-compliant HTTP API while maintaining internal P2P communication.

use axum::{
    extract::{Extension, Query, Path},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    validate_request::ValidateRequestHeaderLayer,
};

use crate::node_shared::SharedNodeState;
use crate::api::schemas::{ErrorResponse, PaginationParams};

pub mod auth;
pub mod cluster;
pub mod health;
pub mod vm;

/// Application state for REST endpoints
#[derive(Clone)]
pub struct AppState {
    /// Shared node state for accessing P2P services
    pub shared_state: Arc<SharedNodeState>,
}

/// Create the main REST API router
pub fn create_api_router(shared_state: Arc<SharedNodeState>) -> Router {
    let app_state = AppState { shared_state };

    Router::new()
        .route("/health", get(health::health_check))
        .route("/health/ready", get(health::readiness_check))
        .route("/health/live", get(health::liveness_check))
        .route("/auth/authenticate", post(auth::authenticate))
        .route("/auth/validate", post(auth::validate_token))
        .route("/auth/authorize", post(auth::authorize_action))
        .route("/cluster/status", get(cluster::get_cluster_status))
        .route("/cluster/join", post(cluster::join_cluster))
        .route("/cluster/leave", post(cluster::leave_cluster))
        .route("/cluster/nodes", get(cluster::list_nodes))
        .route("/cluster/nodes/:node_id", get(cluster::get_node))
        .route("/vms", get(vm::list_vms))
        .route("/vms", post(vm::create_vm))
        .route("/vms/:vm_name", get(vm::get_vm))
        .route("/vms/:vm_name/start", post(vm::start_vm))
        .route("/vms/:vm_name/stop", post(vm::stop_vm))
        .route("/vms/:vm_name", delete(vm::delete_vm))
        .route("/vms/:vm_name/migrate", post(vm::migrate_vm))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
                .layer(Extension(app_state))
        )
}

/// Helper function to create JSON error responses
pub fn json_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    let error = ErrorResponse {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap_or("Unknown").to_string(),
        message: message.into(),
        details: None,
        request_id: None,
        timestamp: chrono::Utc::now(),
    };
    (status, Json(error))
}

/// Extract pagination parameters with defaults
pub fn extract_pagination(params: Option<Query<PaginationParams>>) -> (u32, u32) {
    params
        .map(|Query(p)| (p.offset.unwrap_or(0), p.limit.unwrap_or(20)))
        .unwrap_or((0, 20))
}

/// Helper to convert Blixard errors to HTTP responses
pub fn handle_blixard_error(error: crate::error::BlixardError) -> (StatusCode, Json<ErrorResponse>) {
    use crate::error::BlixardError;
    
    let (status, message) = match &error {
        BlixardError::NotFound { .. } => (StatusCode::NOT_FOUND, error.to_string()),
        BlixardError::NotInitialized { .. } => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        BlixardError::InvalidConfiguration { .. } => (StatusCode::BAD_REQUEST, error.to_string()),
        BlixardError::ValidationError { .. } => (StatusCode::BAD_REQUEST, error.to_string()),
        BlixardError::AuthenticationFailed { .. } => (StatusCode::UNAUTHORIZED, error.to_string()),
        BlixardError::AuthorizationFailed { .. } => (StatusCode::FORBIDDEN, error.to_string()),
        BlixardError::ResourceExhausted { .. } => (StatusCode::TOO_MANY_REQUESTS, error.to_string()),
        BlixardError::Internal { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
    };
    
    json_error(status, message)
}