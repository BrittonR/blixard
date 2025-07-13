//! Health check REST endpoints

use axum::{extract::Extension, response::Json};
use utoipa::path;

use crate::api::schemas::{
    HealthResponse, HealthStatus, ReadinessResponse, LivenessResponse,
    ServiceHealth, ResourceHealthStatus, VersionInfo,
};
use super::{AppState, json_error};

/// Comprehensive health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    summary = "Get comprehensive system health",
    description = "Returns detailed health information including services, resources, and cluster status",
    responses(
        (status = 200, description = "Health information retrieved successfully", body = HealthResponse),
        (status = 503, description = "Service unavailable", body = ErrorResponse)
    )
)]
pub async fn health_check(
    Extension(app_state): Extension<AppState>,
) -> Result<Json<HealthResponse>, (axum::http::StatusCode, Json<crate::api::schemas::ErrorResponse>)> {
    let shared_state = &app_state.shared_state;
    
    // Get basic node information
    let node_id = shared_state.get_id();
    let uptime_seconds = shared_state.get_uptime_seconds();
    
    // Create base health response
    let mut health = HealthResponse::healthy(node_id, uptime_seconds);
    
    // Check Iroh P2P service health
    let iroh_health = if shared_state.get_iroh_endpoint().await.is_some() {
        ServiceHealth {
            name: "iroh_p2p".to_string(),
            status: HealthStatus::Healthy,
            last_check: chrono::Utc::now(),
            message: Some("P2P service operational".to_string()),
            metrics: None,
        }
    } else {
        ServiceHealth {
            name: "iroh_p2p".to_string(),
            status: HealthStatus::Unhealthy,
            last_check: chrono::Utc::now(),
            message: Some("P2P service not initialized".to_string()),
            metrics: None,
        }
    };
    
    // Check Raft service health
    let raft_health = if shared_state.is_initialized() {
        ServiceHealth {
            name: "raft_consensus".to_string(),
            status: HealthStatus::Healthy,
            last_check: chrono::Utc::now(),
            message: Some("Raft consensus operational".to_string()),
            metrics: None,
        }
    } else {
        ServiceHealth {
            name: "raft_consensus".to_string(),
            status: HealthStatus::Degraded,
            last_check: chrono::Utc::now(),
            message: Some("Raft consensus not initialized".to_string()),
            metrics: None,
        }
    };
    
    // Add service health checks
    health = health
        .with_service(iroh_health)
        .with_service(raft_health);
    
    // Get cluster health if available
    if let Ok(cluster_status) = shared_state.get_cluster_status().await {
        let cluster_health = crate::api::schemas::ClusterHealth {
            status: HealthStatus::Healthy,
            connected_peers: cluster_status.nodes.len() as u32 - 1, // Exclude self
            expected_peers: cluster_status.nodes.len() as u32 - 1,
            is_leader: cluster_status.is_leader,
            raft_term: cluster_status.raft_term,
            replication_health: crate::api::schemas::ReplicationHealth {
                status: HealthStatus::Healthy,
                log_index: cluster_status.raft_log_index,
                healthy_followers: cluster_status.nodes.len() as u32 - 1,
                lagging_followers: 0,
                max_lag: 0,
            },
        };
        health = health.with_cluster(cluster_health);
    }
    
    // Calculate overall status based on components
    health.calculate_overall_status();
    
    Ok(Json(health))
}

/// Readiness check endpoint (Kubernetes-style)
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    summary = "Check if service is ready to accept requests",
    description = "Returns simple readiness status for load balancer health checks",
    responses(
        (status = 200, description = "Service is ready", body = ReadinessResponse),
        (status = 503, description = "Service not ready", body = ReadinessResponse)
    )
)]
pub async fn readiness_check(
    Extension(app_state): Extension<AppState>,
) -> Json<ReadinessResponse> {
    let shared_state = &app_state.shared_state;
    
    // Service is ready if it's initialized and P2P is available
    let ready = shared_state.is_initialized() && 
                shared_state.get_iroh_endpoint().await.is_some();
    
    let message = if ready {
        Some("Service ready to accept requests".to_string())
    } else {
        Some("Service not yet ready".to_string())
    };
    
    Json(ReadinessResponse {
        ready,
        timestamp: chrono::Utc::now(),
        message,
    })
}

/// Liveness check endpoint (Kubernetes-style)
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    summary = "Check if service is alive",
    description = "Returns simple liveness status for basic health monitoring",
    responses(
        (status = 200, description = "Service is alive", body = LivenessResponse)
    )
)]
pub async fn liveness_check() -> Json<LivenessResponse> {
    // If we can respond, we're alive
    Json(LivenessResponse {
        alive: true,
        timestamp: chrono::Utc::now(),
    })
}