//! Integration tests for Iroh identity enrollment
//!
//! Tests certificate-based and token-based enrollment for automatic node registration

use blixard_core::{
    error::BlixardResult,
    transport::{
        iroh_identity_enrollment::{
            CertRoleMapping, CertificateEnrollmentConfig, EnrollmentToken,
            IdentityEnrollmentManager,
        },
        iroh_middleware::{IrohMiddleware, NodeIdentityRegistry},
    },
};
use iroh::NodeId;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_token_based_enrollment() -> BlixardResult<()> {
    // Setup
    let registry = Arc::new(NodeIdentityRegistry::new());
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("enrollment_state.json");

    let manager = IdentityEnrollmentManager::new(registry.clone(), None, Some(state_path.clone()));

    // Generate enrollment token
    let token = manager
        .generate_enrollment_token(
            "test-operator".to_string(),
            vec!["operator".to_string(), "monitoring".to_string()],
            "tenant-1".to_string(),
            Duration::from_secs(3600),
            false, // single use
            None,
        )
        .await?;

    // Simulate a node enrolling
    let node_id = NodeId::from_bytes(&[1u8; 32]).unwrap();
    let result = manager
        .enroll_with_token(node_id, &token.token_id, &token.secret)
        .await?;

    assert!(result.success);
    assert_eq!(result.user_id, "test-operator");
    assert_eq!(result.roles, vec!["operator", "monitoring"]);
    assert_eq!(result.tenant_id, "tenant-1");

    // Verify node is registered
    let user = registry.get_user_for_node(node_id).await;
    assert_eq!(user, Some("test-operator".to_string()));

    // Try to use token again (should fail for single-use)
    let node_id2 = NodeId::from_bytes(&[2u8; 32]).unwrap();
    let result2 = manager
        .enroll_with_token(node_id2, &token.token_id, &token.secret)
        .await;

    assert!(result2.is_err());
    assert!(result2.unwrap_err().to_string().contains("already used"));

    Ok(())
}

#[tokio::test]
async fn test_multi_use_token_enrollment() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());
    let manager = IdentityEnrollmentManager::new(registry.clone(), None, None);

    // Generate multi-use token with max uses
    let token = manager
        .generate_enrollment_token(
            "worker-pool".to_string(),
            vec!["worker".to_string()],
            "tenant-2".to_string(),
            Duration::from_secs(3600),
            true,    // multi-use
            Some(3), // max 3 uses
        )
        .await?;

    // Enroll 3 nodes successfully
    for i in 0..3 {
        let node_id = NodeId::from_bytes(&[i as u8; 32]).unwrap();
        let result = manager
            .enroll_with_token(node_id, &token.token_id, &token.secret)
            .await?;

        assert!(result.success);
        assert_eq!(result.user_id, "worker-pool");
    }

    // 4th enrollment should fail
    let node_id4 = NodeId::from_bytes(&[4u8; 32]).unwrap();
    let result4 = manager
        .enroll_with_token(node_id4, &token.token_id, &token.secret)
        .await;

    assert!(result4.is_err());
    assert!(result4
        .unwrap_err()
        .to_string()
        .contains("usage limit exceeded"));

    Ok(())
}

#[tokio::test]
async fn test_certificate_based_enrollment() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());

    // Setup certificate enrollment config
    let cert_config = CertificateEnrollmentConfig {
        ca_cert_path: PathBuf::from("/etc/ssl/certs/ca.pem"),
        cert_role_mappings: vec![
            CertRoleMapping {
                cert_field: "CN".to_string(),
                pattern: "*.ops.example.com".to_string(),
                roles: vec!["operator".to_string(), "admin".to_string()],
                tenant: Some("ops-tenant".to_string()),
            },
            CertRoleMapping {
                cert_field: "OU".to_string(),
                pattern: "Engineering".to_string(),
                roles: vec!["developer".to_string()],
                tenant: None,
            },
            CertRoleMapping {
                cert_field: "O".to_string(),
                pattern: "Example Corp".to_string(),
                roles: vec!["employee".to_string()],
                tenant: None,
            },
        ],
        default_tenant: "default".to_string(),
        allow_self_signed: false,
    };

    let manager = IdentityEnrollmentManager::new(registry.clone(), Some(cert_config), None);

    // Test 1: Operations team member
    let node_id1 = NodeId::from_bytes(&[10u8; 32]).unwrap();
    let mut cert_attrs1 = HashMap::new();
    cert_attrs1.insert("CN".to_string(), "alice.ops.example.com".to_string());
    cert_attrs1.insert("OU".to_string(), "Operations".to_string());
    cert_attrs1.insert("O".to_string(), "Example Corp".to_string());

    let result1 = manager
        .enroll_with_certificate(node_id1, cert_attrs1)
        .await?;
    assert!(result1.success);
    assert_eq!(result1.user_id, "alice.ops.example.com");
    assert!(result1.roles.contains(&"operator".to_string()));
    assert!(result1.roles.contains(&"admin".to_string()));
    assert!(result1.roles.contains(&"employee".to_string()));
    assert_eq!(result1.tenant_id, "ops-tenant");

    // Test 2: Engineering team member
    let node_id2 = NodeId::from_bytes(&[20u8; 32]).unwrap();
    let mut cert_attrs2 = HashMap::new();
    cert_attrs2.insert("CN".to_string(), "bob@example.com".to_string());
    cert_attrs2.insert("OU".to_string(), "Engineering".to_string());
    cert_attrs2.insert("O".to_string(), "Example Corp".to_string());

    let result2 = manager
        .enroll_with_certificate(node_id2, cert_attrs2)
        .await?;
    assert!(result2.success);
    assert_eq!(result2.user_id, "bob@example.com");
    assert!(result2.roles.contains(&"developer".to_string()));
    assert!(result2.roles.contains(&"employee".to_string()));
    assert_eq!(result2.tenant_id, "default");

    // Test 3: No matching patterns
    let node_id3 = NodeId::from_bytes(&[30u8; 32]).unwrap();
    let mut cert_attrs3 = HashMap::new();
    cert_attrs3.insert("CN".to_string(), "external.contractor.com".to_string());
    cert_attrs3.insert("O".to_string(), "Contractor Inc".to_string());

    let result3 = manager.enroll_with_certificate(node_id3, cert_attrs3).await;
    assert!(result3.is_err());
    assert!(result3
        .unwrap_err()
        .to_string()
        .contains("No matching certificate role mappings"));

    Ok(())
}

