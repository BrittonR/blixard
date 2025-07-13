//! Example of using authentication in Blixard

use blixard_core::{
    security::{SecurityManager, Permission, default_dev_security_config},
    config::{SecurityConfig, AuthConfig, TlsConfig},
    error::BlixardResult,
};
use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct TokenFile {
    tokens: HashMap<String, TokenEntry>,
}

#[derive(Serialize, Deserialize)]
struct TokenEntry {
    token_hash: String,
    user: String,
    permissions: Vec<String>,
    active: bool,
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Create tokens file for testing
    create_test_tokens().await?;
    
    // Create security configuration with authentication enabled
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false,  // We'll use TLS separately
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: Some(PathBuf::from("test_tokens.json")),
        },
    };
    
    // Create security manager
    let mut security_manager = SecurityManager::new(security_config).await?;
    
    // Test token authentication
    println!("\n=== Testing Token Authentication ===");
    
    // Test valid admin token
    let admin_token = "admin-token-12345";
    let auth_result = security_manager.authenticate_token(admin_token).await?;
    println!("Admin token auth: {:?}", auth_result);
    
    // Test valid read-only token
    let readonly_token = "readonly-token-67890";
    let auth_result = security_manager.authenticate_token(readonly_token).await?;
    println!("Read-only token auth: {:?}", auth_result);
    
    // Test invalid token
    let invalid_token = "invalid-token";
    let auth_result = security_manager.authenticate_token(invalid_token).await?;
    println!("Invalid token auth: {:?}", auth_result);
    
    // Test permission checking
    println!("\n=== Testing Permission Checking ===");
    
    // Check admin permissions
    let admin_can_write = security_manager.check_permission("admin", &Permission::VmWrite).await?;
    println!("Admin can write VMs: {}", admin_can_write);
    
    // Check read-only permissions
    let readonly_can_write = security_manager.check_permission("viewer", &Permission::VmWrite).await?;
    let readonly_can_read = security_manager.check_permission("viewer", &Permission::VmRead).await?;
    println!("Read-only can write VMs: {}", readonly_can_write);
    println!("Read-only can read VMs: {}", readonly_can_read);
    
    // Generate new tokens
    println!("\n=== Generating New Tokens ===");
    
    // Generate VM operator token
    let operator_token = security_manager.generate_token(
        "vm-operator",
        vec![Permission::VmRead, Permission::VmWrite, Permission::TaskWrite],
        Some(std::time::Duration::from_secs(86400)), // 1 day expiry
    ).await?;
    println!("Generated VM operator token: {}", operator_token);
    
    // Generate temporary admin token
    let temp_admin_token = security_manager.generate_token(
        "temp-admin",
        vec![Permission::Admin],
        Some(std::time::Duration::from_secs(3600)), // 1 hour expiry
    ).await?;
    println!("Generated temporary admin token: {}", temp_admin_token);
    
    // Test secret storage
    println!("\n=== Testing Secret Storage ===");
    
    // Store a secret
    security_manager.store_secret("database-password", "super-secret-password").await?;
    println!("Stored database password");
    
    // Retrieve the secret
    if let Some(secret) = security_manager.get_secret("database-password").await? {
        println!("Retrieved secret: {}", secret);
    }
    
    // Example of using authentication in a gRPC handler
    println!("\n=== Example gRPC Handler ===");
    example_grpc_handler().await;
    
    Ok(())
}

/// Create test tokens file
async fn create_test_tokens() -> BlixardResult<()> {
    use blixard_core::security::TokenInfo;
    
    let mut tokens = HashMap::new();
    
    // Admin token
    let admin_token_hash = hash_token("admin-token-12345");
    tokens.insert(admin_token_hash.clone(), serde_json::json!({
        "token_hash": admin_token_hash,
        "user": "admin",
        "permissions": ["Admin"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": true
    }));
    
    // Read-only token
    let readonly_token_hash = hash_token("readonly-token-67890");
    tokens.insert(readonly_token_hash.clone(), serde_json::json!({
        "token_hash": readonly_token_hash,
        "user": "viewer",
        "permissions": ["ClusterRead", "VmRead", "TaskRead", "MetricsRead"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": true
    }));
    
    // Expired token
    let expired_token_hash = hash_token("expired-token-11111");
    tokens.insert(expired_token_hash.clone(), serde_json::json!({
        "token_hash": expired_token_hash,
        "user": "expired-user",
        "permissions": ["VmRead"],
        "expires_at": "2023-01-01T00:00:00Z",
        "created_at": "2023-01-01T00:00:00Z",
        "active": true
    }));
    
    // Inactive token
    let inactive_token_hash = hash_token("inactive-token-22222");
    tokens.insert(inactive_token_hash.clone(), serde_json::json!({
        "token_hash": inactive_token_hash,
        "user": "inactive-user",
        "permissions": ["VmRead"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": false
    }));
    
    let token_file = serde_json::json!(tokens);
    tokio::fs::write("test_tokens.json", serde_json::to_string_pretty(&token_file)?).await?;
    
    println!("Created test tokens file");
    Ok(())
}

/// Hash a token (simple SHA256 for demo)
fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token);
    format!("{:x}", hasher.finalize())
}

/// Example of using authentication in a gRPC handler
async fn example_grpc_handler() {
    use tonic::{Request, Response, Status};
    use blixard_core::security::extract_auth_token;
    
    // Simulate a gRPC request with auth token
    let mut request = Request::new(());
    request.metadata_mut().insert(
        "authorization",
        "Bearer admin-token-12345".parse().unwrap(),
    );
    
    // Extract token from request
    if let Some(token) = extract_auth_token(&request) {
        println!("Extracted token from request: {}", token);
        
        // In a real handler, you would:
        // 1. Authenticate the token
        // 2. Check permissions for the specific operation
        // 3. Proceed or return error
    } else {
        println!("No auth token in request");
    }
}

/// Example middleware for authentication
pub async fn require_auth<T>(
    request: Request<T>,
    security_manager: &SecurityManager,
    required_permission: Permission,
) -> Result<(Request<T>, String), Status> {
    // Extract token
    let token = extract_auth_token(&request)
        .ok_or_else(|| Status::unauthenticated("No authentication token provided"))?;
    
    // Authenticate
    let auth_result = security_manager.authenticate_token(&token)
        .await
        .map_err(|e| Status::internal(format!("Authentication error: {}", e)))?;
    
    if !auth_result.authenticated {
        return Err(Status::unauthenticated("Invalid or expired token"));
    }
    
    let user = auth_result.user
        .ok_or_else(|| Status::internal("No user in auth result"))?;
    
    // Check permission
    let has_permission = security_manager.check_permission(&user, &required_permission)
        .await
        .map_err(|e| Status::internal(format!("Permission check error: {}", e)))?;
    
    if !has_permission {
        return Err(Status::permission_denied(format!(
            "User {} does not have permission {:?}",
            user, required_permission
        )));
    }
    
    Ok((request, user))
}