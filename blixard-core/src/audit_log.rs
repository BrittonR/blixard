//! Compliance and audit logging for security and regulatory requirements
//!
//! This module provides comprehensive audit logging for all security-relevant
//! events in the cluster, including access control, configuration changes,
//! VM lifecycle events, and administrative actions.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader};
use tokio::fs::{File, OpenOptions};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use crate::error::{BlixardError, BlixardResult};
use crate::types::{VmStatus, NodeState};
use tracing::{info, warn, error};
use sha2::{Sha256, Digest};

/// Audit event categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditCategory {
    /// Authentication and authorization events
    Access,
    /// Configuration changes
    Configuration,
    /// VM lifecycle events
    VmLifecycle,
    /// Cluster membership changes
    ClusterMembership,
    /// Resource allocation and scheduling
    ResourceManagement,
    /// Security policy changes
    SecurityPolicy,
    /// Data access and manipulation
    DataAccess,
    /// System administration
    Administration,
    /// Compliance-specific events
    Compliance,
}

/// Severity levels for audit events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AuditSeverity {
    /// Informational events
    Info,
    /// Warning events that may indicate issues
    Warning,
    /// Error events that indicate failures
    Error,
    /// Critical security events
    Critical,
}

/// Audit event entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Event category
    pub category: AuditCategory,
    /// Event severity
    pub severity: AuditSeverity,
    /// Event type (specific action)
    pub event_type: String,
    /// User or system that triggered the event
    pub actor: AuditActor,
    /// Resource affected by the event
    pub resource: Option<AuditResource>,
    /// Outcome of the event
    pub outcome: AuditOutcome,
    /// Additional context
    pub details: serde_json::Value,
    /// Correlation ID for tracking related events
    pub correlation_id: Option<String>,
    /// Source IP address (if applicable)
    pub source_ip: Option<String>,
    /// Node ID where event occurred
    pub node_id: u64,
}

/// Actor that triggered the audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditActor {
    /// Human user
    User {
        id: String,
        name: String,
        roles: Vec<String>,
    },
    /// System component
    System {
        component: String,
        node_id: u64,
    },
    /// External service
    Service {
        name: String,
        api_key_id: Option<String>,
    },
}

/// Resource affected by the audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResource {
    /// Virtual machine
    Vm {
        name: String,
        tenant_id: String,
    },
    /// Cluster node
    Node {
        id: u64,
        address: String,
    },
    /// Configuration setting
    Config {
        section: String,
        key: String,
    },
    /// Security policy
    Policy {
        name: String,
        policy_type: String,
    },
    /// Data object
    Data {
        object_type: String,
        object_id: String,
    },
}

/// Outcome of the audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    /// Action succeeded
    Success,
    /// Action failed
    Failure { reason: String },
    /// Action was denied by policy
    Denied { policy: String },
}

/// Audit logger configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Directory for audit logs
    pub log_dir: PathBuf,
    /// Maximum log file size before rotation (bytes)
    pub max_file_size: u64,
    /// Number of rotated files to keep
    pub max_files: usize,
    /// Whether to sign audit logs
    pub enable_signing: bool,
    /// Minimum severity to log
    pub min_severity: AuditSeverity,
    /// Whether to include detailed context
    pub include_details: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("/var/log/blixard/audit"),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_files: 30,
            enable_signing: true,
            min_severity: AuditSeverity::Info,
            include_details: true,
        }
    }
}

/// Audit logger for compliance and security logging
pub struct AuditLogger {
    config: AuditConfig,
    current_file: Arc<RwLock<Option<File>>>,
    current_path: Arc<RwLock<PathBuf>>,
    node_id: u64,
    signing_key: Option<Vec<u8>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub async fn new(config: AuditConfig, node_id: u64) -> BlixardResult<Self> {
        // Create audit log directory
        tokio::fs::create_dir_all(&config.log_dir).await
            .map_err(|e| BlixardError::Storage {
                operation: "create audit log directory".to_string(),
                source: Box::new(e),
            })?;
        
        // Generate signing key if needed
        let signing_key = if config.enable_signing {
            Some(Self::generate_signing_key())
        } else {
            None
        };
        
        // Open initial log file
        let log_path = Self::current_log_path(&config.log_dir);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
            .map_err(|e| BlixardError::Storage {
                operation: format!("open audit log {:?}", log_path),
                source: Box::new(e),
            })?;
        
        Ok(Self {
            config,
            current_file: Arc::new(RwLock::new(Some(file))),
            current_path: Arc::new(RwLock::new(log_path)),
            node_id,
            signing_key,
        })
    }
    
