//! OpenAPI documentation generation
//!
//! This module generates comprehensive OpenAPI 3.x documentation for the Blixard REST API.

use utoipa::OpenApi;

/// Main OpenAPI documentation structure
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Blixard API",
        version = "1.0.0",
        description = "Distributed MicroVM Orchestration Platform REST API",
        license(name = "MIT"),
        contact(
            name = "Blixard Team",
            url = "https://github.com/blixard/blixard"
        )
    ),
    servers(
        (url = "/api/v1", description = "Version 1 API"),
        (url = "http://localhost:7000/api/v1", description = "Local development server"),
        (url = "https://api.blixard.dev/api/v1", description = "Production API server")
    ),
    paths(
        // Health endpoints
        crate::api::rest::health::health_check,
        crate::api::rest::health::readiness_check,
        crate::api::rest::health::liveness_check,
        
        // Authentication endpoints
        crate::api::rest::auth::authenticate,
        crate::api::rest::auth::validate_token,
        crate::api::rest::auth::authorize_action,
        
        // Cluster management endpoints
        crate::api::rest::cluster::get_cluster_status,
        crate::api::rest::cluster::join_cluster,
        crate::api::rest::cluster::leave_cluster,
        crate::api::rest::cluster::list_nodes,
        crate::api::rest::cluster::get_node,
        
        // VM management endpoints
        crate::api::rest::vm::list_vms,
        crate::api::rest::vm::create_vm,
        crate::api::rest::vm::get_vm,
        crate::api::rest::vm::start_vm,
        crate::api::rest::vm::stop_vm,
        crate::api::rest::vm::delete_vm,
        crate::api::rest::vm::migrate_vm,
    ),
    components(
        schemas(
            // Common schemas
            crate::api::schemas::PaginationParams,
            crate::api::schemas::PaginatedResponse<crate::api::schemas::VmInfo>,
            crate::api::schemas::ResourceMetadata,
            
            // Error schemas
            crate::api::schemas::ErrorResponse,
            crate::api::schemas::ErrorDetails,
            crate::api::schemas::ValidationError,
            
            // Health schemas
            crate::api::schemas::HealthResponse,
            crate::api::schemas::HealthStatus,
            crate::api::schemas::ServiceHealth,
            crate::api::schemas::ResourceHealth,
            crate::api::schemas::ResourceHealthStatus,
            crate::api::schemas::ClusterHealth,
            crate::api::schemas::ReplicationHealth,
            crate::api::schemas::ReadinessResponse,
            crate::api::schemas::LivenessResponse,
            crate::api::schemas::VersionInfo,
            
            // Authentication schemas
            crate::api::schemas::AuthRequest,
            crate::api::schemas::AuthResponse,
            crate::api::schemas::Identity,
            crate::api::schemas::IdentityType,
            crate::api::schemas::Permission,
            crate::api::schemas::AuthorizeRequest,
            crate::api::schemas::AuthorizeResponse,
            crate::api::schemas::ValidateTokenRequest,
            crate::api::schemas::ValidateTokenResponse,
            crate::api::schemas::TokenInfo,
            crate::api::schemas::TokenType,
            crate::api::schemas::CreateApiKeyRequest,
            crate::api::schemas::CreateApiKeyResponse,
            crate::api::schemas::ApiKeyInfo,
            
            // Cluster schemas
            crate::api::schemas::JoinClusterRequest,
            crate::api::schemas::JoinClusterResponse,
            crate::api::schemas::LeaveClusterRequest,
            crate::api::schemas::LeaveClusterResponse,
            crate::api::schemas::ClusterStatusResponse,
            crate::api::schemas::NodeInfo,
            crate::api::schemas::NodeStatusDto,
            crate::api::schemas::NodeCapabilities,
            crate::api::schemas::NodeResourceUtilization,
            crate::api::schemas::ClusterResourceSummary,
            crate::api::schemas::ClusterResourceUtilization,
            crate::api::schemas::ListNodesParams,
            
            // VM schemas
            crate::api::schemas::CreateVmRequest,
            crate::api::schemas::CreateVmResponse,
            crate::api::schemas::VmInfo,
            crate::api::schemas::VmStatusDto,
            crate::api::schemas::VmConfigDto,
            crate::api::schemas::VmRuntimeInfo,
            crate::api::schemas::VmResourceUsage,
            crate::api::schemas::HypervisorDto,
            crate::api::schemas::StartVmRequest,
            crate::api::schemas::StopVmRequest,
            crate::api::schemas::VmOperationResponse,
            crate::api::schemas::ListVmsParams,
            crate::api::schemas::MigrateVmRequest,
        ),
        security_schemes(
            ("bearer_auth" = (
                type = Http,
                scheme = Bearer,
                bearer_format = "JWT",
                description = "JWT Bearer token authentication"
            )),
            ("api_key" = (
                type = ApiKey,
                key_name = "X-API-Key",
                location = Header,
                description = "API key authentication"
            ))
        )
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    ),
    tags(
        (name = "health", description = "Health check and monitoring endpoints"),
        (name = "auth", description = "Authentication and authorization endpoints"),
        (name = "cluster", description = "Cluster management and node operations"),
        (name = "vm", description = "Virtual machine lifecycle management"),
    )
)]
pub struct ApiDoc;

/// Generate OpenAPI specification as JSON
pub fn generate_openapi_json() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_else(|e| {
        eprintln!("Failed to generate OpenAPI JSON: {}", e);
        "{}".to_string()
    })
}

/// Generate OpenAPI specification as YAML
pub fn generate_openapi_yaml() -> String {
    serde_yaml::to_string(&ApiDoc::openapi()).unwrap_or_else(|e| {
        eprintln!("Failed to generate OpenAPI YAML: {}", e);
        "openapi: 3.0.0\ninfo:\n  title: Blixard API\n  version: 1.0.0".to_string()
    })
}

/// OpenAPI configuration for different environments
#[derive(Debug, Clone)]
pub struct OpenApiConfig {
    /// Base URL for the API
    pub base_url: String,
    /// Whether to include example responses
    pub include_examples: bool,
    /// Whether to include detailed error schemas
    pub detailed_errors: bool,
    /// Custom server URLs
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub url: String,
    pub description: String,
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            base_url: "/api/v1".to_string(),
            include_examples: true,
            detailed_errors: true,
            servers: vec![
                ServerConfig {
                    url: "/api/v1".to_string(),
                    description: "Current server".to_string(),
                },
            ],
        }
    }
}

impl OpenApiConfig {
    /// Create configuration for production environment
    pub fn production(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            include_examples: false,
            detailed_errors: false,
            servers: vec![
                ServerConfig {
                    url: "/api/v1".to_string(),
                    description: "Production API".to_string(),
                },
            ],
        }
    }
    
    /// Create configuration for development environment
    pub fn development() -> Self {
        Self {
            base_url: "http://localhost:7000/api/v1".to_string(),
            include_examples: true,
            detailed_errors: true,
            servers: vec![
                ServerConfig {
                    url: "http://localhost:7000/api/v1".to_string(),
                    description: "Development server".to_string(),
                },
                ServerConfig {
                    url: "/api/v1".to_string(),
                    description: "Relative URL".to_string(),
                },
            ],
        }
    }
}