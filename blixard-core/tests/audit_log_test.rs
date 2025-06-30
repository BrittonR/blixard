use blixard_core::audit_log::{
    AuditLogger, AuditConfig, AuditLogReader, AuditFilters,
    AuditCategory, AuditSeverity, AuditActor, AuditResource,
    log_authentication, log_authentication_failure, log_authorization,
    log_vm_event, log_config_change,
};
use chrono::{Utc, Duration};
use tempfile::TempDir;
use tokio::time;
use std::time::Duration as StdDuration;

#[tokio::test]
async fn test_comprehensive_audit_logging() {
    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        log_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1024 * 1024, // 1MB for testing
        max_files: 5,
        enable_signing: true,
        min_severity: AuditSeverity::Info,
        include_details: true,
    };
    
    let logger = AuditLogger::new(config, 1).await.unwrap();
    
    // Test authentication events
    log_authentication(&logger, "alice", "Alice Smith", Some("10.0.0.100".to_string())).await.unwrap();
    log_authentication_failure(&logger, "bob", "invalid credentials", Some("10.0.0.200".to_string())).await.unwrap();
    
    // Test authorization events
    log_authorization(
        &logger,
        "alice",
        "Alice Smith",
        "vm.create",
        AuditResource::Vm {
            name: "test-vm".to_string(),
            tenant_id: "tenant-1".to_string(),
        },
        true,
        None,
    ).await.unwrap();
    
    log_authorization(
        &logger,
        "bob",
        "Bob Jones",
        "vm.delete",
        AuditResource::Vm {
            name: "prod-vm".to_string(),
            tenant_id: "tenant-2".to_string(),
        },
        false,
        Some("insufficient-privileges"),
    ).await.unwrap();
    
    // Test VM lifecycle events
    log_vm_event(
        &logger,
        "web-server-1",
        "tenant-1",
        "created",
        AuditActor::User {
            id: "alice".to_string(),
            name: "Alice Smith".to_string(),
            roles: vec!["developer".to_string()],
        },
        true,
        serde_json::json!({
            "vcpus": 4,
            "memory": 8192,
            "node_id": 2,
        }),
    ).await.unwrap();
    
    log_vm_event(
        &logger,
        "web-server-1",
        "tenant-1",
        "started",
        AuditActor::System {
            component: "vm-manager".to_string(),
            node_id: 2,
        },
        true,
        serde_json::json!({
            "boot_time_ms": 2500,
        }),
    ).await.unwrap();
    
    // Test configuration changes
    log_config_change(
        &logger,
        "vm.scheduler",
        "overcommit_ratio",
        Some("1.5"),
        "2.0",
        AuditActor::User {
            id: "admin".to_string(),
            name: "System Admin".to_string(),
            roles: vec!["admin".to_string()],
        },
    ).await.unwrap();
    
    // Wait a bit to ensure all logs are written
    time::sleep(StdDuration::from_millis(100)).await;
    
    // Read back and verify
    let reader = AuditLogReader::new(temp_dir.path().to_path_buf());
    let events = reader.query_by_time(
        Utc::now() - Duration::hours(1),
        Utc::now() + Duration::hours(1),
        None,
    ).await.unwrap();
    
    assert_eq!(events.len(), 7);
    
    // Verify event types
    let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_types.contains(&"authentication.success"));
    assert!(event_types.contains(&"authentication.failed"));
    assert!(event_types.contains(&"authorization.allowed"));
    assert!(event_types.contains(&"authorization.denied"));
    assert!(event_types.contains(&"vm.created"));
    assert!(event_types.contains(&"vm.started"));
    assert!(event_types.contains(&"config.changed"));
}

#[tokio::test]
async fn test_audit_log_filtering() {
    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        log_dir: temp_dir.path().to_path_buf(),
        min_severity: AuditSeverity::Info,
        ..Default::default()
    };
    
    let logger = AuditLogger::new(config, 1).await.unwrap();
    
    // Log events of different categories and severities
    for i in 0..10 {
        log_authentication(&logger, &format!("user{}", i), "Test User", None).await.unwrap();
    }
    
    for i in 0..5 {
        log_authentication_failure(&logger, &format!("baduser{}", i), "wrong password", None).await.unwrap();
    }
    
    for i in 0..3 {
        log_vm_event(
            &logger,
            &format!("vm{}", i),
            "tenant-1",
            "created",
            AuditActor::System {
                component: "scheduler".to_string(),
                node_id: 1,
            },
            true,
            serde_json::json!({}),
        ).await.unwrap();
    }
    
    // Test category filter
    let reader = AuditLogReader::new(temp_dir.path().to_path_buf());
    let filters = AuditFilters {
        category: Some(AuditCategory::Access),
        min_severity: None,
        event_type: None,
        actor_pattern: None,
    };
    
    let events = reader.query_by_time(
        Utc::now() - Duration::hours(1),
        Utc::now() + Duration::hours(1),
        Some(filters),
    ).await.unwrap();
    
    assert_eq!(events.len(), 15); // 10 success + 5 failures
    
    // Test severity filter
    let filters = AuditFilters {
        category: None,
        min_severity: Some(AuditSeverity::Warning),
        event_type: None,
        actor_pattern: None,
    };
    
    let events = reader.query_by_time(
        Utc::now() - Duration::hours(1),
        Utc::now() + Duration::hours(1),
        Some(filters),
    ).await.unwrap();
    
    assert_eq!(events.len(), 5); // Only authentication failures
    
    // Test event type filter
    let filters = AuditFilters {
        category: None,
        min_severity: None,
        event_type: Some("vm.".to_string()),
        actor_pattern: None,
    };
    
    let events = reader.query_by_time(
        Utc::now() - Duration::hours(1),
        Utc::now() + Duration::hours(1),
        Some(filters),
    ).await.unwrap();
    
    assert_eq!(events.len(), 3); // VM events
}

