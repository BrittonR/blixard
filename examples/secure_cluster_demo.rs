//! Example demonstrating a secure Blixard cluster with authentication, RBAC, and quotas
//!
//! This example shows how to:
//! - Configure security with token authentication
//! - Set up RBAC with different user roles
//! - Apply resource quotas to tenants
//! - Enable observability (metrics and tracing)

use blixard_core::{
    node::Node,
    types::NodeConfig,
    config_v2::{BlixardConfig, SecurityConfig, TlsConfig, AuthConfig, ObservabilityConfig},
    security::{SecurityManager, Permission},
    quota_system::{TenantQuota, QuotaManager},
    config_global,
};
use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("blixard=info".parse()?)
            .add_directive("secure_cluster_demo=info".parse()?))
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
        logging: blixard_core::config_v2::LoggingConfig {
            level: "info".to_string(),
            format: "pretty".to_string(),
            timestamps: true,
            file: None,
            rotation: blixard_core::config_v2::LogRotationConfig {
                enabled: false,
                max_size_mb: 100,
                max_files: 5,
            },
        },
        metrics: blixard_core::config_v2::MetricsConfig {
            enabled: true,
            prefix: "blixard_secure_demo".to_string(),
            runtime_metrics: true,
        },
        tracing: blixard_core::config_v2::TracingConfig {
            enabled: true,
            otlp_endpoint: None,
            service_name: "blixard-secure-demo".to_string(),
            sampling_ratio: 1.0,
            span_events: false,
        },
    };
    
    // Create a global configuration
    let config = BlixardConfig {
        security: security_config.clone(),
        observability: observability_config,
        ..Default::default()
    };
    config_global::set(config);
    
    // Create node configuration
    let node_config = NodeConfig {
        id: 1,
        bind_address: "127.0.0.1:7001".to_string(),
        data_dir: data_dir.clone(),
        bootstrap: true,
        transport_config: None,
    };
    
    // Create and initialize the node
    let mut node = Node::new(node_config);
    node.initialize().await?;
    
    // Get the shared state
    let shared_state = node.shared();
    
    // Get security manager and generate tokens
    let security_manager = shared_state.get_security_manager().await
        .expect("Security manager should be initialized");
    
    // Generate tokens for different users
    info!("Generating authentication tokens...");
    
    // Admin token with full permissions
    let admin_token = generate_token(&security_manager, "admin", vec![Permission::Admin]).await?;
    info!("Admin token generated");
    
    // Operator token with VM management permissions
    let operator_token = generate_token(
        &security_manager,
        "operator",
        vec![Permission::VmRead, Permission::VmWrite, Permission::ClusterRead]
    ).await?;
    info!("Operator token generated");
    
    // Viewer token with read-only permissions
    let viewer_token = generate_token(
        &security_manager,
        "viewer",
        vec![Permission::VmRead, Permission::ClusterRead, Permission::MetricsRead]
    ).await?;
    info!("Viewer token generated");
    
    // Set up quotas for different tenants
    if let Some(quota_manager) = shared_state.get_quota_manager().await {
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
    }
    
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
    
    // Demonstrate permission checks
    info!("\nDemonstrating permission checks...");
    
    // Admin can do everything
    let admin_can_write = security_manager.check_permission("admin", &Permission::VmWrite).await?;
    info!("Admin can write VMs: {}", admin_can_write);
    
    // Operator can manage VMs
    let operator_can_write = security_manager.check_permission("operator", &Permission::VmWrite).await?;
    info!("Operator can write VMs: {}", operator_can_write);
    
    // Viewer cannot write
    let viewer_can_write = security_manager.check_permission("viewer", &Permission::VmWrite).await?;
    info!("Viewer can write VMs: {}", viewer_can_write);
    
    // Check observability status
    if let Some(observability) = shared_state.get_observability_manager().await {
        info!("\nObservability status:");
        info!("  Metrics enabled: {}", observability.metrics_enabled());
        info!("  Tracing enabled: {}", observability.tracing_enabled());
    }
    
    info!("\nSecure cluster demo completed successfully!");
    info!("\nGenerated tokens:");
    info!("  Admin:    {}", admin_token);
    info!("  Operator: {}", operator_token);
    info!("  Viewer:   {}", viewer_token);
    info!("\nUse these tokens in the Authorization header as 'Bearer <token>'");
    
    Ok(())
}

async fn generate_token(
    security_manager: &SecurityManager,
    user: &str,
    permissions: Vec<Permission>,
) -> Result<String, Box<dyn std::error::Error>> {
    // Clone the security manager and make it mutable for token generation
    let mut security_manager_clone = SecurityManager::new(
        blixard_core::security::default_dev_security_config()
    ).await?;
    
    // Enable authentication
    let mut auth_config = blixard_core::config_v2::AuthConfig {
        enabled: true,
        method: "token".to_string(),
        token_file: None,
    };
    
    // Create a new security manager with auth enabled
    let config = blixard_core::config_v2::SecurityConfig {
        tls: blixard_core::config_v2::TlsConfig {
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
    let token = auth_manager.generate_token(
        user,
        permissions,
        Some(std::time::Duration::from_secs(3600))
    ).await?;
    
    Ok(token)
}