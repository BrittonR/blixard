//! Demo of certificate-based enrollment for Iroh nodes
//!
//! This example shows how to:
//! 1. Generate enrollment tokens for manual distribution
//! 2. Configure certificate-based auto-enrollment
//! 3. Enroll nodes and verify their permissions

use blixard_core::{
    config::{AuthConfig, SecurityConfig, TlsConfig},
    error::BlixardResult,
    node_shared::SharedNodeState,
    security::{default_dev_security_config, SecurityManager},
    transport::{
        iroh_identity_enrollment::{
            CertRoleMapping, CertificateEnrollmentConfig, IdentityEnrollmentManager,
        },
        iroh_middleware::{IrohMiddleware, NodeIdentityRegistry},
        iroh_secure_vm_service::SecureIrohVmService,
        secure_iroh_protocol_handler::SecureIrohServiceBuilder,
    },
    types::{NodeConfig, NodeId as BlixardNodeId},
};
use iroh::{Endpoint, NodeId};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔐 Iroh Node Enrollment Demo\n");

    // Create security infrastructure
    let security_config = SecurityConfig {
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

    let security_manager = Arc::new(SecurityManager::new(security_config).await?);
    let identity_registry = Arc::new(NodeIdentityRegistry::new());

    // Configure certificate-based enrollment
    let cert_config = CertificateEnrollmentConfig {
        ca_cert_path: PathBuf::from("/etc/ssl/certs/ca.pem"),
        cert_role_mappings: vec![
            // Operations team gets full admin access
            CertRoleMapping {
                cert_field: "CN".to_string(),
                pattern: "*.ops.blixard.io".to_string(),
                roles: vec!["admin".to_string(), "operator".to_string()],
                tenant: Some("operations".to_string()),
            },
            // Development team gets developer access
            CertRoleMapping {
                cert_field: "OU".to_string(),
                pattern: "Development".to_string(),
                roles: vec!["developer".to_string(), "viewer".to_string()],
                tenant: None,
            },
            // Specific service accounts
            CertRoleMapping {
                cert_field: "CN".to_string(),
                pattern: "monitoring-service".to_string(),
                roles: vec!["monitoring".to_string(), "viewer".to_string()],
                tenant: Some("monitoring".to_string()),
            },
        ],
        default_tenant: "default".to_string(),
        allow_self_signed: false,
    };

    // Create enrollment manager
    let enrollment_manager = Arc::new(IdentityEnrollmentManager::new(
        identity_registry.clone(),
        Some(cert_config),
        Some(PathBuf::from("enrollment_state.json")),
    ));

    // Load any existing enrollment state
    enrollment_manager.load_state().await?;

    // Demo 1: Generate enrollment tokens
    println!("📝 Generating Enrollment Tokens\n");

    // Single-use operator token
    let op_token = enrollment_manager
        .generate_enrollment_token(
            "new-operator".to_string(),
            vec!["operator".to_string()],
            "operations".to_string(),
            Duration::from_days(7),
            false,
            None,
        )
        .await?;

    println!("Operator Token (single-use, 7 days):");
    println!("  ID: {}", op_token.token_id);
    println!("  Secret: {}", op_token.secret);
    println!(
        "  Command: blixard node enroll --token {} --secret {}\n",
        op_token.token_id, op_token.secret
    );

    // Multi-use worker token
    let worker_token = enrollment_manager
        .generate_enrollment_token(
            "worker-pool".to_string(),
            vec!["worker".to_string()],
            "compute".to_string(),
            Duration::from_days(30),
            true,
            Some(50), // Allow up to 50 workers
        )
        .await?;

    println!("Worker Token (multi-use, 30 days, max 50):");
    println!("  ID: {}", worker_token.token_id);
    println!("  Secret: {}", worker_token.secret);
    println!(
        "  Command: blixard node enroll --token {} --secret {}\n",
        worker_token.token_id, worker_token.secret
    );

    // Demo 2: Simulate certificate-based enrollment
    println!("🎓 Certificate-Based Enrollment Examples\n");

    // Simulate ops team member
    let ops_node = NodeId::from_bytes(&[1u8; 32]).unwrap();
    let mut ops_cert = HashMap::new();
    ops_cert.insert("CN".to_string(), "alice.ops.blixard.io".to_string());
    ops_cert.insert("OU".to_string(), "Operations".to_string());
    ops_cert.insert("O".to_string(), "Blixard Inc".to_string());

    match enrollment_manager
        .enroll_with_certificate(ops_node, ops_cert.clone())
        .await
    {
        Ok(result) => {
            println!("✅ Ops team enrollment successful:");
            println!("   User: {}", result.user_id);
            println!("   Roles: {:?}", result.roles);
            println!("   Tenant: {}\n", result.tenant_id);
        }
        Err(e) => {
            println!("❌ Ops team enrollment failed: {}\n", e);
        }
    }

    // Simulate developer
    let dev_node = NodeId::from_bytes(&[2u8; 32]).unwrap();
    let mut dev_cert = HashMap::new();
    dev_cert.insert("CN".to_string(), "bob@blixard.io".to_string());
    dev_cert.insert("OU".to_string(), "Development".to_string());
    dev_cert.insert("O".to_string(), "Blixard Inc".to_string());

    match enrollment_manager
        .enroll_with_certificate(dev_node, dev_cert.clone())
        .await
    {
        Ok(result) => {
            println!("✅ Developer enrollment successful:");
            println!("   User: {}", result.user_id);
            println!("   Roles: {:?}", result.roles);
            println!("   Tenant: {}\n", result.tenant_id);
        }
        Err(e) => {
            println!("❌ Developer enrollment failed: {}\n", e);
        }
    }

    // Demo 3: Test token enrollment
    println!("🎟️ Token-Based Enrollment Example\n");

    let test_node = NodeId::from_bytes(&[3u8; 32]).unwrap();
    match enrollment_manager
        .enroll_with_token(test_node, &worker_token.token_id, &worker_token.secret)
        .await
    {
        Ok(result) => {
            println!("✅ Worker enrollment successful:");
            println!("   User: {}", result.user_id);
            println!("   Roles: {:?}", result.roles);
            println!("   Tenant: {}\n", result.tenant_id);
        }
        Err(e) => {
            println!("❌ Worker enrollment failed: {}\n", e);
        }
    }

    // Demo 4: List active tokens
    println!("📋 Active Enrollment Tokens:\n");

    let tokens = enrollment_manager.list_tokens().await;
    for token in tokens {
        println!("Token: {}", token.token_id);
        println!("  User: {}", token.user_id);
        println!("  Roles: {:?}", token.roles);
        println!("  Multi-use: {}", token.multi_use);
        println!(
            "  Uses: {}/{}",
            token.use_count,
            token
                .max_uses
                .map(|m| m.to_string())
                .unwrap_or("unlimited".to_string())
        );
        println!();
    }

    // Demo 5: Authorization test
    println!("🔑 Testing Authorization with Enrolled Nodes\n");

    // Create middleware with our registry
    let middleware = Arc::new(IrohMiddleware::new(
        Some(security_manager),
        None,
        identity_registry.clone(),
    ));

    // Test ops node authorization
    if let Some((user, _roles, _tenant)) = identity_registry.get_user_identity(&ops_node).await {
        println!("Testing ops node ({}) permissions:", user);

        // Simulate authorization checks
        let actions = vec![
            ("createVM", "Node", "node-1"),
            ("deleteVM", "VM", "vm-123"),
            ("updateCluster", "Cluster", "prod-cluster"),
        ];

        for (action, resource_type, resource_id) in actions {
            // In real code, this would go through Cedar
            println!(
                "  {} on {}/{}: Would check Cedar policy",
                action, resource_type, resource_id
            );
        }
        println!();
    }

    // Test developer node authorization
    if let Some((user, _roles, _tenant)) = identity_registry.get_user_identity(&dev_node).await {
        println!("Testing developer node ({}) permissions:", user);

        let actions = vec![
            ("createVM", "Node", "node-1"),
            ("listVMs", "Node", "node-1"),
            ("viewLogs", "VM", "vm-123"),
        ];

        for (action, resource_type, resource_id) in actions {
            println!(
                "  {} on {}/{}: Would check Cedar policy",
                action, resource_type, resource_id
            );
        }
        println!();
    }

    // Save enrollment state
    enrollment_manager.save_state().await?;
    println!("💾 Enrollment state saved\n");

    println!("✨ Demo complete!");

    Ok(())
}

// Helper trait to add from_days to Duration
trait DurationExt {
    fn from_days(days: u64) -> Self;
}

impl DurationExt for Duration {
    fn from_days(days: u64) -> Self {
        Duration::from_secs(days * 24 * 60 * 60)
    }
}