#[tokio::test]
async fn test_compliance_report_generation() {
    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        log_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    
    let logger = AuditLogger::new(config, 1).await.unwrap();
    
    // Generate diverse audit events
    // Authentication events
    for _ in 0..20 {
        log_authentication(&logger, "user1", "User One", None).await.unwrap();
    }
    for _ in 0..5 {
        log_authentication_failure(&logger, "attacker", "brute force", Some("suspicious.ip".to_string())).await.unwrap();
    }
    
    // Authorization events
    for _ in 0..10 {
        log_authorization(
            &logger,
            "user2",
            "User Two",
            "vm.create",
            AuditResource::Node { id: 1, address: "10.0.0.1:7001".to_string() },
            false,
            Some("quota-exceeded"),
        ).await.unwrap();
    }
    
    // Configuration changes
    for i in 0..3 {
        log_config_change(
            &logger,
            "security",
            &format!("setting{}", i),
            Some("old"),
            "new",
            AuditActor::User {
                id: "admin".to_string(),
                name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
            },
        ).await.unwrap();
    }
    
    // VM events (some privileged)
    for _ in 0..5 {
        log_vm_event(
            &logger,
            "vm1",
            "tenant1",
            "deleted", // Privileged operation
            AuditActor::User {
                id: "admin".to_string(),
                name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
            },
            true,
            serde_json::json!({}),
        ).await.unwrap();
    }
    
    // Generate compliance report
    let reader = AuditLogReader::new(temp_dir.path().to_path_buf());
    let report = reader.generate_compliance_report(
        Utc::now() - Duration::hours(1),
        Utc::now() + Duration::hours(1),
    ).await.unwrap();
    
    // Verify report contents
    assert_eq!(report.total_events, 43);
    assert_eq!(report.failed_authentications, 5);
    assert_eq!(report.policy_violations, 10);
    assert_eq!(report.configuration_changes, 3);
    assert_eq!(report.privileged_operations, 5); // VM deletions
    
    // Check category counts
    assert_eq!(report.events_by_category.get(&AuditCategory::Access).copied().unwrap_or(0), 35);
    assert_eq!(report.events_by_category.get(&AuditCategory::Configuration).copied().unwrap_or(0), 3);
    assert_eq!(report.events_by_category.get(&AuditCategory::VmLifecycle).copied().unwrap_or(0), 5);
    
    // Check severity counts
    assert_eq!(report.events_by_severity.get(&AuditSeverity::Info).copied().unwrap_or(0), 28);
    assert_eq!(report.events_by_severity.get(&AuditSeverity::Warning).copied().unwrap_or(0), 15);
}

#[tokio::test]
async fn test_audit_log_rotation() {
    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        log_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1000, // Very small for testing rotation
        max_files: 3,
        enable_signing: false,
        ..Default::default()
    };
    
    let logger = AuditLogger::new(config, 1).await.unwrap();
    
    // Generate enough events to trigger rotation
    for i in 0..50 {
        log_authentication(
            &logger,
            &format!("user{}", i),
            &format!("User {}", i),
            Some(format!("10.0.0.{}", i % 256)),
        ).await.unwrap();
    }
    
    // Check that multiple log files were created
    let mut count = 0;
    let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("log") {
            count += 1;
        }
    }
    
    assert!(count > 1, "Should have created multiple log files due to rotation");
    assert!(count <= 3, "Should not exceed max_files limit");
}

#[tokio::test]
async fn test_audit_log_signatures() {
    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        log_dir: temp_dir.path().to_path_buf(),
        enable_signing: true,
        ..Default::default()
    };
    
    let logger = AuditLogger::new(config, 1).await.unwrap();
    
    // Log an event
    log_authentication(&logger, "alice", "Alice", None).await.unwrap();
    
    // Read the raw log file
    let log_path = temp_dir.path().join(format!("audit-{}.log", Utc::now().format("%Y%m%d")));
    let content = tokio::fs::read_to_string(&log_path).await.unwrap();
    
    // Verify signature is present
    assert!(content.contains(" SIG:"), "Log entry should contain signature");
    
    // Verify the signature format (should be hex)
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);
    
    let parts: Vec<&str> = lines[0].split(" SIG:").collect();
    assert_eq!(parts.len(), 2);
    
    let signature = parts[1];
    assert!(signature.chars().all(|c| c.is_ascii_hexdigit()), "Signature should be hex encoded");
    assert_eq!(signature.len(), 64, "HMAC-SHA256 signature should be 64 hex characters");
}