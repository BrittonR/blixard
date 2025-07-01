//! Integration tests for authentication and authorization

use blixard_core::{
    security::{SecurityManager, Permission, default_dev_security_config},
    config_v2::{SecurityConfig, AuthConfig, TlsConfig},
    error::BlixardResult,
    auth_interceptor::{AuthLayer, AuthService},
};
use std::sync::Arc;
use std::path::PathBuf;
use tonic::{Request, Response, Status, body::BoxBody};
use tower::Service;
use futures::future::FutureExt;

/// Mock service for testing
#[derive(Clone)]
struct MockService;

#[tonic::async_trait]
impl tower::Service<Request<String>> for MockService {
    type Response = Response<BoxBody>;
    type Error = Status;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<String>) -> Self::Future {
        Box::pin(async move {
            Ok(Response::new(BoxBody::empty()))
        })
    }
}

#[tokio::test]
async fn test_auth_layer_blocks_unauthenticated_requests() -> BlixardResult<()> {
    // Create security config with auth enabled
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: None,
        },
    };
    
    // Create security manager
    let security_manager = Arc::new(SecurityManager::new(security_config).await?);
    
    // Create auth layer
    let auth_layer = AuthLayer::new(security_manager.clone());
    
    // Create service with auth layer
    let mock_service = MockService;
    let mut auth_service = auth_layer.layer(mock_service);
    
    // Test request without token to protected endpoint
    let request = Request::builder()
        .uri("/blixard.ClusterService/CreateVm")
        .body(String::from("test"))
        .unwrap();
    
    let result = auth_service.call(request).await;
    assert!(result.is_err());
    
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    
    Ok(())
}

#[tokio::test]
async fn test_auth_layer_allows_health_check_without_auth() -> BlixardResult<()> {
    // Create security config with auth enabled
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: None,
        },
    };
    
    // Create security manager
    let security_manager = Arc::new(SecurityManager::new(security_config).await?);
    
    // Create auth layer
    let auth_layer = AuthLayer::new(security_manager.clone());
    
    // Create service with auth layer
    let mock_service = MockService;
    let mut auth_service = auth_layer.layer(mock_service);
    
    // Test health check request without token
    let request = Request::builder()
        .uri("/blixard.ClusterService/HealthCheck")
        .body(String::from("test"))
        .unwrap();
    
    let result = auth_service.call(request).await;
    assert!(result.is_ok());
    
    Ok(())
}

#[tokio::test]
async fn test_auth_layer_validates_permissions() -> BlixardResult<()> {
    // Create temp token file
    let token_file = tempfile::NamedTempFile::new()?;
    let token_file_path = token_file.path().to_path_buf();
    
    // Write test tokens
    let tokens = serde_json::json!({
        // Hash of "readonly-token"
        "b5d54c39c9d6f79c38b4f991e7e3c8a2e1234567890abcdef1234567890abcdef": {
            "token_hash": "b5d54c39c9d6f79c38b4f991e7e3c8a2e1234567890abcdef1234567890abcdef",
            "user": "readonly-user",
            "permissions": ["VmRead", "ClusterRead"],
            "expires_at": null,
            "created_at": "2024-01-01T00:00:00Z",
            "active": true
        }
    });
    
    std::fs::write(&token_file_path, serde_json::to_string(&tokens)?)?;
    
    // Create security config
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: Some(token_file_path),
        },
    };
    
    // Create security manager
    let security_manager = Arc::new(SecurityManager::new(security_config).await?);
    
    // Create auth layer
    let auth_layer = AuthLayer::new(security_manager.clone());
    
    // Create service with auth layer
    let mock_service = MockService;
    let mut auth_service = auth_layer.layer(mock_service);
    
    // Use the actual token (not the hash)
    let test_token = "test-readonly-token";
    
    // Generate a token programmatically
    let mut security_manager_mut = SecurityManager::new(SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: Some(token_file_path.clone()),
        },
    }).await?;
    
    let readonly_token = security_manager_mut.generate_token(
        "readonly-user",
        vec![Permission::VmRead, Permission::ClusterRead],
        None,
    ).await?;
    
    // Test read operation with readonly token - should succeed
    let mut request = Request::builder()
        .uri("/blixard.ClusterService/GetVmStatus")
        .body(String::from("test"))
        .unwrap();
    
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", readonly_token).parse().unwrap(),
    );
    
    let result = auth_service.call(request).await;
    assert!(result.is_ok());
    
    // Test write operation with readonly token - should fail
    let mut request = Request::builder()
        .uri("/blixard.ClusterService/CreateVm")
        .body(String::from("test"))
        .unwrap();
    
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", readonly_token).parse().unwrap(),
    );
    
    let result = auth_service.call(request).await;
    assert!(result.is_err());
    
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
    
    Ok(())
}

#[tokio::test]
async fn test_auth_disabled_allows_all() -> BlixardResult<()> {
    // Create security config with auth disabled
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: false,
            method: "token".to_string(),
            token_file: None,
        },
    };
    
    // Create security manager
    let security_manager = Arc::new(SecurityManager::new(security_config).await?);
    
    // With auth disabled, all requests should be allowed
    let auth_result = security_manager.authenticate_token("any-token").await?;
    assert!(auth_result.authenticated);
    assert_eq!(auth_result.user.unwrap(), "anonymous");
    assert!(auth_result.permissions.contains(&Permission::Admin));
    
    Ok(())
}

#[test]
fn test_permission_mapping() {
    use blixard_core::auth_interceptor::{get_required_permission, requires_auth};
    
    // Test permission mapping
    assert_eq!(
        get_required_permission("/blixard.ClusterService/CreateVm"),
        Some(Permission::VmWrite)
    );
    assert_eq!(
        get_required_permission("/blixard.ClusterService/GetVmStatus"),
        Some(Permission::VmRead)
    );
    assert_eq!(
        get_required_permission("/blixard.ClusterService/JoinCluster"),
        Some(Permission::ClusterWrite)
    );
    
    // Test auth requirements
    assert!(!requires_auth("/blixard.ClusterService/HealthCheck"));
    assert!(!requires_auth("/grpc.health.v1.Health/Check"));
    assert!(requires_auth("/blixard.ClusterService/CreateVm"));
    assert!(requires_auth("/blixard.ClusterService/GetClusterStatus"));
}

/// Test the authentication macros
#[tokio::test]
async fn test_auth_macros() -> BlixardResult<()> {
    use blixard_core::{require_auth, optional_auth, authenticate};
    
    // Create security manager with a test token
    let mut security_manager = SecurityManager::new(SecurityConfig {
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: None,
        },
    }).await?;
    
    // Generate test token
    let token = security_manager.generate_token(
        "test-user",
        vec![Permission::VmRead],
        None,
    ).await?;
    
    // Create request with token
    let mut request = Request::new(());
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );
    
    // Test require_auth macro
    let user = require_auth!(request, security_manager, Permission::VmRead);
    assert_eq!(user, "test-user");
    
    // Test optional_auth macro
    let mut request_with_token = Request::new(());
    request_with_token.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );
    
    let user = optional_auth!(request_with_token, security_manager);
    assert_eq!(user, Some("test-user".to_string()));
    
    // Test optional_auth without token
    let request_without_token = Request::new(());
    let user = optional_auth!(request_without_token, security_manager);
    assert_eq!(user, None);
    
    Ok(())
}

// Helper function to hash tokens for testing
fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}