    /// Generate a signing key for audit log integrity
    fn generate_signing_key() -> Vec<u8> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key[..]);
        key
    }
    
    /// Get the current log file path
    fn current_log_path(log_dir: &Path) -> PathBuf {
        let timestamp = Utc::now().format("%Y%m%d");
        log_dir.join(format!("audit-{}.log", timestamp))
    }
    
    /// Log an audit event
    pub async fn log(&self, event: AuditEvent) -> BlixardResult<()> {
        // Check severity filter
        if event.severity < self.config.min_severity {
            return Ok(());
        }
        
        // Prepare log entry
        let mut entry = if self.config.include_details {
            event
        } else {
            // Strip detailed context for minimal logging
            AuditEvent {
                details: serde_json::Value::Null,
                ..event
            }
        };
        
        // Add node ID
        entry.node_id = self.node_id;
        
        // Serialize event
        let mut json = serde_json::to_string(&entry)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize audit event".to_string(),
                source: Box::new(e),
            })?;
        
        // Add signature if enabled
        if let Some(key) = &self.signing_key {
            let signature = Self::sign_entry(&json, key)?;
            json.push_str(&format!(" SIG:{}", signature));
        }
        
        // Write to log file
        let mut file_guard = self.current_file.write().await;
        if let Some(file) = file_guard.as_mut() {
            file.write_all(json.as_bytes()).await
                .map_err(|e| BlixardError::Storage {
                    operation: "write audit log entry".to_string(),
                    source: Box::new(e),
                })?;
            
            file.write_all(b"\n").await
                .map_err(|e| BlixardError::Storage {
                    operation: "write audit log newline".to_string(),
                    source: Box::new(e),
                })?;
            
            file.flush().await
                .map_err(|e| BlixardError::Storage {
                    operation: "flush audit log".to_string(),
                    source: Box::new(e),
                })?;
            
            // Check if rotation is needed
            let metadata = file.metadata().await
                .map_err(|e| BlixardError::Storage {
                    operation: "get audit log metadata".to_string(),
                    source: Box::new(e),
                })?;
            
            if metadata.len() >= self.config.max_file_size {
                drop(file_guard);
                self.rotate().await?;
            }
        }
        
        Ok(())
    }
    
    /// Sign an audit log entry
    fn sign_entry(entry: &str, key: &[u8]) -> BlixardResult<String> {
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<Sha256>;
        
        let mut mac = HmacSha256::new_from_slice(key)
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to create HMAC key: {}", e),
            })?;
        mac.update(entry.as_bytes());
        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }
    
    /// Rotate audit log files
    async fn rotate(&self) -> BlixardResult<()> {
        let new_path = Self::current_log_path(&self.config.log_dir);
        
        // If we're still on the same day, add a sequence number
        let mut path_guard = self.current_path.write().await;
        if new_path == *path_guard {
            let mut seq = 1;
            loop {
                let seq_path = self.config.log_dir.join(
                    format!("audit-{}-{:03}.log", Utc::now().format("%Y%m%d"), seq)
                );
                if !seq_path.exists() {
                    *path_guard = seq_path;
                    break;
                }
                seq += 1;
            }
        } else {
            *path_guard = new_path;
        }
        
        // Open new file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*path_guard)
            .await
            .map_err(|e| BlixardError::Storage {
                operation: format!("create rotated audit log {:?}", *path_guard),
                source: Box::new(e),
            })?;
        
        let mut file_guard = self.current_file.write().await;
        *file_guard = Some(file);
        
        info!("Rotated audit log to {:?}", *path_guard);
        
        // Clean up old files
        self.cleanup_old_logs().await?;
        
        Ok(())
    }
    
    /// Clean up old audit log files
    async fn cleanup_old_logs(&self) -> BlixardResult<()> {
        let mut entries = tokio::fs::read_dir(&self.config.log_dir).await
            .map_err(|e| BlixardError::Storage {
                operation: "read audit log directory".to_string(),
                source: Box::new(e),
            })?;
        
        let mut log_files = Vec::new();
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| BlixardError::Storage {
                operation: "read directory entry".to_string(),
                source: Box::new(e),
            })? {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("audit-") && name.ends_with(".log") {
                    let metadata = entry.metadata().await
                        .map_err(|e| BlixardError::Storage {
                            operation: "get file metadata".to_string(),
                            source: Box::new(e),
                        })?;
                    log_files.push((path, metadata.modified().ok()));
                }
            }
        }
        
        // Sort by modification time (oldest first)
        log_files.sort_by_key(|(_, mtime)| *mtime);
        
        // Remove oldest files if we exceed max_files
        if log_files.len() > self.config.max_files {
            let to_remove = log_files.len() - self.config.max_files;
            for (path, _) in log_files.iter().take(to_remove) {
                info!("Removing old audit log: {:?}", path);
                if let Err(e) = tokio::fs::remove_file(path).await {
                    warn!("Failed to remove old audit log {:?}: {}", path, e);
                }
            }
        }
        
        Ok(())
    }
}

