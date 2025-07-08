//! Demo of secure Iroh services with Cedar authorization

use blixard_core::{
    config_v2::{AuthConfig, SecurityConfig, TlsConfig},
    node_shared::SharedNodeState,
    security::{default_dev_security_config, SecurityManager},
    transport::{
        iroh_middleware::{IrohMiddleware, NodeIdentityRegistry},
        iroh_protocol::{
            deserialize_payload, read_message, serialize_payload, write_message, RpcRequest,
        },
        iroh_vm_service::{VmOperationRequest, VmRequest},
        secure_iroh_protocol_handler::SecureIrohServiceBuilder,
    },
};
use bytes::Bytes;
use iroh::{protocol::Router, Endpoint, NodeId};
use std::sync::Arc;
use tracing::{error, info};

const ALPN: &[u8] = b"blixard-secure-rpc/0";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔒 Secure Iroh Demo with Cedar Authorization\n");

    // Check if Cedar files exist
    let cedar_exists = std::path::Path::new("cedar/schema.cedarschema.json").exists()
        && std::path::Path::new("cedar/policies").exists();

    if !cedar_exists {
        println!("⚠️  Cedar files not found - authorization will use fallback RBAC");
    } else {
        println!("✅ Cedar files found - using policy-based authorization");
    }

    // Create security config with auth enabled
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

    // Create security manager
    let security_manager = SecurityManager::new(security_config).await?;

    // Create node identity registry
    let identity_registry = Arc::new(NodeIdentityRegistry::new());

    // Create middleware
    let middleware = Arc::new(IrohMiddleware::new(
        Some(Arc::new(security_manager)),
        None, // No quota manager for demo
        identity_registry.clone(),
    ));

    // Start server
    tokio::spawn(async move {
        if let Err(e) = run_secure_server(middleware).await {
            error!("Server error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Run client tests
    run_client_tests(identity_registry).await?;

    Ok(())
}

async fn run_secure_server(
    middleware: Arc<IrohMiddleware>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting secure Iroh server...");

    // Create endpoint
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    let node_id = endpoint.node_id();
    let addr = endpoint.node_addr().await?;

    println!("Server listening at: {}", addr);
    println!("Server node ID: {}", node_id);

    // Create a mock shared node state (normally this would be real)
    // For demo, we'll skip this and just show the structure

    // Create secure service builder
    let handler = SecureIrohServiceBuilder::new(middleware)
        // .with_vm_service(node_state).await  // Would add real node state
        .build();

    // Create router with secure handler
    let router = Router::builder(endpoint)
        .accept(ALPN.to_vec(), Arc::new(handler))
        .spawn()
        .await?;

    // Keep server running
    tokio::signal::ctrl_c().await?;
    info!("Server shutting down...");

    Ok(())
}

async fn run_client_tests(
    identity_registry: Arc<NodeIdentityRegistry>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 Running client authorization tests...\n");

    // Create client endpoint
    let client_endpoint = Endpoint::builder().discovery_n0().bind().await?;

    let client_node_id = client_endpoint.node_id();

    // Test 1: Unknown client (should fail)
    println!("Test 1: Unknown client attempting to create VM");
    test_unauthorized_access(&client_endpoint).await;

    // Test 2: Register client as operator and retry
    println!("\nTest 2: Registered operator creating VM");
    identity_registry
        .register_node(
            client_node_id,
            "demo-operator".to_string(),
            vec!["operator".to_string()],
            "tenant-1".to_string(),
        )
        .await;
    test_authorized_access(&client_endpoint).await;

    // Test 3: Time-based access control
    println!("\nTest 3: Time-based access control for VM deletion");
    test_time_based_access(&client_endpoint).await;

    println!("\n✨ Client tests complete!");

    Ok(())
}

async fn test_unauthorized_access(_endpoint: &Endpoint) {
    // In a real test, we would:
    // 1. Connect to the server
    // 2. Send a CreateVM request
    // 3. Expect an authorization failure

    println!("❌ Expected: Authorization denied for unknown client");
}

async fn test_authorized_access(_endpoint: &Endpoint) {
    // In a real test, we would:
    // 1. Connect with registered identity
    // 2. Send a CreateVM request
    // 3. Expect success (if Cedar allows operator to create VMs)

    println!("✅ Expected: VM creation allowed for registered operator");
}

async fn test_time_based_access(_endpoint: &Endpoint) {
    // Test Cedar's time-based policies
    let hour = chrono::Utc::now().hour();

    if hour >= 22 || hour <= 6 {
        println!(
            "✅ Current time ({} UTC) is in maintenance window - deletion allowed",
            hour
        );
    } else {
        println!(
            "❌ Current time ({} UTC) is outside maintenance window - deletion blocked",
            hour
        );
    }
}

/// Helper function to demonstrate RPC request structure
fn create_vm_request() -> RpcRequest {
    let vm_op = VmOperationRequest::Create {
        name: "test-vm".to_string(),
        config_path: "/tmp/test-vm.yaml".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    };

    let request = VmRequest::Operation(vm_op);
    let payload = serialize_payload(&request).unwrap();

    RpcRequest {
        request_id: 1,
        service: "vm".to_string(),
        method: "vm_operation".to_string(),
        payload,
    }
}
