//! Cedar Policy Engine Demo for Blixard

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use std::collections::HashMap;
use std::str::FromStr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔒 Cedar Policy Engine Demo for Blixard\n");

    // Define Blixard-style policies
    let policy_text = r#"
        // Admin has full access
        permit(
            principal,
            action,
            resource
        ) when {
            principal == User::"admin"
        };
        
        // Operator can manage VMs
        permit(
            principal,
            action in [
                Action::"readCluster",
                Action::"readNode", 
                Action::"createVM",
                Action::"readVM",
                Action::"updateVM",
                Action::"deleteVM"
            ],
            resource
        ) when {
            principal == User::"operator"
        };
        
        // Time-based restriction for operators
        forbid(
            principal,
            action == Action::"deleteVM",
            resource
        ) when {
            principal == User::"operator"
        } unless {
            context.hour >= 22 || context.hour <= 6
        };
        
        // Viewer has read-only access
        permit(
            principal,
            action in [
                Action::"readCluster",
                Action::"readNode",
                Action::"readVM"
            ],
            resource
        ) when {
            principal == User::"viewer"
        };
        
        // Resource quota check
        forbid(
            principal,
            action == Action::"createVM",
            resource
        ) when {
            context has current_vm_count &&
            context has vm_quota &&
            context.current_vm_count >= context.vm_quota
        };
    "#;

    // Parse the policy set
    let policy_set = PolicySet::from_str(policy_text)?;
    println!("✅ Loaded {} policies\n", policy_set.policies().count());

    // Create an authorizer
    let authorizer = Authorizer::new();

    // Create empty entities (simplified for demo)
    let entities = Entities::empty();

    // Test scenarios
    println!("📋 Testing Blixard authorization scenarios:\n");

    // Scenario 1: Admin managing cluster
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"admin\"",
        "Action::\"manageCluster\"",
        "Cluster::\"prod\"",
        HashMap::new(),
        "Admin managing cluster",
    )?;

    // Scenario 2: Operator creating VM
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"operator\"",
        "Action::\"createVM\"",
        "VM::\"web-1\"",
        HashMap::new(),
        "Operator creating VM",
    )?;

    // Scenario 3: Operator deleting VM during business hours
    let mut context = HashMap::new();
    context.insert("hour".to_string(), serde_json::json!(14)); // 2 PM
    
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"operator\"",
        "Action::\"deleteVM\"",
        "VM::\"web-1\"",
        context,
        "Operator deleting VM at 2 PM (business hours)",
    )?;

    // Scenario 4: Operator deleting VM during maintenance window
    let mut context = HashMap::new();
    context.insert("hour".to_string(), serde_json::json!(23)); // 11 PM
    
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"operator\"",
        "Action::\"deleteVM\"",
        "VM::\"web-1\"",
        context,
        "Operator deleting VM at 11 PM (maintenance)",
    )?;

    // Scenario 5: Viewer trying to create VM
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"viewer\"",
        "Action::\"createVM\"",
        "VM::\"web-2\"",
        HashMap::new(),
        "Viewer trying to create VM",
    )?;

    // Scenario 6: Viewer reading VM
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"viewer\"",
        "Action::\"readVM\"",
        "VM::\"web-1\"",
        HashMap::new(),
        "Viewer reading VM",
    )?;

    // Scenario 7: Quota enforcement
    let mut context = HashMap::new();
    context.insert("current_vm_count".to_string(), serde_json::json!(10));
    context.insert("vm_quota".to_string(), serde_json::json!(10));
    
    test_authorization(
        &authorizer,
        &policy_set,
        &entities,
        "User::\"operator\"",
        "Action::\"createVM\"",
        "VM::\"web-11\"",
        context,
        "Operator creating VM at quota limit",
    )?;

    println!("\n✨ Cedar authorization demo complete!");
    println!("\nKey Benefits of Cedar for Blixard:");
    println!("✅ Policy-as-Code: Policies are readable and version-controlled");
    println!("✅ Formal Verification: Mathematically prove policies are correct");
    println!("✅ Flexible Authorization: Support RBAC, ABAC, and custom rules");
    println!("✅ Performance: Sub-millisecond authorization decisions");
    println!("✅ Separation of Concerns: Authorization logic separate from code");

    Ok(())
}

fn test_authorization(
    authorizer: &Authorizer,
    policy_set: &PolicySet,
    entities: &Entities,
    principal: &str,
    action: &str,
    resource: &str,
    context_map: HashMap<String, serde_json::Value>,
    description: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build context
    let context = if context_map.is_empty() {
        Context::empty()
    } else {
        let context_json = serde_json::Value::Object(
            context_map
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        );
        Context::from_json_value(context_json, None)?
    };

    // Create request (Cedar 3.x requires an optional schema parameter)
    let request = Request::new(
        Some(EntityUid::from_str(principal)?),
        Some(EntityUid::from_str(action)?),
        Some(EntityUid::from_str(resource)?),
        context,
        None, // No schema for this demo
    )?;

    // Make authorization decision
    let response = authorizer.is_authorized(&request, policy_set, entities);
    
    let result = match response.decision() {
        Decision::Allow => "✅ ALLOWED",
        Decision::Deny => "❌ DENIED",
    };

    println!("{}: {}", description, result);
    
    Ok(())
}