// Helper functions for common audit events

/// Log a successful authentication
pub async fn log_authentication(
    logger: &AuditLogger,
    user_id: &str,
    user_name: &str,
    source_ip: Option<String>,
) -> BlixardResult<()> {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        category: AuditCategory::Access,
        severity: AuditSeverity::Info,
        event_type: "authentication.success".to_string(),
        actor: AuditActor::User {
            id: user_id.to_string(),
            name: user_name.to_string(),
            roles: Vec::new(),
        },
        resource: None,
        outcome: AuditOutcome::Success,
        details: serde_json::json!({
            "method": "token",
        }),
        correlation_id: None,
        source_ip,
        node_id: logger.node_id,
    };
    
    logger.log(event).await
}

/// Log a failed authentication attempt
pub async fn log_authentication_failure(
    logger: &AuditLogger,
    user_id: &str,
    reason: &str,
    source_ip: Option<String>,
) -> BlixardResult<()> {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        category: AuditCategory::Access,
        severity: AuditSeverity::Warning,
        event_type: "authentication.failed".to_string(),
        actor: AuditActor::User {
            id: user_id.to_string(),
            name: "unknown".to_string(),
            roles: Vec::new(),
        },
        resource: None,
        outcome: AuditOutcome::Failure {
            reason: reason.to_string(),
        },
        details: serde_json::json!({}),
        correlation_id: None,
        source_ip,
        node_id: logger.node_id,
    };
    
    logger.log(event).await
}

/// Log an authorization decision
pub async fn log_authorization(
    logger: &AuditLogger,
    user_id: &str,
    user_name: &str,
    action: &str,
    resource: AuditResource,
    allowed: bool,
    policy: Option<&str>,
) -> BlixardResult<()> {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        category: AuditCategory::Access,
        severity: if allowed { AuditSeverity::Info } else { AuditSeverity::Warning },
        event_type: format!("authorization.{}", if allowed { "allowed" } else { "denied" }),
        actor: AuditActor::User {
            id: user_id.to_string(),
            name: user_name.to_string(),
            roles: Vec::new(),
        },
        resource: Some(resource),
        outcome: if allowed {
            AuditOutcome::Success
        } else {
            AuditOutcome::Denied {
                policy: policy.unwrap_or("unknown").to_string(),
            }
        },
        details: serde_json::json!({
            "action": action,
        }),
        correlation_id: None,
        source_ip: None,
        node_id: logger.node_id,
    };
    
    logger.log(event).await
}

