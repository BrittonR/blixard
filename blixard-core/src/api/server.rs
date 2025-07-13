//! REST API server implementation

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
};
use super::{
    docs::{ApiDoc, OpenApiConfig},
    middleware::{auth_middleware, create_middleware_stack, handle_middleware_error, AuthState},
    rest::{create_api_router, AppState},
};

/// REST API server configuration
#[derive(Debug, Clone)]
pub struct RestApiConfig {
    /// Bind address for the REST API server
    pub bind_address: SocketAddr,
    /// Whether to enable Swagger UI
    pub enable_swagger_ui: bool,
    /// OpenAPI configuration
    pub openapi_config: OpenApiConfig,
    /// Whether to require authentication
    pub require_auth: bool,
}

impl Default for RestApiConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:7000".parse().unwrap(),
            enable_swagger_ui: true,
            openapi_config: OpenApiConfig::development(),
            require_auth: false, // Disabled by default for development
        }
    }
}

/// REST API server
pub struct RestApiServer {
    config: RestApiConfig,
    shared_state: Arc<SharedNodeState>,
}

impl RestApiServer {
    /// Create a new REST API server
    pub fn new(config: RestApiConfig, shared_state: Arc<SharedNodeState>) -> Self {
        Self {
            config,
            shared_state,
        }
    }
    
    /// Create the main application router
    pub fn create_router(&self) -> Router {
        let app_state = AppState {
            shared_state: self.shared_state.clone(),
        };
        
        let auth_state = AuthState {
            shared_state: self.shared_state.clone(),
        };
        
        // Create API routes
        let api_routes = create_api_router(self.shared_state.clone())
            .layer(
                ServiceBuilder::new()
                    .layer(axum::middleware::from_fn_with_state(
                        auth_state.clone(),
                        auth_middleware,
                    ))
                    .into_inner(),
            );
        
        let mut router = Router::new()
            .nest("/api/v1", api_routes)
            .route("/", get(serve_index))
            .route("/health", get(super::rest::health::health_check))
            .route("/openapi.json", get(serve_openapi_json))
            .route("/openapi.yaml", get(serve_openapi_yaml))
            .with_state(app_state);
        
        // Add Swagger UI if enabled
        if self.config.enable_swagger_ui {
            router = router.merge(
                SwaggerUi::new("/docs")
                    .url("/openapi.json", ApiDoc::openapi())
                    .config(utoipa_swagger_ui::Config::new(["/openapi.json"]))
            );
        }
        
        // Add middleware stack
        router
            .layer(create_middleware_stack())
            .handle_error(handle_middleware_error)
    }
    
    /// Start the REST API server
    pub async fn serve(self) -> BlixardResult<()> {
        let router = self.create_router();
        
        tracing::info!(
            "Starting REST API server on {}",
            self.config.bind_address
        );
        
        if self.config.enable_swagger_ui {
            tracing::info!(
                "Swagger UI available at http://{}/docs",
                self.config.bind_address
            );
        }
        
        let listener = tokio::net::TcpListener::bind(&self.config.bind_address)
            .await
            .map_err(|e| BlixardError::Network {
                message: format!("Failed to bind to {}: {}", self.config.bind_address, e),
            })?;
        
        axum::serve(listener, router)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("REST API server error: {}", e),
            })?;
        
        Ok(())
    }
    
    /// Start the server in a background task
    pub fn spawn(self) -> tokio::task::JoinHandle<BlixardResult<()>> {
        tokio::spawn(async move { self.serve().await })
    }
}

/// Serve the index page
async fn serve_index() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Blixard API</title>
    <style>
        body { 
            font-family: system-ui, -apple-system, sans-serif; 
            max-width: 800px; 
            margin: 40px auto; 
            padding: 20px; 
            line-height: 1.6; 
        }
        .header { text-align: center; margin-bottom: 40px; }
        .endpoints { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 20px; }
        .endpoint-group { 
            border: 1px solid #ddd; 
            border-radius: 8px; 
            padding: 20px; 
            background: #f9f9f9; 
        }
        .endpoint-group h3 { margin-top: 0; color: #333; }
        .endpoint-group ul { margin: 0; padding-left: 20px; }
        .endpoint-group li { margin: 8px 0; }
        .endpoint-group a { color: #0066cc; text-decoration: none; }
        .endpoint-group a:hover { text-decoration: underline; }
        .docs-link { 
            display: inline-block; 
            background: #0066cc; 
            color: white; 
            padding: 12px 24px; 
            border-radius: 6px; 
            text-decoration: none; 
            margin: 20px 0; 
        }
        .docs-link:hover { background: #0052a3; }
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 Blixard REST API</h1>
        <p>Distributed MicroVM Orchestration Platform</p>
        <a href="/docs" class="docs-link">📖 View API Documentation</a>
    </div>

    <div class="endpoints">
        <div class="endpoint-group">
            <h3>🏥 Health</h3>
            <ul>
                <li><a href="/health">GET /health</a> - System health</li>
                <li><a href="/health/ready">GET /health/ready</a> - Readiness check</li>
                <li><a href="/health/live">GET /health/live</a> - Liveness check</li>
            </ul>
        </div>

        <div class="endpoint-group">
            <h3>🔐 Authentication</h3>
            <ul>
                <li>POST /api/v1/auth/authenticate</li>
                <li>POST /api/v1/auth/validate</li>
                <li>POST /api/v1/auth/authorize</li>
            </ul>
        </div>

        <div class="endpoint-group">
            <h3>🏗️ Cluster</h3>
            <ul>
                <li>GET /api/v1/cluster/status</li>
                <li>POST /api/v1/cluster/join</li>
                <li>POST /api/v1/cluster/leave</li>
                <li>GET /api/v1/cluster/nodes</li>
            </ul>
        </div>

        <div class="endpoint-group">
            <h3>💻 Virtual Machines</h3>
            <ul>
                <li>GET /api/v1/vms</li>
                <li>POST /api/v1/vms</li>
                <li>GET /api/v1/vms/{name}</li>
                <li>POST /api/v1/vms/{name}/start</li>
                <li>POST /api/v1/vms/{name}/stop</li>
                <li>DELETE /api/v1/vms/{name}</li>
            </ul>
        </div>
    </div>

    <div style="text-align: center; margin-top: 40px; color: #666;">
        <p>
            <a href="/openapi.json">OpenAPI JSON</a> | 
            <a href="/openapi.yaml">OpenAPI YAML</a> |
            <a href="/docs">Interactive Documentation</a>
        </p>
    </div>
</body>
</html>
        "#,
    )
}

/// Serve OpenAPI specification as JSON
async fn serve_openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Serve OpenAPI specification as YAML
async fn serve_openapi_yaml() -> Result<(StatusCode, [(&'static str, &'static str); 1], String), StatusCode> {
    match serde_yaml::to_string(&ApiDoc::openapi()) {
        Ok(yaml) => Ok((
            StatusCode::OK,
            [("content-type", "application/x-yaml")],
            yaml,
        )),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}