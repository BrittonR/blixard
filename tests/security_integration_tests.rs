//! Comprehensive security integration tests
//!
//! Tests end-to-end security flows including:
//! - Authentication and authorization flows
//! - Token management and validation
//! - Secrets management with encryption
//! - Cedar policy engine integration
//! - Multi-tenant resource isolation
//! - Security configuration hot reload

#[cfg(test)]
mod security_integration_tests {
    use blixard_core::{
        cedar_authz::CedarAuthz,
        config_v2::{AuthConfig, SecurityConfig, TlsConfig},
        error::BlixardError,
        security::{SecurityManager, AuthResult, TokenInfo, UserRole},
    };
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;
    use tokio::fs;

/// Helper to create test security config
fn create_test_security_config(auth_enabled: bool) -> SecurityConfig {
    SecurityConfig {
        auth: AuthConfig {
            enabled: auth_enabled,
            token_lifetime: Duration::from_secs(3600),
            admin_tokens: vec!["admin-token".to_string()],
            rate_limiting: true,
            max_requests_per_minute: 100,
        },
        tls: TlsConfig {
            enabled: false, // Iroh handles transport security
            cert_path: None,
            key_path: None,
            ca_path: None,
            verify_client_cert: false,
        },
        secrets_encryption_key: Some("test-key-for-encryption-32bytes!".to_string()),
        audit_log_path: Some(PathBuf::from("/tmp/test-audit.log")),
        hot_reload: true,
    }
}

/// Helper to create Cedar test files
async fn create_cedar_test_files(temp_dir: &TempDir) -> Result<(), Box<dyn std::error::Error>> {
    let cedar_dir = temp_dir.path().join("cedar");
    fs::create_dir_all(&cedar_dir).await?;
    
    // Create schema file
    let schema_content = r#"{
        "User": {
            "attributes": {
                "department": {"type": "String"},
                "role": {"type": "String"}
            }
        },
        "VM": {
            "attributes": {
                "owner": {"type": "String"},
                "tenant": {"type": "String"}
            }
        },
        "Action": {}
    }"#;
    
    fs::write(cedar_dir.join("schema.cedarschema.json"), schema_content).await?;
    
    // Create policies directory and policies
    let policies_dir = cedar_dir.join("policies");
    fs::create_dir_all(&policies_dir).await?;
    
    let admin_policy = r#"
permit(
    principal == User::"admin",
    action,
    resource
);
"#;
    
    let vm_owner_policy = r#"
permit(
    principal,
    action == Action::"start" || action == Action::"stop",
    resource
) when {
    resource.owner == principal.uid
};
"#;
    
    fs::write(policies_dir.join("admin.cedar"), admin_policy).await?;
    fs::write(policies_dir.join("vm_owner.cedar"), vm_owner_policy).await?;
    
    Ok(())
}

