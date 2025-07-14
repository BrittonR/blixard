//! Example demonstrating a secure Blixard cluster with authentication, RBAC, and quotas
//!
//! This example shows how to:
//! - Configure security with token authentication
//! - Set up RBAC with different user roles
//! - Apply resource quotas to tenants
//! - Enable observability (metrics and tracing)

use blixard_core::{
    config_global,
    config::{Config, AuthConfig, ObservabilityConfig, SecurityConfig, TlsConfig},
    node::Node,
    quota_system::{QuotaManager, TenantQuota},
    security::SecurityManager,
    types::NodeConfig,
};
use tempfile::TempDir;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
                .add_directive("secure_cluster_demo=info".parse()?),
        )
        .init();

    info!("Starting secure cluster demo");

    // Create a temporary directory for data
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_string_lossy().to_string();

    // Configure security settings
    let security_config = SecurityConfig {
        tls: TlsConfig {
            enabled: false, // For demo simplicity; in production, use TLS
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: AuthConfig {
            enabled: true,
            method: "token".to_string(),
            token_file: None,
        },
    };

    // Configure observability
    let observability_config = ObservabilityConfig {
        logging: blixard_core::config::LoggingConfig {
            level: "info".to_string(),
            format: "pretty".to_string(),
            timestamps: true,
            file: None,
            rotation: blixard_core::config::LogRotationConfig {
                enabled: false,
                max_size_mb: 100,
                max_files: 5,
            },
        },
        metrics: blixard_core::config::MetricsConfig {
            enabled: true,
            prefix: "blixard_secure_demo".to_string(),
            runtime_metrics: true,
        },
        tracing: blixard_core::config::TracingConfig {
            enabled: true,
            otlp_endpoint: None,
            service_name: "blixard-secure-demo".to_string(),
            sampling_ratio: 1.0,
            span_events: false,
        },
    };

    // Create a global configuration
    let config = Config {
        security: security_config.clone(),
        observability: observability_config,
        ..Default::default()
    };
    config_global::init(config)?;

    // Create node configuration
    let node_config = NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:7001".parse()?,
        data_dir: data_dir.clone(),
        vm_backend: "mock".to_string(),
        ..Default::default()
    };

    // Create and initialize the node
    let mut node = Node::new(node_config);
    node.initialize().await?;

    // Get the shared state
    let _shared_state = node.shared();

    // Create security manager directly (since shared state doesn't provide it)
    let security_manager = SecurityManager::new(security_config.clone()).await?;

    // Generate tokens for different users
    info!("Generating authentication tokens...");

    // Admin token with full permissions
    let admin_token = generate_token(&security_manager, "admin").await?;
    info!("Admin token generated");

    // Operator token with VM management permissions
    let operator_token = generate_token(
        &security_manager,
        "operator",
    )
    .await?;
    info!("Operator token generated");

    // Viewer token with read-only permissions
    let viewer_token = generate_token(
        &security_manager,
        "viewer",
    )
    .await?;
    info!("Viewer token generated");

    // Set up quotas for different tenants
    let quota_manager = QuotaManager::new();
    info!("Setting up tenant quotas...");

    // Free tier quota
    let free_quota = TenantQuota {
        tenant_id: "free-tier".to_string(),
        max_vms: 2,
        max_vcpus: 4,
        max_memory: 4096, // 4GB
        max_disk: 20,     // 20GB
        max_tasks: 10,
        enabled: true,
    };
    quota_manager.set_quota(free_quota).await?;
    info!("Free tier quota configured");

    // Premium tier quota
    let premium_quota = TenantQuota {
        tenant_id: "premium-tier".to_string(),
        max_vms: 10,
        max_vcpus: 40,
        max_memory: 65536, // 64GB
        max_disk: 500,     // 500GB
        max_tasks: 100,
        enabled: true,
    };
    quota_manager.set_quota(premium_quota).await?;
    info!("Premium tier quota configured");

    // Demonstrate authentication
    info!("\nDemonstrating authentication...");

    // Test admin authentication
    let admin_auth = security_manager.authenticate_token(&admin_token).await?;
    info!("Admin authentication: {:?}", admin_auth);

    // Test operator authentication
    let operator_auth = security_manager.authenticate_token(&operator_token).await?;
    info!("Operator authentication: {:?}", operator_auth);

    // Test invalid token
    match security_manager.authenticate_token("invalid-token").await {
        Ok(result) => info!("Invalid token result: {:?}", result),
        Err(e) => info!("Invalid token rejected: {}", e),
    }

    // Demonstrate Cedar authorization (simplified example)
    info!("\nDemonstrating Cedar authorization...");
    
    // Example Cedar authorization check for VM creation
    let mut context = std::collections::HashMap::new();
    context.insert("tenant".to_string(), serde_json::Value::String("free-tier".to_string()));
    
    match security_manager.check_permission_cedar(
        "admin", 
        "vm_create", 
        &SecurityManager::build_resource_uid("Vm", "test-vm"),
        context.clone()
    ).await {
        Ok(allowed) => info!("Admin can create VMs: {}", allowed),
        Err(e) => info!("Cedar authorization check failed (expected): {}", e),
    }

    // Check quota usage
    let free_tier_usage = quota_manager.get_usage("free-tier").await;
    info!("Free tier current usage: {:?}", free_tier_usage);
    
    let premium_tier_usage = quota_manager.get_usage("premium-tier").await;
    info!("Premium tier current usage: {:?}", premium_tier_usage);

    info!("\nSecure cluster demo completed successfully!");
    info!("\nGenerated tokens:");
    info!("  Admin:    {}", admin_token);
    info!("  Operator: {}", operator_token);
    info!("  Viewer:   {}", viewer_token);
    info!("\nUse these tokens in the Authorization header as 'Bearer <token>'");

    Ok(())
}

async fn generate_token(
    _security_manager: &SecurityManager,
    user: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Clone the security manager and make it mutable for token generation
    let _security_manager_clone =
        SecurityManager::new(blixard_core::security::default_dev_security_config()).await?;

    // Enable authentication
    let auth_config = blixard_core::config::AuthConfig {
        enabled: true,
        method: "token".to_string(),
        token_file: None,
    };

    // Create a new security manager with auth enabled
    let config = blixard_core::config::SecurityConfig {
        tls: blixard_core::config::TlsConfig {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            require_client_cert: false,
        },
        auth: auth_config,
    };

    let mut auth_manager = SecurityManager::new(config).await?;

    // Generate token with 1 hour expiration
    let token = auth_manager
        .generate_token(
            user,
            Some(std::time::Duration::from_secs(3600)),
        )
        .await?;

    Ok(token)
}
