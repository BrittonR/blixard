//! Example of running a secure Blixard cluster with TLS and authentication

use blixard_core::{
    config::{SecurityConfig, TlsConfig, AuthConfig},
    security::SecurityManager,
    error::BlixardResult,
};
use std::path::PathBuf;
use std::collections::HashMap;
use serde_json;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("=== Blixard Secure Cluster Example ===\n");
    
    // Step 1: Create token file for authentication
    println!("1. Creating authentication tokens...");
    create_example_tokens().await?;
    
    // Step 2: Create security configuration
    println!("2. Setting up security configuration...");
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: true,
            cert_file: Some(PathBuf::from("certs/node1.crt")),
            key_file: Some(PathBuf::from("certs/node1.key")),
            ca_file: Some(PathBuf::from("certs/ca.crt")),
            require_client_cert: true,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: Some(PathBuf::from("example_tokens.json")),
        },
    };
    
    // Step 3: Show how to start a secure node
    println!("\n3. To start a secure node, use:");
    println!("   export BLIXARD_TLS_CA_CERT=certs/ca.crt");
    println!("   export BLIXARD_TLS_CERT=certs/node1.crt");
    println!("   export BLIXARD_TLS_KEY=certs/node1.key");
    println!("   export BLIXARD_TLS_REQUIRE_CLIENT_CERTS=true");
    println!("   export BLIXARD_AUTH_ENABLED=true");
    println!("   export BLIXARD_AUTH_TOKEN_FILE=example_tokens.json");
    println!("   blixard node --id 1 --bind 127.0.0.1:7001 --data-dir ./data");
    
    // Step 4: Show how to use authenticated CLI
    println!("\n4. To use the CLI with authentication:");
    println!("   export BLIXARD_API_TOKEN=admin-token-12345");
    println!("   blixard --endpoint https://127.0.0.1:7001 cluster status");
    
    // Step 5: Demonstrate token validation
    println!("\n5. Testing token authentication...");
    
    let mut security_manager = SecurityManager::new(security_config).await?;
    
    // Test admin token
    let admin_result = security_manager.authenticate_token("admin-token-12345").await?;
    println!("   Admin token: authenticated={}, user={:?}", 
        admin_result.authenticated, 
        admin_result.user
    );
    
    // Test viewer token
    let viewer_result = security_manager.authenticate_token("viewer-token-67890").await?;
    println!("   Viewer token: authenticated={}, user={:?}, permissions={:?}", 
        viewer_result.authenticated, 
        viewer_result.user,
        viewer_result.permissions
    );
    
    // Test invalid token
    let invalid_result = security_manager.authenticate_token("invalid-token").await?;
    println!("   Invalid token: authenticated={}", invalid_result.authenticated);
    
    // Step 6: Generate new tokens
    println!("\n6. Generating new API tokens...");
    
    let operator_token = security_manager.generate_token(
        "vm-operator",
        vec![
            blixard_core::security::Permission::VmRead,
            blixard_core::security::Permission::VmWrite,
            blixard_core::security::Permission::TaskWrite,
        ],
        Some(std::time::Duration::from_secs(86400)),
    ).await?;
    
    println!("   Generated VM operator token: {}", operator_token);
    println!("   This token expires in 24 hours");
    
    // Step 7: Show gRPC client example
    println!("\n7. Example gRPC client code:");
    println!(r#"
    use tonic::transport::Channel;
    use tonic::metadata::MetadataValue;
    
    let channel = Channel::from_static("https://127.0.0.1:7001")
        .tls_config(client_tls_config)?
        .connect()
        .await?;
    
    let token = MetadataValue::from_str(&format!("Bearer {}", api_token))?;
    
    let mut request = tonic::Request::new(GetClusterStatusRequest {});
    request.metadata_mut().insert("authorization", token);
    
    let response = client.get_cluster_status(request).await?;
    "#);
    
    println!("\n✅ Security setup complete!");
    println!("\nNext steps:");
    println!("1. Generate TLS certificates using scripts/generate-certs.sh");
    println!("2. Start nodes with security environment variables");
    println!("3. Use authenticated CLI or gRPC clients");
    
    Ok(())
}

/// Create example tokens file
async fn create_example_tokens() -> BlixardResult<()> {
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
    
    // Viewer token
    let viewer_token_hash = hash_token("viewer-token-67890");
    tokens.insert(viewer_token_hash.clone(), serde_json::json!({
        "token_hash": viewer_token_hash,
        "user": "viewer",
        "permissions": ["ClusterRead", "VmRead", "TaskRead", "MetricsRead"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": true
    }));
    
    // VM operator token
    let operator_token_hash = hash_token("operator-token-11111");
    tokens.insert(operator_token_hash.clone(), serde_json::json!({
        "token_hash": operator_token_hash,
        "user": "vm-operator",
        "permissions": ["ClusterRead", "VmRead", "VmWrite", "TaskRead", "TaskWrite"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": true
    }));
    
    // Prometheus token
    let prometheus_token_hash = hash_token("prometheus-token-22222");
    tokens.insert(prometheus_token_hash.clone(), serde_json::json!({
        "token_hash": prometheus_token_hash,
        "user": "prometheus",
        "permissions": ["MetricsRead"],
        "expires_at": null,
        "created_at": "2024-01-01T00:00:00Z",
        "active": true
    }));
    
    let token_file = serde_json::to_string_pretty(&tokens)?;
    tokio::fs::write("example_tokens.json", token_file).await?;
    
    println!("   Created example_tokens.json with:");
    println!("   - admin-token-12345 (full admin access)");
    println!("   - viewer-token-67890 (read-only access)");
    println!("   - operator-token-11111 (VM operations)");
    println!("   - prometheus-token-22222 (metrics only)");
    
    Ok(())
}

/// Hash a token using SHA256
fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token);
    format!("{:x}", hasher.finalize())
}