#[tokio::test]
async fn test_security_manager_authentication_flow() {
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Test initial state - no valid tokens except admin
    let invalid_result = security_manager.authenticate_token("invalid-token").await.unwrap();
    assert!(!invalid_result.authenticated);
    
    // Generate a token for a user
    let user_token = security_manager.generate_token("test-user", Some(Duration::from_secs(3600))).await.unwrap();
    assert!(!user_token.is_empty());
    
    // Test authentication with generated token
    let auth_result = security_manager.authenticate_token(&user_token).await.unwrap();
    assert!(auth_result.authenticated);
    assert_eq!(auth_result.user, Some("test-user".to_string()));
    
    // Test token expiration (simulate by generating with very short lifetime)
    let short_token = security_manager.generate_token("short-lived", Some(Duration::from_millis(1))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await; // Wait for expiration
    
    let expired_result = security_manager.authenticate_token(&short_token).await.unwrap();
    assert!(!expired_result.authenticated);
}

#[tokio::test]
async fn test_security_manager_disabled_auth() {
    let config = create_test_security_config(false);
    let security_manager = SecurityManager::new(config).await.unwrap();
    
    // When auth is disabled, all tokens should be accepted
    let result = security_manager.authenticate_token("any-random-token").await.unwrap();
    assert!(result.authenticated);
    assert_eq!(result.user, Some("anonymous".to_string()));
    assert_eq!(result.auth_method, "disabled");
}

#[tokio::test]
async fn test_secrets_management_integration() {
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Store secrets
    security_manager.store_secret("db-password", "super-secret-password").await.unwrap();
    security_manager.store_secret("api-key", "sk-1234567890abcdef").await.unwrap();
    
    // Retrieve secrets
    let db_password = security_manager.get_secret("db-password").await.unwrap();
    assert_eq!(db_password, Some("super-secret-password".to_string()));
    
    let api_key = security_manager.get_secret("api-key").await.unwrap();
    assert_eq!(api_key, Some("sk-1234567890abcdef".to_string()));
    
    // Test non-existent secret
    let missing = security_manager.get_secret("non-existent").await.unwrap();
    assert_eq!(missing, None);
    
    // Store and retrieve complex secret
    let complex_secret = json!({
        "username": "admin",
        "password": "complex-password",
        "permissions": ["read", "write", "admin"]
    }).to_string();
    
    security_manager.store_secret("complex-config", &complex_secret).await.unwrap();
    let retrieved_complex = security_manager.get_secret("complex-config").await.unwrap();
    assert_eq!(retrieved_complex, Some(complex_secret));
}

#[tokio::test]
async fn test_cedar_authorization_integration() {
    let temp_dir = TempDir::new().unwrap();
    create_cedar_test_files(&temp_dir).await.unwrap();
    
    // Change to temp directory so Cedar files are found
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    
    let config = create_test_security_config(true);
    let security_manager = SecurityManager::new(config).await.unwrap();
    
    // Add entities to Cedar
    let user_attrs = HashMap::from([
        ("department".to_string(), json!("engineering")),
        ("role".to_string(), json!("admin")),
    ]);
    
    security_manager.add_cedar_entity("User", "admin", user_attrs, vec![]).await.unwrap();
    
    let vm_attrs = HashMap::from([
        ("owner".to_string(), json!("admin")),
        ("tenant".to_string(), json!("default")),
    ]);
    
    security_manager.add_cedar_entity("VM", "test-vm", vm_attrs, vec![]).await.unwrap();
    
    // Test authorization
    let resource = SecurityManager::build_resource_uid("VM", "test-vm");
    let context = HashMap::new();
    
    // Admin should be able to do anything
    let admin_can_start = security_manager
        .check_permission_cedar("admin", "start", &resource, context.clone())
        .await
        .unwrap();
    assert!(admin_can_start);
    
    // Test with regular user
    let user_attrs = HashMap::from([
        ("department".to_string(), json!("engineering")),
        ("role".to_string(), json!("user")),
    ]);
    security_manager.add_cedar_entity("User", "regular-user", user_attrs, vec![]).await.unwrap();
    
    // Regular user should not be able to access VM they don't own
    let user_can_start = security_manager
        .check_permission_cedar("regular-user", "start", &resource, context)
        .await
        .unwrap();
    assert!(!user_can_start);
    
    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
}

#[tokio::test]
async fn test_multi_tenant_isolation() {
    let temp_dir = TempDir::new().unwrap();
    create_cedar_test_files(&temp_dir).await.unwrap();
    
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Create users in different tenants
    let tenant_a_user_attrs = HashMap::from([
        ("tenant".to_string(), json!("tenant-a")),
        ("role".to_string(), json!("user")),
    ]);
    security_manager.add_cedar_entity("User", "user-a", tenant_a_user_attrs, vec![]).await.unwrap();
    
    let tenant_b_user_attrs = HashMap::from([
        ("tenant".to_string(), json!("tenant-b")),
        ("role".to_string(), json!("user")),
    ]);
    security_manager.add_cedar_entity("User", "user-b", tenant_b_user_attrs, vec![]).await.unwrap();
    
    // Create VMs in different tenants
    let vm_a_attrs = HashMap::from([
        ("tenant".to_string(), json!("tenant-a")),
        ("owner".to_string(), json!("user-a")),
    ]);
    security_manager.add_cedar_entity("VM", "vm-a", vm_a_attrs, vec![]).await.unwrap();
    
    let vm_b_attrs = HashMap::from([
        ("tenant".to_string(), json!("tenant-b")),
        ("owner".to_string(), json!("user-b")),
    ]);
    security_manager.add_cedar_entity("VM", "vm-b", vm_b_attrs, vec![]).await.unwrap();
    
    // Test cross-tenant access (should be denied)
    let vm_a_resource = SecurityManager::build_resource_uid("VM", "vm-a");
    let vm_b_resource = SecurityManager::build_resource_uid("VM", "vm-b");
    let context = HashMap::new();
    
    // User A should not access VM B
    let cross_tenant_access = security_manager
        .check_permission_cedar("user-a", "start", &vm_b_resource, context.clone())
        .await
        .unwrap();
    assert!(!cross_tenant_access);
    
    // User B should not access VM A
    let reverse_cross_tenant = security_manager
        .check_permission_cedar("user-b", "start", &vm_a_resource, context)
        .await
        .unwrap();
    assert!(!reverse_cross_tenant);
    
    std::env::set_current_dir(original_dir).unwrap();
}

#[tokio::test]
async fn test_token_lifecycle_management() {
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Generate multiple tokens for different users
    let alice_token = security_manager.generate_token("alice", Some(Duration::from_secs(3600))).await.unwrap();
    let bob_token = security_manager.generate_token("bob", Some(Duration::from_secs(7200))).await.unwrap();
    let service_token = security_manager.generate_token("service-account", None).await.unwrap(); // No expiration
    
    // All tokens should be valid initially
    assert!(security_manager.authenticate_token(&alice_token).await.unwrap().authenticated);
    assert!(security_manager.authenticate_token(&bob_token).await.unwrap().authenticated);
    assert!(security_manager.authenticate_token(&service_token).await.unwrap().authenticated);
    
    // Test that tokens are different
    assert_ne!(alice_token, bob_token);
    assert_ne!(alice_token, service_token);
    assert_ne!(bob_token, service_token);
    
    // Test token reuse prevention (generating a new token for same user)
    let alice_token_2 = security_manager.generate_token("alice", Some(Duration::from_secs(3600))).await.unwrap();
    assert_ne!(alice_token, alice_token_2);
    
    // Both tokens should be valid for Alice
    let auth1 = security_manager.authenticate_token(&alice_token).await.unwrap();
    let auth2 = security_manager.authenticate_token(&alice_token_2).await.unwrap();
    assert!(auth1.authenticated);
    assert!(auth2.authenticated);
    assert_eq!(auth1.user, Some("alice".to_string()));
    assert_eq!(auth2.user, Some("alice".to_string()));
}

#[tokio::test]
async fn test_security_error_scenarios() {
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Test malformed tokens
    let malformed_results = vec![
        security_manager.authenticate_token("").await.unwrap(),
        security_manager.authenticate_token("short").await.unwrap(),
        security_manager.authenticate_token("spaces in token").await.unwrap(),
        security_manager.authenticate_token("extremely-long-token-that-exceeds-reasonable-length-and-should-be-rejected-or-handled-gracefully").await.unwrap(),
    ];
    
    for result in malformed_results {
        assert!(!result.authenticated, "Malformed token should not authenticate");
    }
    
    // Test security manager with disabled auth trying to generate tokens
    let disabled_config = create_test_security_config(false);
    let mut disabled_security = SecurityManager::new(disabled_config).await.unwrap();
    
    let token_result = disabled_security.generate_token("test-user", None).await;
    assert!(token_result.is_err());
    match token_result.unwrap_err() {
        BlixardError::Security { message } => {
            assert!(message.contains("Authentication is disabled"));
        }
        _ => panic!("Expected Security error"),
    }
}

#[tokio::test]
async fn test_concurrent_security_operations() {
    let config = create_test_security_config(true);
    let mut security_manager = SecurityManager::new(config).await.unwrap();
    
    // Generate tokens concurrently
    let token_futures = (0..10).map(|i| {
        security_manager.generate_token(&format!("user-{}", i), Some(Duration::from_secs(3600)))
    });
    
    let tokens: Vec<_> = futures::future::try_join_all(token_futures).await.unwrap();
    
    // All tokens should be unique
    let mut unique_tokens = std::collections::HashSet::new();
    for token in &tokens {
        assert!(unique_tokens.insert(token.clone()), "Duplicate token generated");
    }
    
    // All tokens should authenticate concurrently
    let auth_futures = tokens.iter().map(|token| {
        security_manager.authenticate_token(token)
    });
    
    let auth_results: Vec<_> = futures::future::try_join_all(auth_futures).await.unwrap();
    
    for (i, result) in auth_results.iter().enumerate() {
        assert!(result.authenticated);
        assert_eq!(result.user, Some(format!("user-{}", i)));
    }
    
    // Store secrets concurrently
    let secret_futures = (0..5).map(|i| {
        security_manager.store_secret(&format!("secret-{}", i), &format!("value-{}", i))
    });
    
    futures::future::try_join_all(secret_futures).await.unwrap();
    
    // Retrieve secrets concurrently
    let retrieval_futures = (0..5).map(|i| {
        security_manager.get_secret(&format!("secret-{}", i))
    });
    
    let retrieved_secrets: Vec<_> = futures::future::try_join_all(retrieval_futures).await.unwrap();
    
    for (i, secret) in retrieved_secrets.iter().enumerate() {
        assert_eq!(secret, &Some(format!("value-{}", i)));
    }
}

#[tokio::test]
async fn test_security_configuration_edge_cases() {
    // Test with minimal configuration
    let minimal_config = SecurityConfig {
        auth: AuthConfig {
            enabled: false,
            token_lifetime: Duration::from_secs(0), // Zero lifetime
            admin_tokens: vec![],
            rate_limiting: false,
            max_requests_per_minute: 0,
        },
        tls: TlsConfig {
            enabled: false,
            cert_path: None,
            key_path: None,
            ca_path: None,
            verify_client_cert: false,
        },
        secrets_encryption_key: None,
        audit_log_path: None,
        hot_reload: false,
    };
    
    let minimal_security = SecurityManager::new(minimal_config).await.unwrap();
    
    // Should still work with minimal config
    let auth_result = minimal_security.authenticate_token("any-token").await.unwrap();
    assert!(auth_result.authenticated);
    
    // Test with very long token lifetime
    let long_lifetime_config = SecurityConfig {
        auth: AuthConfig {
            enabled: true,
            token_lifetime: Duration::from_secs(365 * 24 * 3600), // 1 year
            admin_tokens: vec!["admin".to_string()],
            rate_limiting: true,
            max_requests_per_minute: 1000000, // Very high limit
        },
        tls: TlsConfig {
            enabled: false,
            cert_path: None,
            key_path: None,
            ca_path: None,
            verify_client_cert: false,
        },
        secrets_encryption_key: Some("test-key-for-encryption-32bytes!".to_string()),
        audit_log_path: Some(PathBuf::from("/tmp/long-test-audit.log")),
        hot_reload: true,
    };
    
    let long_security = SecurityManager::new(long_lifetime_config).await.unwrap();
    
    // Should work with extreme configuration
    let auth_result = long_security.authenticate_token("admin").await.unwrap();
    assert!(auth_result.authenticated);
}

}