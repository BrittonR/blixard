//! Example demonstrating Cedar policy engine integration

use blixard_core::cedar_authz::CedarAuthz;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Paths to Cedar files
    let schema_path = PathBuf::from("cedar/schema.cedarschema.json");
    let policies_dir = PathBuf::from("cedar/policies");
    
    println!("🔒 Initializing Cedar Policy Engine...");
    
    // Create Cedar authorization engine
    let cedar = CedarAuthz::new(&schema_path, &policies_dir).await?;
    
    println!("✅ Cedar initialized with schema and policies");
    
    // Add some example entities
    println!("\n📝 Adding entities...");
    
    // Add roles
    cedar.add_entity("Role", "admin", HashMap::new(), vec![]).await?;
    cedar.add_entity("Role", "operator", HashMap::new(), vec![]).await?;
    cedar.add_entity("Role", "viewer", HashMap::new(), vec![]).await?;
    println!("  ✓ Added roles: admin, operator, viewer");
    
    // Add a tenant
    let mut tenant_attrs = HashMap::new();
    tenant_attrs.insert("name".to_string(), json!("acme-corp"));
    tenant_attrs.insert("tier".to_string(), json!("pro"));
    tenant_attrs.insert("quota_cpu".to_string(), json!(1000));
    tenant_attrs.insert("quota_memory".to_string(), json!(8192));
    tenant_attrs.insert("quota_vms".to_string(), json!(50));
    
    cedar.add_entity("Tenant", "acme", tenant_attrs, vec![]).await?;
    println!("  ✓ Added tenant: acme-corp");
    
    // Add a cluster
    let mut cluster_attrs = HashMap::new();
    cluster_attrs.insert("name".to_string(), json!("prod-cluster"));
    cluster_attrs.insert("region".to_string(), json!("us-west-2"));
    
    cedar.add_entity("Cluster", "prod", cluster_attrs, vec![]).await?;
    println!("  ✓ Added cluster: prod-cluster");
    
    // Add users
    let mut alice_attrs = HashMap::new();
    alice_attrs.insert("email".to_string(), json!("alice@acme.com"));
    alice_attrs.insert("tenant_id".to_string(), json!("acme"));
    alice_attrs.insert("created_at".to_string(), json!(1234567890));
    
    cedar.add_entity(
        "User",
        "alice",
        alice_attrs,
        vec!["Role::\"admin\"".to_string(), "Tenant::\"acme\"".to_string()],
    ).await?;
    println!("  ✓ Added user: alice (admin)");
    
    let mut bob_attrs = HashMap::new();
    bob_attrs.insert("email".to_string(), json!("bob@acme.com"));
    bob_attrs.insert("tenant_id".to_string(), json!("acme"));
    bob_attrs.insert("created_at".to_string(), json!(1234567890));
    
    cedar.add_entity(
        "User",
        "bob",
        bob_attrs,
        vec!["Role::\"operator\"".to_string(), "Tenant::\"acme\"".to_string()],
    ).await?;
    println!("  ✓ Added user: bob (operator)");
    
    // Test authorization scenarios
    println!("\n🔍 Testing authorization scenarios...");
    
    // Scenario 1: Admin reading cluster
    let result = cedar.is_authorized(
        "User::\"alice\"",
        "Action::\"readCluster\"",
        "Cluster::\"prod\"",
        HashMap::new(),
    ).await?;
    println!("\n1. Can Alice (admin) read the cluster? {}", if result { "✅ YES" } else { "❌ NO" });
    
    // Scenario 2: Admin managing cluster
    let result = cedar.is_authorized(
        "User::\"alice\"",
        "Action::\"manageCluster\"",
        "Cluster::\"prod\"",
        HashMap::new(),
    ).await?;
    println!("2. Can Alice (admin) manage the cluster? {}", if result { "✅ YES" } else { "❌ NO" });
    
    // Scenario 3: Operator managing cluster
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"manageCluster\"",
        "Cluster::\"prod\"",
        HashMap::new(),
    ).await?;
    println!("3. Can Bob (operator) manage the cluster? {}", if result { "✅ YES" } else { "❌ NO" });
    
    // Scenario 4: Time-based access (VM deletion)
    println!("\n⏰ Testing time-based access control...");
    
    // Add a VM
    let mut vm_attrs = HashMap::new();
    vm_attrs.insert("name".to_string(), json!("web-server-1"));
    vm_attrs.insert("tenant_id".to_string(), json!("acme"));
    vm_attrs.insert("priority".to_string(), json!(100));
    vm_attrs.insert("preemptible".to_string(), json!(true));
    vm_attrs.insert("state".to_string(), json!("Running"));
    
    cedar.add_entity(
        "VM",
        "vm1",
        vm_attrs,
        vec!["Tenant::\"acme\"".to_string()],
    ).await?;
    
    // Test during business hours (2 PM)
    let mut context = HashMap::new();
    context.insert("hour".to_string(), json!(14));
    
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"deleteVM\"",
        "VM::\"vm1\"",
        context.clone(),
    ).await?;
    println!("4. Can Bob delete VM at 2 PM? {}", if result { "✅ YES" } else { "❌ NO (maintenance window only)" });
    
    // Test during maintenance window (11 PM)
    context.insert("hour".to_string(), json!(23));
    
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"deleteVM\"",
        "VM::\"vm1\"",
        context,
    ).await?;
    println!("5. Can Bob delete VM at 11 PM? {}", if result { "✅ YES (maintenance window)" } else { "❌ NO" });
    
    // Scenario 5: Resource quota enforcement
    println!("\n📊 Testing resource quota enforcement...");
    
    let mut context = HashMap::new();
    context.insert("current_vm_count".to_string(), json!(45));
    context.insert("requested_cpu".to_string(), json!(4));
    context.insert("current_cpu_usage".to_string(), json!(800));
    
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"createVM\"",
        "Tenant::\"acme\"",
        context.clone(),
    ).await?;
    println!("6. Can Bob create VM (45/50 VMs, 800/1000 CPU)? {}", if result { "✅ YES" } else { "❌ NO" });
    
    // Try to exceed quota
    context.insert("current_vm_count".to_string(), json!(50));
    
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"createVM\"",
        "Tenant::\"acme\"",
        context,
    ).await?;
    println!("7. Can Bob create VM (50/50 VMs)? {}", if result { "✅ YES" } else { "❌ NO (quota exceeded)" });
    
    // Test multi-tenancy isolation
    println!("\n🏢 Testing multi-tenancy isolation...");
    
    // Add another tenant and VM
    let mut other_tenant_attrs = HashMap::new();
    other_tenant_attrs.insert("name".to_string(), json!("competitor-corp"));
    other_tenant_attrs.insert("tier".to_string(), json!("pro"));
    
    cedar.add_entity("Tenant", "competitor", other_tenant_attrs, vec![]).await?;
    
    let mut other_vm_attrs = HashMap::new();
    other_vm_attrs.insert("name".to_string(), json!("competitor-vm"));
    other_vm_attrs.insert("tenant_id".to_string(), json!("competitor"));
    
    cedar.add_entity(
        "VM",
        "competitor-vm",
        other_vm_attrs,
        vec!["Tenant::\"competitor\"".to_string()],
    ).await?;
    
    let result = cedar.is_authorized(
        "User::\"bob\"",
        "Action::\"readVM\"",
        "VM::\"competitor-vm\"",
        HashMap::new(),
    ).await?;
    println!("8. Can Bob read competitor's VM? {}", if result { "✅ YES" } else { "❌ NO (tenant isolation)" });
    
    // Validate policy coverage
    println!("\n📋 Validating policy coverage...");
    let uncovered = cedar.validate_policy_coverage().await?;
    if uncovered.is_empty() {
        println!("✅ All actions are covered by policies!");
    } else {
        println!("⚠️  Uncovered actions: {:?}", uncovered);
    }
    
    println!("\n🎉 Cedar authorization demo complete!");
    
    Ok(())
}