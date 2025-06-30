//! Test Cedar integration with SecurityManager

use blixard_core::{
    security::{SecurityManager, default_dev_security_config},
    config_v2::{SecurityConfig, AuthConfig, TlsConfig},
    error::BlixardResult,
};
use std::collections::HashMap;
use std::path::Path;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("🔒 Testing Cedar integration with SecurityManager\n");
    
    // Create security config with auth enabled
    let config = SecurityConfig {
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: None,
        },
        tls: TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
    };
    
    // Initialize SecurityManager (will try to load Cedar)
    let security_manager = SecurityManager::new(config).await?;
    
    // Test Cedar authorization scenarios
    println!("Testing Cedar authorization scenarios:\n");
    
    // Test 1: Admin accessing cluster
    let mut context = HashMap::new();
    context.insert("tenant_id".to_string(), serde_json::json!("default"));
    
    let result = security_manager.check_permission_cedar(
        "admin",
        "readCluster",
        "Cluster::\"prod\"",
        context.clone(),
    ).await?;
    
    println!("1. Admin reading cluster: {}", if result { "✅ ALLOWED" } else { "❌ DENIED" });
    
    // Test 2: Operator creating VM
    let result = security_manager.check_permission_cedar(
        "operator",
        "createVM",
        "Node::\"node-1\"",
        context.clone(),
    ).await?;
    
    println!("2. Operator creating VM: {}", if result { "✅ ALLOWED" } else { "❌ DENIED" });
    
    // Test 3: Viewer trying to delete VM
    let result = security_manager.check_permission_cedar(
        "viewer",
        "deleteVM",
        "VM::\"vm-123\"",
        context.clone(),
    ).await?;
    
    println!("3. Viewer deleting VM: {}", if result { "✅ ALLOWED" } else { "❌ DENIED" });
    
    // Test 4: Time-based access (operator deleting VM during business hours)
    context.insert("hour".to_string(), serde_json::json!(14)); // 2 PM
    let result = security_manager.check_permission_cedar(
        "operator",
        "deleteVM",
        "VM::\"vm-123\"",
        context.clone(),
    ).await?;
    
    println!("4. Operator deleting VM at 2 PM: {}", if result { "✅ ALLOWED" } else { "❌ DENIED" });
    
    // Test 5: Time-based access (operator deleting VM during maintenance)
    context.insert("hour".to_string(), serde_json::json!(23)); // 11 PM
    let result = security_manager.check_permission_cedar(
        "operator",
        "deleteVM",
        "VM::\"vm-123\"",
        context,
    ).await?;
    
    println!("5. Operator deleting VM at 11 PM: {}", if result { "✅ ALLOWED" } else { "❌ DENIED" });
    
    println!("\n✨ Cedar integration test complete!");
    
    // Check if Cedar files exist
    let schema_path = Path::new("cedar/schema.cedarschema.json");
    let policies_dir = Path::new("cedar/policies");
    
    if schema_path.exists() && policies_dir.exists() {
        println!("\n✅ Cedar files found - using Cedar for authorization");
    } else {
        println!("\n⚠️  Cedar files not found - using fallback RBAC");
    }
    
    Ok(())
}