//! Simple Cedar demo that doesn't depend on the rest of the codebase

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use std::collections::HashMap;
use std::str::FromStr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔒 Cedar Policy Engine Simple Demo\n");

    // Create a simple policy
    let policy_text = r#"
        permit(
            principal == User::"alice",
            action == Action::"read",
            resource == Document::"public-doc"
        );
        
        permit(
            principal == User::"bob",
            action == Action::"read",
            resource == Document::"public-doc"
        );
        
        forbid(
            principal == User::"bob",
            action == Action::"write",
            resource
        );
    "#;

    // Parse the policy set
    let policy_set = PolicySet::from_str(policy_text)?;
    println!("✅ Loaded {} policies", policy_set.policies().count());

    // Create an authorizer
    let authorizer = Authorizer::new();

    // Create empty entities (no entity data needed for this simple example)
    let entities = Entities::empty();

    // Test Case 1: Alice reading public doc (should be allowed)
    let request1 = Request::new(
        Some(EntityUid::from_str("User::\"alice\"")?),
        Some(EntityUid::from_str("Action::\"read\"")?),
        Some(EntityUid::from_str("Document::\"public-doc\"")?),
        Context::empty(),
        None,
    )?;

    let response1 = authorizer.is_authorized(&request1, &policy_set, &entities);
    println!(
        "\n1. Alice reading public doc: {}",
        if matches!(response1.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED"
        }
    );

    // Test Case 2: Bob reading public doc (should be allowed)
    let request2 = Request::new(
        Some(EntityUid::from_str("User::\"bob\"")?),
        Some(EntityUid::from_str("Action::\"read\"")?),
        Some(EntityUid::from_str("Document::\"public-doc\"")?),
        Context::empty(),
        None,
    )?;

    let response2 = authorizer.is_authorized(&request2, &policy_set, &entities);
    println!(
        "2. Bob reading public doc: {}",
        if matches!(response2.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED"
        }
    );

    // Test Case 3: Bob writing (should be denied by forbid rule)
    let request3 = Request::new(
        Some(EntityUid::from_str("User::\"bob\"")?),
        Some(EntityUid::from_str("Action::\"write\"")?),
        Some(EntityUid::from_str("Document::\"public-doc\"")?),
        Context::empty(),
        None,
    )?;

    let response3 = authorizer.is_authorized(&request3, &policy_set, &entities);
    println!(
        "3. Bob writing public doc: {}",
        if matches!(response3.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED (forbid rule)"
        }
    );

    // Test Case 4: Unknown user (should be denied - no matching permit)
    let request4 = Request::new(
        Some(EntityUid::from_str("User::\"charlie\"")?),
        Some(EntityUid::from_str("Action::\"read\"")?),
        Some(EntityUid::from_str("Document::\"public-doc\"")?),
        Context::empty(),
        None,
    )?;

    let response4 = authorizer.is_authorized(&request4, &policy_set, &entities);
    println!(
        "4. Charlie reading public doc: {}",
        if matches!(response4.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED (no matching permit)"
        }
    );

    // Test with context
    println!("\n📋 Testing with context...");

    let context_policy = r#"
        permit(
            principal,
            action == Action::"access",
            resource
        ) when {
            context.time_of_day >= 9 && 
            context.time_of_day <= 17
        };
    "#;

    let context_policy_set = PolicySet::from_str(context_policy)?;

    // Create context for business hours
    let mut context_map = HashMap::new();
    context_map.insert("time_of_day".to_string(), serde_json::json!(14)); // 2 PM

    let context_json = serde_json::Value::Object(
        context_map
            .into_iter()
            .collect::<serde_json::Map<String, serde_json::Value>>(),
    );

    let context = Context::from_json_value(context_json, None)?;

    let request5 = Request::new(
        Some(EntityUid::from_str("User::\"alice\"")?),
        Some(EntityUid::from_str("Action::\"access\"")?),
        Some(EntityUid::from_str("Resource::\"data\"")?),
        context,
        None,
    )?;

    let response5 = authorizer.is_authorized(&request5, &context_policy_set, &entities);
    println!(
        "5. Access during business hours (2 PM): {}",
        if matches!(response5.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED"
        }
    );

    // Test outside business hours
    let mut context_map = HashMap::new();
    context_map.insert("time_of_day".to_string(), serde_json::json!(22)); // 10 PM

    let context_json = serde_json::Value::Object(
        context_map
            .into_iter()
            .collect::<serde_json::Map<String, serde_json::Value>>(),
    );

    let context = Context::from_json_value(context_json, None)?;

    let request6 = Request::new(
        Some(EntityUid::from_str("User::\"alice\"")?),
        Some(EntityUid::from_str("Action::\"access\"")?),
        Some(EntityUid::from_str("Resource::\"data\"")?),
        context,
        None,
    )?;

    let response6 = authorizer.is_authorized(&request6, &context_policy_set, &entities);
    println!(
        "6. Access after hours (10 PM): {}",
        if matches!(response6.decision(), Decision::Allow) {
            "✅ ALLOWED"
        } else {
            "❌ DENIED (outside business hours)"
        }
    );

    println!("\n✨ Cedar demo complete!");

    Ok(())
}
