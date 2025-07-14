//! Example demonstrating VM health monitoring capabilities
//!
//! This example shows how to:
//! - Configure health checks for VMs
//! - Monitor VM health status
//! - Handle auto-recovery for failed VMs

use blixard_core::{
    node_shared::SharedNodeState,
    types::{NodeConfig, NodeTopology, VmConfig},
    vm_auto_recovery::RecoveryPolicy,
    vm_backend::{MockVmBackend, VmManager},
    vm_health_monitor::VmHealthMonitor,
    vm_health_types::{HealthCheckType, HealthCheck, HealthCheckPriority, VmHealthCheckConfig},
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::Duration;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting VM Health Monitoring Demo");

    // Set up temporary storage
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("demo.db");
    let database = Arc::new(redb::Database::create(db_path)?);

    // Create node configuration
    let node_config = NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:7001".parse()?,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: NodeTopology::default(),
    };

    // Initialize node state
    let node_state = Arc::new(SharedNodeState::new(node_config));
    node_state.set_database(Some(database.clone())).await;
    node_state.set_initialized(true);

    // Create VM backend and manager
    let vm_backend = Arc::new(MockVmBackend::new(database.clone()));
    let vm_manager = Arc::new(VmManager::new(
        database.clone(),
        vm_backend,
        node_state.clone(),
    ));

    // Create a VM with comprehensive health checks
    let health_config = VmHealthCheckConfig {
        check_interval_secs: 10, // Check every 10 seconds
        failure_threshold: 3,     // Number of consecutive failures
        success_threshold: 2,     // Number of consecutive successes
        initial_delay_secs: 5,    // Delay before starting checks
        checks: vec![
            // HTTP health endpoint check
            HealthCheck {
                name: "web_health".to_string(),
                check_type: HealthCheckType::Http {
                    url: "http://localhost:8080/health".to_string(),
                    expected_status: 200,
                    timeout_secs: 3,
                    headers: Some(vec![(
                        "User-Agent".to_string(),
                        "Blixard-HealthCheck/1.0".to_string(),
                    )]),
                },
                weight: 1.0,
                critical: true, // Web service is critical
                priority: HealthCheckPriority::Deep,
                recovery_escalation: None,
                failure_threshold: 2,
            },
            // SSH connectivity check
            HealthCheck {
                name: "ssh_port".to_string(),
                check_type: HealthCheckType::Tcp {
                    address: "localhost:22".to_string(),
                    timeout_secs: 2,
                },
                weight: 0.5,
                critical: false, // SSH is nice to have but not critical
                priority: HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 3,
            },
            // Database process check
            HealthCheck {
                name: "database_process".to_string(),
                check_type: HealthCheckType::Process {
                    process_name: "postgres".to_string(),
                    min_instances: 1,
                },
                weight: 1.0,
                critical: true, // Database is critical
                priority: HealthCheckPriority::Deep,
                recovery_escalation: None,
                failure_threshold: 2,
            },
            // Custom script check
            HealthCheck {
                name: "disk_space".to_string(),
                check_type: HealthCheckType::Script {
                    command: "df".to_string(),
                    args: vec!["-h".to_string(), "/".to_string()],
                    expected_exit_code: 0,
                    timeout_secs: 2,
                },
                weight: 0.3,
                critical: false, // Low disk space is a warning
                priority: HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 5,
            },
            // Console pattern check
            HealthCheck {
                name: "kernel_errors".to_string(),
                check_type: HealthCheckType::Console {
                    healthy_pattern: "systemd.*Started".to_string(),
                    unhealthy_pattern: Some("kernel panic|out of memory".to_string()),
                    timeout_secs: 1,
                },
                weight: 0.8,
                critical: false,
                priority: HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 3,
            },
        ],
    };

    let vm_config = VmConfig {
        name: "demo-web-server".to_string(),
        config_path: "/tmp/demo-vm.nix".to_string(),
        vcpus: 2,
        memory: 2048,
        health_check_config: Some(health_config),
        ..Default::default()
    };

    info!("Creating VM with health monitoring: {}", vm_config.name);

    // Create and start VM through node state
    node_state.create_vm(vm_config.clone()).await?;

    // Wait for VM to be created
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Start the VM
    node_state.start_vm(&vm_config.name).await?;

    info!("VM started, initializing health monitoring");

    // Configure auto-recovery policy
    let recovery_policy = RecoveryPolicy {
        max_restart_attempts: 3,
        restart_delay: Duration::from_secs(30),
        enable_migration: true,
        backoff_multiplier: 2.0,
        max_backoff_delay: Duration::from_secs(300),
    };

    // Create and start health monitor
    let mut health_monitor = VmHealthMonitor::with_recovery_policy(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_secs(5), // Check every 5 seconds
        recovery_policy,
    );

    health_monitor.start();
    info!("Health monitoring started");

    // Simulate running for a while
    info!("Monitoring VM health for 30 seconds...");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Check VM status (simplified - direct backend call)
    if let Ok(Some(status)) = vm_manager.backend().get_vm_status(&vm_config.name).await {
        info!("VM {} status: {:?}", vm_config.name, status);
    }

    // Simulate VM failure by stopping it
    info!("Simulating VM failure...");
    node_state.stop_vm(&vm_config.name).await?;

    // Wait for health monitor to detect failure and trigger recovery
    info!("Waiting for auto-recovery to kick in...");
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Check if VM was recovered
    if let Ok(Some(status)) = vm_manager.backend().get_vm_status(&vm_config.name).await {
        info!("VM {} status after recovery: {:?}", vm_config.name, status);
    }

    // Stop health monitoring
    health_monitor.stop();
    info!("Health monitoring stopped");

    // Clean up
    node_state.delete_vm(&vm_config.name).await?;
    node_state.shutdown_components().await?;

    info!("Demo completed successfully");
    Ok(())
}
