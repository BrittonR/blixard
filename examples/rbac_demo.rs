//! RBAC demonstration example
//!
//! This example shows how Role-Based Access Control (RBAC) works in Blixard.

use blixard_core::{
    rbac::{RbacManager, Action, ResourceType, ResourcePermissionChecker},
    security::{SecurityManager, Permission},
    config_v2::{SecurityConfig, AuthConfig, TlsConfig},
    error::BlixardResult,
};
use std::sync::Arc;
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("=== Blixard RBAC Demo ===\n");
    
    // Create temporary policy file
    let policy_file = NamedTempFile::new()?;
    let policy_path = policy_file.path().to_string_lossy().to_string();
    
    // Initialize RBAC manager
    println!("1. Initializing RBAC system...");
    let rbac = Arc::new(RbacManager::with_default_model(policy_path).await?);
    rbac.init_default_policies().await?;
    
    // Create permission checker
    let checker = ResourcePermissionChecker::new(rbac.clone());
    
    // Demo multi-tenancy setup
    println!("\n2. Setting up multi-tenant environment...");
    
    // Tenant 1: ACME Corp
    rbac.add_role_for_user("alice", "admin", Some("acme")).await?;
    rbac.add_role_for_user("bob", "operator", Some("acme")).await?;
    rbac.add_role_for_user("charlie", "viewer", Some("acme")).await?;
    
    // Tenant 2: Tech Inc
    rbac.add_role_for_user("david", "admin", Some("tech")).await?;
    rbac.add_role_for_user("eve", "operator", Some("tech")).await?;
    
    println!("   - Created 2 tenants: 'acme' and 'tech'");
    println!("   - ACME: alice (admin), bob (operator), charlie (viewer)");
    println!("   - Tech: david (admin), eve (operator)");
    
    // Test cross-tenant isolation
    println!("\n3. Testing tenant isolation...");
    
    // Alice (ACME admin) can manage ACME resources
    assert!(rbac.enforce("alice", "vm", &Action::Write, Some("acme")).await?);
    println!("   ✓ Alice (ACME admin) can create VMs in ACME tenant");
    
    // Alice cannot access Tech Inc resources
    assert!(!rbac.enforce("alice", "vm", &Action::Write, Some("tech")).await?);
    println!("   ✓ Alice (ACME admin) CANNOT create VMs in Tech tenant");
    
    // Test role-based permissions
    println!("\n4. Testing role-based permissions...");
    
    // Bob (operator) can manage VMs
    assert!(rbac.enforce("bob", "vm", &Action::Write, Some("acme")).await?);
    assert!(rbac.enforce("bob", "vm", &Action::Read, Some("acme")).await?);
    println!("   ✓ Bob (operator) can read and write VMs");
    
    // Bob cannot perform admin tasks
    assert!(!rbac.enforce("bob", "cluster", &Action::Admin, Some("acme")).await?);
    println!("   ✓ Bob (operator) CANNOT perform cluster admin tasks");
    
    // Charlie (viewer) can only read
    assert!(rbac.enforce("charlie", "vm", &Action::Read, Some("acme")).await?);
    assert!(!rbac.enforce("charlie", "vm", &Action::Write, Some("acme")).await?);
    println!("   ✓ Charlie (viewer) can read but NOT write VMs");
    
    // Test resource-specific permissions
    println!("\n5. Testing resource-specific permissions...");
    
    // Grant Frank permission to specific VMs only
    rbac.add_role_for_user("frank", "viewer", Some("acme")).await?;
    
    // Grant access to specific VMs
    checker.grant_resource_permission(
        "frank",
        &ResourceType::Vm,
        "web-server-1",
        &Action::Write,
        Some("acme"),
    ).await?;
    
    checker.grant_resource_permission(
        "frank",
        &ResourceType::Vm,
        "web-server-2",
        &Action::Write,
        Some("acme"),
    ).await?;
    
    // Test Frank's permissions
    assert!(checker.can_access_vm("frank", "web-server-1", &Action::Write, Some("acme")).await?);
    assert!(checker.can_access_vm("frank", "web-server-2", &Action::Write, Some("acme")).await?);
    assert!(!checker.can_access_vm("frank", "database-1", &Action::Write, Some("acme")).await?);
    
    println!("   ✓ Frank can manage web-server-1 and web-server-2");
    println!("   ✓ Frank CANNOT manage database-1");
    
    // Test node-specific permissions
    println!("\n6. Testing node management permissions...");
    
    // Grant Grace permission to manage specific nodes
    rbac.add_policy("grace", "node", &Action::Read, Some("*")).await?;
    checker.grant_resource_permission(
        "grace",
        &ResourceType::Node,
        "node-1",
        &Action::Admin,
        None,
    ).await?;
    
    assert!(checker.can_manage_node("grace", "node-1", &Action::Admin).await?);
    assert!(!checker.can_manage_node("grace", "node-2", &Action::Admin).await?);
    
    println!("   ✓ Grace can admin node-1");
    println!("   ✓ Grace CANNOT admin node-2");
    
    // Demonstrate permission inheritance
    println!("\n7. Testing permission inheritance...");
    
    // Create a custom role that inherits from operator
    rbac.add_role_for_user("henry", "operator", Some("acme")).await?;
    rbac.add_policy("henry", "metrics", &Action::Write, Some("acme")).await?;
    
    // Henry has operator permissions plus metrics write
    assert!(rbac.enforce("henry", "vm", &Action::Write, Some("acme")).await?);
    assert!(rbac.enforce("henry", "metrics", &Action::Write, Some("acme")).await?);
    
    println!("   ✓ Henry inherits operator permissions");
    println!("   ✓ Henry also has custom metrics write permission");
    
    // Test dynamic permission changes
    println!("\n8. Testing dynamic permission updates...");
    
    // Revoke Bob's operator role
    rbac.remove_role_for_user("bob", "operator", Some("acme")).await?;
    
    // Bob can no longer write VMs
    assert!(!rbac.enforce("bob", "vm", &Action::Write, Some("acme")).await?);
    println!("   ✓ Bob's operator role revoked - cannot write VMs anymore");
    
    // Grant Bob a custom permission
    rbac.add_policy("bob", "vm:critical-app", &Action::Admin, Some("acme")).await?;
    assert!(rbac.enforce("bob", "vm:critical-app", &Action::Admin, Some("acme")).await?);
    println!("   ✓ Bob granted admin access to specific VM 'critical-app'");
    
    // Demonstrate audit trail
    println!("\n9. Role and permission audit...");
    
    // Get all users with admin role in ACME
    let acme_admins = rbac.get_users_for_role("admin", Some("acme")).await?;
    println!("   ACME admins: {:?}", acme_admins);
    
    // Get all roles for Alice
    let alice_roles = rbac.get_roles_for_user("alice", Some("acme")).await?;
    println!("   Alice's roles in ACME: {:?}", alice_roles);
    
    // Save policies
    println!("\n10. Saving RBAC policies...");
    rbac.save_policies().await?;
    println!("   ✓ Policies saved to disk");
    
    println!("\n=== RBAC Demo Complete ===");
    println!("\nKey concepts demonstrated:");
    println!("- Multi-tenant isolation");
    println!("- Role-based permissions (admin, operator, viewer)");
    println!("- Resource-specific permissions");
    println!("- Dynamic permission management");
    println!("- Permission inheritance");
    println!("- Audit capabilities");
    
    Ok(())
}