#[tokio::test]
async fn test_enrollment_state_persistence() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("enrollment_state.json");

    // Create manager and generate tokens
    {
        let manager =
            IdentityEnrollmentManager::new(registry.clone(), None, Some(state_path.clone()));

        // Generate some tokens
        manager
            .generate_enrollment_token(
                "user1".to_string(),
                vec!["role1".to_string()],
                "tenant1".to_string(),
                Duration::from_secs(3600),
                false,
                None,
            )
            .await?;

        manager
            .generate_enrollment_token(
                "user2".to_string(),
                vec!["role2".to_string()],
                "tenant2".to_string(),
                Duration::from_secs(7200),
                true,
                Some(5),
            )
            .await?;
    }

    // Create new manager and load state
    {
        let manager2 = IdentityEnrollmentManager::new(registry.clone(), None, Some(state_path));

        manager2.load_state().await?;

        // List tokens to verify they were loaded
        let tokens = manager2.list_tokens().await;
        assert_eq!(tokens.len(), 2);

        // Verify token details
        let user1_token = tokens.iter().find(|t| t.user_id == "user1").unwrap();
        assert_eq!(user1_token.roles, vec!["role1"]);
        assert!(!user1_token.multi_use);

        let user2_token = tokens.iter().find(|t| t.user_id == "user2").unwrap();
        assert_eq!(user2_token.roles, vec!["role2"]);
        assert!(user2_token.multi_use);
        assert_eq!(user2_token.max_uses, Some(5));
    }

    Ok(())
}

#[tokio::test]
async fn test_token_expiration() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());
    let manager = IdentityEnrollmentManager::new(registry.clone(), None, None);

    // Generate token with very short validity
    let token = manager
        .generate_enrollment_token(
            "temp-user".to_string(),
            vec!["temp-role".to_string()],
            "tenant-1".to_string(),
            Duration::from_millis(100), // 100ms validity
            false,
            None,
        )
        .await?;

    // Wait for token to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Try to use expired token
    let node_id = NodeId::from_bytes(&[99u8; 32]).unwrap();
    let result = manager
        .enroll_with_token(node_id, &token.token_id, &token.secret)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expired"));

    Ok(())
}

#[tokio::test]
async fn test_concurrent_enrollment() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());
    let manager = Arc::new(IdentityEnrollmentManager::new(registry.clone(), None, None));

    // Generate a multi-use token
    let token = manager
        .generate_enrollment_token(
            "concurrent-user".to_string(),
            vec!["role1".to_string()],
            "tenant-1".to_string(),
            Duration::from_secs(3600),
            true,
            Some(10),
        )
        .await?;

    // Spawn multiple concurrent enrollment attempts
    let mut handles = vec![];
    for i in 0..5 {
        let manager_clone = manager.clone();
        let token_id = token.token_id.clone();
        let secret = token.secret.clone();

        let handle = tokio::spawn(async move {
            let node_id = NodeId::from_bytes(&[i as u8; 32]).unwrap();
            manager_clone
                .enroll_with_token(node_id, &token_id, &secret)
                .await
        });

        handles.push(handle);
    }

    // Wait for all enrollments to complete
    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(result)) = handle.await {
            if result.success {
                success_count += 1;
            }
        }
    }

    // All 5 should succeed (under the limit of 10)
    assert_eq!(success_count, 5);

    // Verify use count
    let tokens = manager.list_tokens().await;
    let token_info = tokens
        .iter()
        .find(|t| t.token_id == token.token_id)
        .unwrap();
    assert_eq!(token_info.use_count, 5);

    Ok(())
}

#[tokio::test]
async fn test_wildcard_pattern_matching() -> BlixardResult<()> {
    let registry = Arc::new(NodeIdentityRegistry::new());
    let manager = IdentityEnrollmentManager::new(registry.clone(), None, None);

    // Test various wildcard patterns
    assert!(manager.matches_pattern("test.example.com", "*.example.com"));
    assert!(manager.matches_pattern("sub.test.example.com", "*.example.com"));
    assert!(!manager.matches_pattern("example.com", "*.example.com"));
    assert!(!manager.matches_pattern("test.example.org", "*.example.com"));

    assert!(manager.matches_pattern("node-123", "node-*"));
    assert!(manager.matches_pattern("node-abc-xyz", "node-*"));
    assert!(!manager.matches_pattern("server-123", "node-*"));

    assert!(manager.matches_pattern("prefix-middle-suffix", "prefix-*-suffix"));
    assert!(!manager.matches_pattern("prefix-middle", "prefix-*-suffix"));

    assert!(manager.matches_pattern("anything", "*"));
    assert!(manager.matches_pattern("", "*"));

    Ok(())
}