/// Log a VM lifecycle event
pub async fn log_vm_event(
    logger: &AuditLogger,
    vm_name: &str,
    tenant_id: &str,
    event_type: &str,
    actor: AuditActor,
    success: bool,
    details: serde_json::Value,
) -> BlixardResult<()> {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        category: AuditCategory::VmLifecycle,
        severity: if success { AuditSeverity::Info } else { AuditSeverity::Error },
        event_type: format!("vm.{}", event_type),
        actor,
        resource: Some(AuditResource::Vm {
            name: vm_name.to_string(),
            tenant_id: tenant_id.to_string(),
        }),
        outcome: if success {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure {
                reason: "operation failed".to_string(),
            }
        },
        details,
        correlation_id: None,
        source_ip: None,
        node_id: logger.node_id,
    };
    
    logger.log(event).await
}

/// Log a configuration change
pub async fn log_config_change(
    logger: &AuditLogger,
    section: &str,
    key: &str,
    old_value: Option<&str>,
    new_value: &str,
    actor: AuditActor,
) -> BlixardResult<()> {
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        category: AuditCategory::Configuration,
        severity: AuditSeverity::Warning,
        event_type: "config.changed".to_string(),
        actor,
        resource: Some(AuditResource::Config {
            section: section.to_string(),
            key: key.to_string(),
        }),
        outcome: AuditOutcome::Success,
        details: serde_json::json!({
            "old_value": old_value,
            "new_value": new_value,
        }),
        correlation_id: None,
        source_ip: None,
        node_id: logger.node_id,
    };
    
    logger.log(event).await
}

/// Audit log reader for compliance reporting
pub struct AuditLogReader {
    log_dir: PathBuf,
}

impl AuditLogReader {
    /// Create a new audit log reader
    pub fn new(log_dir: PathBuf) -> Self {
        Self { log_dir }
    }
    
    /// Query audit logs by time range
    pub async fn query_by_time(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        filters: Option<AuditFilters>,
    ) -> BlixardResult<Vec<AuditEvent>> {
        let mut events = Vec::new();
        
        // List all log files
        let mut log_files = self.list_log_files().await?;
        log_files.sort();
        
        // Read each log file
        for log_path in log_files {
            let file = File::open(&log_path).await
                .map_err(|e| BlixardError::Storage {
                    operation: format!("open audit log {:?}", log_path),
                    source: Box::new(e),
                })?;
            
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            
            while let Some(line) = lines.next_line().await
                .map_err(|e| BlixardError::Storage {
                    operation: "read audit log line".to_string(),
                    source: Box::new(e),
                })? {
                if line.trim().is_empty() {
                    continue;
                }
                
                // Remove signature if present
                let json_part = if let Some(sig_pos) = line.find(" SIG:") {
                    &line[..sig_pos]
                } else {
                    &line
                };
                
                match serde_json::from_str::<AuditEvent>(json_part) {
                    Ok(event) => {
                        if event.timestamp >= start_time && event.timestamp <= end_time {
                            if Self::matches_filters(&event, &filters) {
                                events.push(event);
                            }
                        } else if event.timestamp > end_time {
                            // Logs are chronological, so we can stop here
                            return Ok(events);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse audit log entry: {}", e);
                    }
                }
            }
        }
        
        Ok(events)
    }
    
    /// Check if an event matches the filters
    fn matches_filters(event: &AuditEvent, filters: &Option<AuditFilters>) -> bool {
        if let Some(filters) = filters {
            if let Some(category) = filters.category {
                if event.category != category {
                    return false;
                }
            }
            
            if let Some(min_severity) = filters.min_severity {
                if event.severity < min_severity {
                    return false;
                }
            }
            
            if let Some(event_type) = &filters.event_type {
                if !event.event_type.contains(event_type) {
                    return false;
                }
            }
            
            if let Some(actor_pattern) = &filters.actor_pattern {
                let actor_str = format!("{:?}", event.actor);
                if !actor_str.contains(actor_pattern) {
                    return false;
                }
            }
        }
        
        true
    }
    
    /// List all audit log files
    async fn list_log_files(&self) -> BlixardResult<Vec<PathBuf>> {
        let mut log_files = Vec::new();
        
        let mut entries = tokio::fs::read_dir(&self.log_dir).await
            .map_err(|e| BlixardError::Storage {
                operation: "read audit log directory".to_string(),
                source: Box::new(e),
            })?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| BlixardError::Storage {
                operation: "read directory entry".to_string(),
                source: Box::new(e),
            })? {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("audit-") && name.ends_with(".log") {
                    log_files.push(path);
                }
            }
        }
        
