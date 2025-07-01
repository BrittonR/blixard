//! Example demonstrating VM health monitoring capabilities
//! 
//! This example shows how to:
//! - Configure health checks for VMs
//! - Monitor VM health status
//! - Handle auto-recovery for failed VMs

use blixard_core::{
    types::{VmConfig, NodeConfig},
    vm_health_types::{VmHealthCheckConfig, VmHealthCheck, HealthCheckType},
    node_shared::SharedNodeState,
    vm_backend::{VmManager, MockVmBackend},
    vm_health_monitor::VmHealthMonitor,
    vm_auto_recovery::RecoveryPolicy,
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::Duration;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();
    
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
    };
    
    // Initialize node state
    let node_state = Arc::new(SharedNodeState::new(node_config));
    node_state.set_database(database.clone()).await;
    node_state.initialize().await?;
    
    // Create VM backend and manager
    let vm_backend = Arc::new(MockVmBackend::new(database.clone()));
    let vm_manager = Arc::new(VmManager::new(
        database.clone(),
        vm_backend,
        node_state.clone()
    ));
    
    // Create a VM with comprehensive health checks
    let health_config = VmHealthCheckConfig {
        enabled: true,
        interval_seconds: 10,  // Check every 10 seconds
        timeout_seconds: 5,    // Overall timeout for all checks
        checks: vec![
            // HTTP health endpoint check
            VmHealthCheck {
                name: "web_health".to_string(),
                check_type: HealthCheckType::Http {
                    url: "http://localhost:8080/health".to_string(),
                    expected_status: 200,
                    timeout_secs: 3,
                    headers: Some(vec![
                        ("User-Agent".to_string(), "Blixard-HealthCheck/1.0".to_string()),
                    ]),
                },
                weight: 1.0,
                critical: true,  // Web service is critical
            },
            // SSH connectivity check
            VmHealthCheck {
                name: "ssh_port".to_string(),
                check_type: HealthCheckType::Tcp {
                    address: "localhost:22".to_string(),
                    timeout_secs: 2,
                },
                weight: 0.5,
                critical: false,  // SSH is nice to have but not critical
            },
            // Database process check
            VmHealthCheck {
                name: "database_process".to_string(),
                check_type: HealthCheckType::Process {
                    process_name: "postgres".to_string(),
                    min_instances: 1,
                },
                weight: 1.0,
                critical: true,  // Database is critical
            },
            // Custom script check
            VmHealthCheck {
                name: "disk_space".to_string(),
                check_type: HealthCheckType::Script {
                    command: "df".to_string(),
                    args: vec!["-h".to_string(), "/".to_string()],
                    expected_exit_code: 0,
                    timeout_secs: 2,
                },
                weight: 0.3,
                critical: false,  // Low disk space is a warning
            },
            // Console pattern check
            VmHealthCheck {
                name: "kernel_errors".to_string(),
                check_type: HealthCheckType::Console {
                    healthy_pattern: "systemd.*Started".to_string(),
                    unhealthy_pattern: Some("kernel panic|out of memory".to_string()),
                    timeout_secs: 1,
                },
                weight: 0.8,
                critical: false,
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
    
    // Create VM through Raft consensus
    node_state.propose_vm_creation(vm_config.clone(), 1).await?;
    
    // Wait for VM to be created
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Start the VM
    node_state.propose_vm_start(vm_config.name.clone()).await?;
    
    info!("VM started, initializing health monitoring");
    
    // Configure auto-recovery policy
    let recovery_policy = RecoveryPolicy {
        enabled: true,
        max_retries: 3,
        retry_delay: Duration::from_secs(30),
        failure_threshold: 2,  // Trigger recovery after 2 consecutive failures
    };
    
    // Create and start health monitor
    let mut health_monitor = VmHealthMonitor::with_recovery_policy(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_secs(5),  // Check every 5 seconds
        recovery_policy,
    );
    
    health_monitor.start();
    info!("Health monitoring started");
    
    // Simulate running for a while
    info!("Monitoring VM health for 30 seconds...");
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Check VM health status
    if let Ok(Some((config, status))) = vm_manager.get_vm_status(&vm_config.name).await {
        info!("VM {} status: {:?}", config.name, status);
    }
    
    // Simulate VM failure by stopping it
    info!("Simulating VM failure...");
    node_state.propose_vm_stop(vm_config.name.clone()).await?;
    
    // Wait for health monitor to detect failure and trigger recovery
    info!("Waiting for auto-recovery to kick in...");
    tokio::time::sleep(Duration::from_secs(15)).await;
    
    // Check if VM was recovered
    if let Ok(Some((config, status))) = vm_manager.get_vm_status(&vm_config.name).await {
        info!("VM {} status after recovery: {:?}", config.name, status);
    }
    
    // Stop health monitoring
    health_monitor.stop();
    info!("Health monitoring stopped");
    
    // Clean up
    node_state.propose_vm_deletion(vm_config.name.clone()).await?;
    node_state.stop().await?;
    
    info!("Demo completed successfully");
    Ok(())
}