use blixard_core::{
    node_shared::SharedNodeState,
    types::{VmConfig, VmStatus},
    vm_auto_recovery::RecoveryPolicy,
    vm_backend::{MockVmBackend, VmBackend},
    vm_health_monitor::VmHealthMonitor,
    vm_health_types::{
        HealthCheckType, HealthState, HealthCheck, VmHealthCheckConfig, VmHealthStatus,
    },
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::Duration;

#[tokio::test]
async fn test_vm_health_check_configuration() {
    // Create a VM configuration with health checks
    let health_config = VmHealthCheckConfig {
        check_interval_secs: 30,
        failure_threshold: 3,
        success_threshold: 2,
        initial_delay_secs: 60,
        checks: vec![
            blixard_core::vm_health_types::HealthCheck {
                name: "http_check".to_string(),
                check_type: HealthCheckType::Http {
                    url: "http://localhost:8080/health".to_string(),
                    expected_status: 200,
                    timeout_secs: 5,
                    headers: None,
                },
                weight: 1.0,
                critical: true,
                priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 3,
            },
            blixard_core::vm_health_types::HealthCheck {
                name: "tcp_check".to_string(),
                check_type: HealthCheckType::Tcp {
                    address: "localhost:22".to_string(),
                    timeout_secs: 3,
                },
                weight: 0.5,
                critical: false,
                priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 3,
            },
        ],
    };

    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory: 1024,
        health_check_config: Some(health_config.clone()),
        ..Default::default()
    };

    // Verify health check configuration
    assert!(vm_config.health_check_config.is_some());
    assert_eq!(
        vm_config.health_check_config.as_ref().unwrap().checks.len(),
        2
    );
    assert_eq!(
        vm_config
            .health_check_config
            .as_ref()
            .unwrap()
            .check_interval_secs,
        30
    );
}

#[tokio::test]
async fn test_health_check_result_scoring() {
    let mut health_status = VmHealthStatus::default();

    // Add some successful health check results
    health_status
        .check_results
        .push(blixard_core::vm_health_types::HealthCheckResult {
            check_name: "http_check".to_string(),
            success: true,
            message: "HTTP check passed".to_string(),
            duration_ms: 100,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: None,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: true,
            recovery_action: None,
        });

    health_status
        .check_results
        .push(blixard_core::vm_health_types::HealthCheckResult {
            check_name: "tcp_check".to_string(),
            success: true,
            message: "TCP check passed".to_string(),
            duration_ms: 50,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: None,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: false,
            recovery_action: None,
        });

    // Calculate score
    health_status.calculate_score();

    // Score should be 1.0 when all checks pass
    assert_eq!(health_status.score, 1.0);

    // Add a failed check
    health_status
        .check_results
        .push(blixard_core::vm_health_types::HealthCheckResult {
            check_name: "process_check".to_string(),
            success: false,
            message: "Process not found".to_string(),
            duration_ms: 10,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: Some("Process not running".to_string()),
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: true,
            recovery_action: None,
        });

    // Recalculate score
    health_status.calculate_score();

    // Score should be less than 1.0 with a failed check
    assert!(health_status.score < 1.0);
    assert!(health_status.score > 0.0);
}

#[tokio::test]
async fn test_health_state_determination() {
    let health_config = VmHealthCheckConfig {
        check_interval_secs: 30,
        failure_threshold: 3,
        success_threshold: 2,
        initial_delay_secs: 60,
        checks: vec![blixard_core::vm_health_types::HealthCheck {
            name: "critical_check".to_string(),
            check_type: HealthCheckType::Process {
                process_name: "nginx".to_string(),
                min_instances: 1,
            },
            weight: 1.0,
            critical: true,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            recovery_escalation: None,
            failure_threshold: 3,
        }],
        ..Default::default()
    };

    let mut health_status = VmHealthStatus::default();

    // Failed critical check should result in Unhealthy state
    health_status
        .check_results
        .push(blixard_core::vm_health_types::HealthCheckResult {
            check_name: "critical_check".to_string(),
            success: false,
            message: "Critical process not running".to_string(),
            duration_ms: 10,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: Some("Process not found".to_string()),
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: true,
            recovery_action: None,
        });

    health_status.update_state(&health_config);
    assert_eq!(health_status.state, HealthState::Unhealthy);

    // Reset and test successful check
    health_status.check_results.clear();
    health_status
        .check_results
        .push(blixard_core::vm_health_types::HealthCheckResult {
            check_name: "critical_check".to_string(),
            success: true,
            message: "Process running".to_string(),
            duration_ms: 10,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            error: None,
            priority: blixard_core::vm_health_types::HealthCheckPriority::Quick,
            critical: true,
            recovery_action: None,
        });

    health_status.calculate_score();
    health_status.update_state(&health_config);
    assert_eq!(health_status.state, HealthState::Healthy);
}

#[tokio::test]
async fn test_vm_health_monitor_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let config = blixard_core::types::NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:7001".parse().unwrap(),
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };

    let node_state = Arc::new(SharedNodeState::new(config));
    let database = Arc::new(redb::Database::create(temp_dir.path().join("test.db")).unwrap());
    node_state.set_database(Some(database.clone())).await;

    let vm_backend = Arc::new(MockVmBackend::new(database.clone()));
    let vm_manager = Arc::new(blixard_core::vm_backend::VmManager::new(
        database.clone(),
        vm_backend,
        node_state.clone(),
    ));

    // Create health monitor with custom recovery policy
    let recovery_policy = RecoveryPolicy {
        max_restart_attempts: 3,
        restart_delay: Duration::from_secs(10),
        enable_migration: true,
        backoff_multiplier: 2.0,
        max_backoff_delay: Duration::from_secs(300),
    };

    let mut monitor = VmHealthMonitor::with_recovery_policy(
        node_state,
        vm_manager,
        Duration::from_secs(5),
        recovery_policy,
    );

    // Start should succeed
    monitor.start();

    // Starting again should log a warning but not panic
    monitor.start();

    // Stop should succeed
    monitor.stop();

    // Stopping again should be safe
    monitor.stop();
}

#[tokio::test]
async fn test_consecutive_failure_tracking() {
    let mut health_status = VmHealthStatus::default();

    // Track consecutive failures manually
    health_status.consecutive_failures = 1;
    assert_eq!(health_status.consecutive_failures, 1);

    health_status.consecutive_failures = 2;
    assert_eq!(health_status.consecutive_failures, 2);

    // Reset on success
    health_status.consecutive_failures = 0;
    health_status.consecutive_successes = 1;
    assert_eq!(health_status.consecutive_failures, 0);
    assert_eq!(health_status.consecutive_successes, 1);
}

#[tokio::test]
async fn test_health_check_types_serialization() {
    use serde_json;

    let http_check = HealthCheckType::Http {
        url: "http://example.com/health".to_string(),
        expected_status: 200,
        timeout_secs: 10,
        headers: Some(vec![(
            "Authorization".to_string(),
            "Bearer token".to_string(),
        )]),
    };

    // Serialize
    let json = serde_json::to_string(&http_check).unwrap();

    // Deserialize
    let deserialized: HealthCheckType = serde_json::from_str(&json).unwrap();

    match deserialized {
        HealthCheckType::Http {
            url,
            expected_status,
            ..
        } => {
            assert_eq!(url, "http://example.com/health");
            assert_eq!(expected_status, 200);
        }
        _ => panic!("Unexpected health check type"),
    }
}