        Ok(log_files)
    }
    
    /// Generate compliance report
    pub async fn generate_compliance_report(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> BlixardResult<ComplianceReport> {
        let events = self.query_by_time(start_time, end_time, None).await?;
        
        let mut report = ComplianceReport {
            period_start: start_time,
            period_end: end_time,
            total_events: events.len(),
            events_by_category: std::collections::HashMap::new(),
            events_by_severity: std::collections::HashMap::new(),
            failed_authentications: 0,
            policy_violations: 0,
            configuration_changes: 0,
            privileged_operations: 0,
        };
        
        for event in events {
            // Count by category
            *report.events_by_category.entry(event.category).or_insert(0) += 1;
            
            // Count by severity
            *report.events_by_severity.entry(event.severity).or_insert(0) += 1;
            
            // Count specific events
            match &event.event_type[..] {
                "authentication.failed" => report.failed_authentications += 1,
                "authorization.denied" => report.policy_violations += 1,
                "config.changed" => report.configuration_changes += 1,
                _ => {}
            }
            
            // Count privileged operations
            if event.event_type.contains("admin") || event.event_type.contains("delete") {
                report.privileged_operations += 1;
            }
        }
        
        Ok(report)
    }
}

/// Filters for querying audit logs
#[derive(Debug, Clone)]
pub struct AuditFilters {
    pub category: Option<AuditCategory>,
    pub min_severity: Option<AuditSeverity>,
    pub event_type: Option<String>,
    pub actor_pattern: Option<String>,
}

/// Compliance report summary
#[derive(Debug, Serialize)]
pub struct ComplianceReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_events: usize,
    pub events_by_category: std::collections::HashMap<AuditCategory, usize>,
    pub events_by_severity: std::collections::HashMap<AuditSeverity, usize>,
    pub failed_authentications: usize,
    pub policy_violations: usize,
    pub configuration_changes: usize,
    pub privileged_operations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_audit_logger() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            log_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = AuditLogger::new(config, 1).await.unwrap();
        
        // Log some events
        log_authentication(&logger, "user123", "John Doe", Some("192.168.1.100".to_string())).await.unwrap();
        
        log_vm_event(
            &logger,
            "test-vm",
            "tenant-1",
            "created",
            AuditActor::User {
                id: "user123".to_string(),
                name: "John Doe".to_string(),
                roles: vec!["admin".to_string()],
            },
            true,
            serde_json::json!({
                "vcpus": 4,
                "memory": 8192,
            }),
        ).await.unwrap();
        
        // Read back events
        let reader = AuditLogReader::new(temp_dir.path().to_path_buf());
        let events = reader.query_by_time(
            Utc::now() - chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::hours(1),
            None,
        ).await.unwrap();
        
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "authentication.success");
        assert_eq!(events[1].event_type, "vm.created");
    }
    
    #[tokio::test]
    async fn test_audit_filters() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            log_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let logger = AuditLogger::new(config, 1).await.unwrap();
        
        // Log various events
        for i in 0..5 {
            log_authentication(&logger, &format!("user{}", i), "Test User", None).await.unwrap();
        }
        
        log_authentication_failure(&logger, "baduser", "invalid password", None).await.unwrap();
        
        // Query with filters
        let reader = AuditLogReader::new(temp_dir.path().to_path_buf());
        let filters = AuditFilters {
            category: Some(AuditCategory::Access),
            min_severity: Some(AuditSeverity::Warning),
            event_type: None,
            actor_pattern: None,
        };
        
        let events = reader.query_by_time(
            Utc::now() - chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::hours(1),
            Some(filters),
        ).await.unwrap();
        
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "authentication.failed");
    }
}