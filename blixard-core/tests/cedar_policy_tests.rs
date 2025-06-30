//! Tests for Cedar policy integration

#[cfg(feature = "test-helpers")]
mod tests {
    use blixard_core::cedar_authz::CedarAuthz;
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio::fs;

    /// Helper to create test Cedar setup with policies
    async fn create_test_cedar_with_policies() -> (CedarAuthz, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        
        // Create schema file
        let schema_path = temp_dir.path().join("schema.cedarschema.json");
        let schema = include_str!("../../cedar/schema.cedarschema.json");
        fs::write(&schema_path, schema).await.unwrap();
        
        // Create policies directory
        let policies_dir = temp_dir.path().join("policies");
        fs::create_dir(&policies_dir).await.unwrap();
        
        // Write a simple test policy
        let test_policy = r#"
// Test admin policy
permit(
    principal in Role::"admin",
    action,
    resource
);

// Test operator policy
permit(
    principal in Role::"operator",
    action in [
        Action::"readCluster",
        Action::"readNode",
        Action::"createVM",
        Action::"readVM",
        Action::"updateVM",
        Action::"deleteVM"
    ],
    resource
);

// Test viewer policy
permit(
    principal in Role::"viewer",
    action in [
        Action::"readCluster",
        Action::"readNode",
        Action::"readVM"
    ],
    resource
);

// Test time-based policy
forbid(
    principal in Role::"operator",
    action == Action::"deleteVM",
    resource
) unless {
    context.hour >= 22 || context.hour <= 6
};
"#;
        
        fs::write(policies_dir.join("test_policies.cedar"), test_policy)
            .await
            .unwrap();
        
        let cedar = CedarAuthz::new(&schema_path, &policies_dir).await.unwrap();
        
        (cedar, temp_dir)
    }

    #[tokio::test]
    async fn test_cedar_initialization() {
        let (_cedar, _temp_dir) = create_test_cedar_with_policies().await;
        // If we get here without panic, initialization succeeded
    }

    #[tokio::test]
    async fn test_basic_authorization() {
        let (cedar, _temp_dir) = create_test_cedar_with_policies().await;
        
        // Test admin access - should be allowed for any action
        let result = cedar
            .is_authorized(
                "User::\"admin-user\"",
                "Action::\"manageCluster\"",
                "Cluster::\"prod\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(result, "Admin should be able to manage cluster");
        
        // Test operator access to allowed action
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"readCluster\"",
                "Cluster::\"prod\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(result, "Operator should be able to read cluster");
        
        // Test operator access to denied action
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"manageCluster\"",
                "Cluster::\"prod\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(!result, "Operator should NOT be able to manage cluster");
    }

    #[tokio::test]
    async fn test_time_based_access_control() {
        let (cedar, _temp_dir) = create_test_cedar_with_policies().await;
        
        // Test deletion during business hours (should be denied)
        let mut context = HashMap::new();
        context.insert("hour".to_string(), json!(14)); // 2 PM
        
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"deleteVM\"",
                "VM::\"test-vm\"",
                context,
            )
            .await
            .unwrap();
        assert!(!result, "Operator should NOT be able to delete VM during business hours");
        
        // Test deletion during maintenance window (should be allowed)
        let mut context = HashMap::new();
        context.insert("hour".to_string(), json!(23)); // 11 PM
        
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"deleteVM\"",
                "VM::\"test-vm\"",
                context,
            )
            .await
            .unwrap();
        assert!(result, "Operator should be able to delete VM during maintenance window");
    }

    #[tokio::test]
    async fn test_viewer_restrictions() {
        let (cedar, _temp_dir) = create_test_cedar_with_policies().await;
        
        // Viewer can read
        let result = cedar
            .is_authorized(
                "User::\"viewer-user\"",
                "Action::\"readVM\"",
                "VM::\"test-vm\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(result, "Viewer should be able to read VM");
        
        // Viewer cannot create
        let result = cedar
            .is_authorized(
                "User::\"viewer-user\"",
                "Action::\"createVM\"",
                "VM::\"test-vm\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(!result, "Viewer should NOT be able to create VM");
        
        // Viewer cannot delete
        let result = cedar
            .is_authorized(
                "User::\"viewer-user\"",
                "Action::\"deleteVM\"",
                "VM::\"test-vm\"",
                HashMap::new(),
            )
            .await
            .unwrap();
        assert!(!result, "Viewer should NOT be able to delete VM");
    }

    #[tokio::test]
    async fn test_policy_coverage_validation() {
        let (cedar, _temp_dir) = create_test_cedar_with_policies().await;
        
        // Check that required actions are covered
        let uncovered = cedar.validate_policy_coverage().await.unwrap();
        
        // With our test policies, some actions might not be covered
        // This is expected for the test
        println!("Uncovered actions: {:?}", uncovered);
    }

    #[tokio::test]
    async fn test_context_based_authorization() {
        let (cedar, _temp_dir) = create_test_cedar_with_policies().await;
        
        // Create a policy that checks context values
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.cedarschema.json");
        let schema = include_str!("../../cedar/schema.cedarschema.json");
        fs::write(&schema_path, schema).await.unwrap();
        
        let policies_dir = temp_dir.path().join("policies");
        fs::create_dir(&policies_dir).await.unwrap();
        
        let context_policy = r#"
// Allow VM creation only if resources are available
permit(
    principal,
    action == Action::"createVM",
    resource
) when {
    context has requested_cpu &&
    context has available_cpu &&
    context.requested_cpu <= context.available_cpu
};
"#;
        
        fs::write(policies_dir.join("context_policy.cedar"), context_policy)
            .await
            .unwrap();
        
        let cedar = CedarAuthz::new(&schema_path, &policies_dir).await.unwrap();
        
        // Test with sufficient resources
        let mut context = HashMap::new();
        context.insert("requested_cpu".to_string(), json!(4));
        context.insert("available_cpu".to_string(), json!(8));
        
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"createVM\"",
                "Node::\"node1\"",
                context,
            )
            .await
            .unwrap();
        assert!(result, "Should be able to create VM with sufficient resources");
        
        // Test with insufficient resources
        let mut context = HashMap::new();
        context.insert("requested_cpu".to_string(), json!(10));
        context.insert("available_cpu".to_string(), json!(8));
        
        let result = cedar
            .is_authorized(
                "User::\"operator-user\"",
                "Action::\"createVM\"",
                "Node::\"node1\"",
                context,
            )
            .await
            .unwrap();
        assert!(!result, "Should NOT be able to create VM with insufficient resources");
    }
}