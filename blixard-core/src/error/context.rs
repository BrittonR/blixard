//! Enhanced error context and recovery patterns for Blixard
//!
//! This module provides type-safe error handling with enhanced context,
//! recovery patterns, and structured error data for better debugging
//! and operational visibility.

use crate::error::BlixardError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};

/// Enhanced error context with recovery suggestions and structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// The operation that was being performed when the error occurred
    pub operation: String,
    /// The component or service where the error originated
    pub component: String,
    /// Unique identifier for this error instance (for correlation)
    pub error_id: String,
    /// Timestamp when the error occurred
    pub timestamp: SystemTime,
    /// Severity level of the error
    pub severity: ErrorSeverity,
    /// Recovery suggestions for this error
    pub recovery_hints: Vec<RecoveryHint>,
    /// Structured metadata about the error
    pub metadata: HashMap<String, serde_json::Value>,
    /// The error chain leading to this error
    pub error_chain: Vec<String>,
    /// Whether this error is transient and might succeed on retry
    pub is_transient: bool,
    /// Estimated time to wait before retrying (if transient)
    pub retry_after: Option<Duration>,
}

/// Error severity levels for prioritization and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Informational - operation succeeded with warnings
    Info,
    /// Warning - operation succeeded but with degraded performance
    Warning,
    /// Error - operation failed but system is still functional
    Error,
    /// Critical - operation failed and system functionality is impacted
    Critical,
    /// Fatal - system failure requiring immediate intervention
    Fatal,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "INFO"),
            ErrorSeverity::Warning => write!(f, "WARN"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
            ErrorSeverity::Fatal => write!(f, "FATAL"),
        }
    }
}

/// Recovery suggestions for error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryHint {
    /// Type of recovery action
    pub action: RecoveryAction,
    /// Human-readable description of the recovery step
    pub description: String,
    /// Whether this action can be automated
    pub automated: bool,
    /// Priority of this recovery action (lower number = higher priority)
    pub priority: u8,
    /// Additional parameters for the recovery action
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Types of recovery actions that can be suggested
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Retry the operation with the same parameters
    Retry,
    /// Retry with exponential backoff
    RetryWithBackoff,
    /// Retry with different parameters
    RetryWithParameters,
    /// Check system configuration
    CheckConfiguration,
    /// Check resource availability
    CheckResources,
    /// Check network connectivity
    CheckConnectivity,
    /// Restart a component or service
    RestartComponent,
    /// Contact administrator or support
    ContactSupport,
    /// Scale resources up
    ScaleUp,
    /// Reduce load or scale down
    ReduceLoad,
    /// Clean up resources
    CleanupResources,
    /// Update configuration
    UpdateConfiguration,
    /// Check logs for more information
    CheckLogs,
    /// Manual intervention required
    ManualIntervention,
}

impl fmt::Display for RecoveryAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryAction::Retry => write!(f, "retry"),
            RecoveryAction::RetryWithBackoff => write!(f, "retry_with_backoff"),
            RecoveryAction::RetryWithParameters => write!(f, "retry_with_parameters"),
            RecoveryAction::CheckConfiguration => write!(f, "check_configuration"),
            RecoveryAction::CheckResources => write!(f, "check_resources"),
            RecoveryAction::CheckConnectivity => write!(f, "check_connectivity"),
            RecoveryAction::RestartComponent => write!(f, "restart_component"),
            RecoveryAction::ContactSupport => write!(f, "contact_support"),
            RecoveryAction::ScaleUp => write!(f, "scale_up"),
            RecoveryAction::ReduceLoad => write!(f, "reduce_load"),
            RecoveryAction::CleanupResources => write!(f, "cleanup_resources"),
            RecoveryAction::UpdateConfiguration => write!(f, "update_configuration"),
            RecoveryAction::CheckLogs => write!(f, "check_logs"),
            RecoveryAction::ManualIntervention => write!(f, "manual_intervention"),
        }
    }
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(operation: impl Into<String>, component: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            component: component.into(),
            error_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            severity: ErrorSeverity::Error,
            recovery_hints: Vec::new(),
            metadata: HashMap::new(),
            error_chain: Vec::new(),
            is_transient: false,
            retry_after: None,
        }
    }

    /// Set the error severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Add a recovery hint
    pub fn with_recovery_hint(mut self, hint: RecoveryHint) -> Self {
        self.recovery_hints.push(hint);
        self.recovery_hints.sort_by_key(|h| h.priority);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Mark as transient error
    pub fn as_transient(mut self, retry_after: Option<Duration>) -> Self {
        self.is_transient = true;
        self.retry_after = retry_after;
        self
    }

    /// Add to error chain
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.error_chain.push(cause.into());
        self
    }

    /// Convert to JSON for structured logging
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({
            "error": "failed to serialize error context"
        }))
    }
}

/// Extension trait for adding enhanced context to errors
pub trait ErrorContextExt<T> {
    /// Add enhanced context to the error
    fn with_context(self, context: ErrorContext) -> Result<T, EnhancedBlixardError>;
    
    /// Add context with operation and component
    fn with_operation_context(
        self,
        operation: impl Into<String>,
        component: impl Into<String>,
    ) -> Result<T, EnhancedBlixardError>;
}

impl<T, E> ErrorContextExt<T> for Result<T, E>
where
    E: Into<BlixardError>,
{
    fn with_context(self, context: ErrorContext) -> Result<T, EnhancedBlixardError> {
        self.map_err(|e| EnhancedBlixardError {
            error: e.into(),
            context: Some(context),
        })
    }

    fn with_operation_context(
        self,
        operation: impl Into<String>,
        component: impl Into<String>,
    ) -> Result<T, EnhancedBlixardError> {
        let context = ErrorContext::new(operation, component);
        self.with_context(context)
    }
}

/// Enhanced error type with rich context
#[derive(Debug)]
pub struct EnhancedBlixardError {
    pub error: BlixardError,
    pub context: Option<ErrorContext>,
}

impl fmt::Display for EnhancedBlixardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(ctx) => write!(
                f,
                "[{}] {} in {}.{}: {}",
                ctx.severity, ctx.error_id, ctx.component, ctx.operation, self.error
            ),
            None => write!(f, "{}", self.error),
        }
    }
}

impl std::error::Error for EnhancedBlixardError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<BlixardError> for EnhancedBlixardError {
    fn from(error: BlixardError) -> Self {
        Self {
            error,
            context: None,
        }
    }
}

/// Trait for domain-specific error categories
pub trait ErrorCategory {
    /// Get the error category name
    fn category(&self) -> &'static str;
    
    /// Get suggested recovery actions for this error category
    fn recovery_hints(&self) -> Vec<RecoveryHint>;
    
    /// Check if this error category is typically transient
    fn is_transient_category(&self) -> bool;
    
    /// Get the default severity for this error category
    fn default_severity(&self) -> ErrorSeverity;
}

/// VM operation specific errors with enhanced context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmError {
    /// VM not found
    NotFound { vm_name: String },
    /// VM already exists
    AlreadyExists { vm_name: String },
    /// VM is in wrong state for operation
    InvalidState { vm_name: String, current_state: String, required_state: String },
    /// Insufficient resources to create/start VM
    InsufficientResources { vm_name: String, resource_type: String, available: u64, required: u64 },
    /// VM configuration is invalid
    InvalidConfiguration { vm_name: String, field: String, reason: String },
    /// VM backend error
    BackendError { vm_name: String, backend: String, details: String },
    /// VM health check failed
    HealthCheckFailed { vm_name: String, check_name: String, reason: String },
    /// VM migration failed
    MigrationFailed { vm_name: String, source_node: u64, target_node: u64, reason: String },
}

impl ErrorCategory for VmError {
    fn category(&self) -> &'static str {
        "vm_operations"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            VmError::NotFound { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConfiguration,
                    description: "Verify VM name is correct".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            VmError::InsufficientResources { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::ScaleUp,
                    description: "Add more resources to the cluster".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
                RecoveryHint {
                    action: RecoveryAction::ReduceLoad,
                    description: "Stop some VMs to free resources".to_string(),
                    automated: false,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            VmError::InvalidState { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckLogs,
                    description: "Check VM logs for state transition issues".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            VmError::BackendError { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::RestartComponent,
                    description: "Restart the VM backend service".to_string(),
                    automated: true,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckLogs,
                    description: "Check system logs for more details".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
        }
    }

    fn is_transient_category(&self) -> bool {
        matches!(
            self,
            VmError::InsufficientResources { .. } | VmError::BackendError { .. }
        )
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            VmError::NotFound { .. } => ErrorSeverity::Warning,
            VmError::InsufficientResources { .. } => ErrorSeverity::Error,
            VmError::MigrationFailed { .. } => ErrorSeverity::Critical,
            _ => ErrorSeverity::Error,
        }
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::NotFound { vm_name } => write!(f, "VM '{}' not found", vm_name),
            VmError::AlreadyExists { vm_name } => write!(f, "VM '{}' already exists", vm_name),
            VmError::InvalidState { vm_name, current_state, required_state } => {
                write!(f, "VM '{}' is in state '{}', but '{}' is required", vm_name, current_state, required_state)
            }
            VmError::InsufficientResources { vm_name, resource_type, available, required } => {
                write!(f, "VM '{}' requires {} {} but only {} available", vm_name, required, resource_type, available)
            }
            VmError::InvalidConfiguration { vm_name, field, reason } => {
                write!(f, "VM '{}' has invalid configuration for '{}': {}", vm_name, field, reason)
            }
            VmError::BackendError { vm_name, backend, details } => {
                write!(f, "VM '{}' backend '{}' error: {}", vm_name, backend, details)
            }
            VmError::HealthCheckFailed { vm_name, check_name, reason } => {
                write!(f, "VM '{}' health check '{}' failed: {}", vm_name, check_name, reason)
            }
            VmError::MigrationFailed { vm_name, source_node, target_node, reason } => {
                write!(f, "VM '{}' migration from node {} to node {} failed: {}", vm_name, source_node, target_node, reason)
            }
        }
    }
}

/// Raft consensus specific errors with enhanced context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftError {
    /// Not the leader for this operation
    NotLeader { operation: String, current_leader: Option<u64> },
    /// Node not found in cluster
    NodeNotFound { node_id: u64 },
    /// Consensus timeout
    ConsensusTimeout { operation: String, timeout: Duration },
    /// Log inconsistency detected
    LogInconsistency { node_id: u64, expected_index: u64, actual_index: u64 },
    /// Snapshot operation failed
    SnapshotFailed { reason: String },
    /// Configuration change failed
    ConfigChangeFailed { change_type: String, reason: String },
}

impl ErrorCategory for RaftError {
    fn category(&self) -> &'static str {
        "raft_consensus"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            RaftError::NotLeader { current_leader, .. } => {
                let mut hints = vec![
                    RecoveryHint {
                        action: RecoveryAction::RetryWithParameters,
                        description: "Retry the operation on the current leader".to_string(),
                        automated: true,
                        priority: 1,
                        parameters: current_leader.map(|id| {
                            let mut params = HashMap::new();
                            params.insert("leader_id".to_string(), serde_json::json!(id));
                            params
                        }).unwrap_or_default(),
                    },
                ];
                if current_leader.is_none() {
                    hints.push(RecoveryHint {
                        action: RecoveryAction::CheckConnectivity,
                        description: "Check cluster connectivity - no leader available".to_string(),
                        automated: false,
                        priority: 2,
                        parameters: HashMap::new(),
                    });
                }
                hints
            }
            RaftError::ConsensusTimeout { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConnectivity,
                    description: "Check network connectivity between cluster nodes".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
                RecoveryHint {
                    action: RecoveryAction::RetryWithBackoff,
                    description: "Retry with exponential backoff".to_string(),
                    automated: true,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            RaftError::LogInconsistency { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::RestartComponent,
                    description: "Restart affected node to trigger log repair".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckLogs,
                    description: "Check Raft logs for detailed error information".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
        }
    }

    fn is_transient_category(&self) -> bool {
        matches!(
            self,
            RaftError::NotLeader { .. } | RaftError::ConsensusTimeout { .. }
        )
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            RaftError::NotLeader { .. } => ErrorSeverity::Warning,
            RaftError::ConsensusTimeout { .. } => ErrorSeverity::Error,
            RaftError::LogInconsistency { .. } => ErrorSeverity::Critical,
            _ => ErrorSeverity::Error,
        }
    }
}

impl fmt::Display for RaftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RaftError::NotLeader { operation, current_leader } => {
                match current_leader {
                    Some(leader) => write!(f, "Not leader for '{}' (current leader: {})", operation, leader),
                    None => write!(f, "Not leader for '{}' (no current leader)", operation),
                }
            }
            RaftError::NodeNotFound { node_id } => write!(f, "Node {} not found in cluster", node_id),
            RaftError::ConsensusTimeout { operation, timeout } => {
                write!(f, "Consensus timeout for '{}' after {:?}", operation, timeout)
            }
            RaftError::LogInconsistency { node_id, expected_index, actual_index } => {
                write!(f, "Log inconsistency on node {}: expected index {}, actual {}", node_id, expected_index, actual_index)
            }
            RaftError::SnapshotFailed { reason } => write!(f, "Snapshot operation failed: {}", reason),
            RaftError::ConfigChangeFailed { change_type, reason } => {
                write!(f, "Configuration change '{}' failed: {}", change_type, reason)
            }
        }
    }
}

/// P2P networking specific errors with enhanced context  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pError {
    /// Failed to connect to peer
    PeerConnectionFailed { peer_id: String, address: String, reason: String },
    /// Peer not found
    PeerNotFound { peer_id: String },
    /// Protocol version mismatch
    ProtocolMismatch { expected: String, actual: String },
    /// Message serialization failed
    SerializationFailed { message_type: String, reason: String },
    /// Transfer failed
    TransferFailed { resource_id: String, peer_id: String, reason: String },
    /// Network partition detected
    NetworkPartition { affected_peers: Vec<String> },
}

impl ErrorCategory for P2pError {
    fn category(&self) -> &'static str {
        "p2p_networking"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            P2pError::PeerConnectionFailed { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConnectivity,
                    description: "Check network connectivity to peer".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
                RecoveryHint {
                    action: RecoveryAction::RetryWithBackoff,
                    description: "Retry connection with exponential backoff".to_string(),
                    automated: true,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            P2pError::NetworkPartition { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConnectivity,
                    description: "Check network infrastructure for connectivity issues".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckLogs,
                    description: "Check P2P logs for detailed error information".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
        }
    }

    fn is_transient_category(&self) -> bool {
        matches!(
            self,
            P2pError::PeerConnectionFailed { .. } | P2pError::TransferFailed { .. }
        )
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            P2pError::NetworkPartition { .. } => ErrorSeverity::Critical,
            P2pError::ProtocolMismatch { .. } => ErrorSeverity::Error,
            _ => ErrorSeverity::Warning,
        }
    }
}

impl fmt::Display for P2pError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P2pError::PeerConnectionFailed { peer_id, address, reason } => {
                write!(f, "Failed to connect to peer {} at {}: {}", peer_id, address, reason)
            }
            P2pError::PeerNotFound { peer_id } => write!(f, "Peer {} not found", peer_id),
            P2pError::ProtocolMismatch { expected, actual } => {
                write!(f, "Protocol mismatch: expected {}, got {}", expected, actual)
            }
            P2pError::SerializationFailed { message_type, reason } => {
                write!(f, "Failed to serialize {} message: {}", message_type, reason)
            }
            P2pError::TransferFailed { resource_id, peer_id, reason } => {
                write!(f, "Transfer of {} to peer {} failed: {}", resource_id, peer_id, reason)
            }
            P2pError::NetworkPartition { affected_peers } => {
                write!(f, "Network partition detected affecting {} peers", affected_peers.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let context = ErrorContext::new("test_operation", "test_component")
            .with_severity(ErrorSeverity::Critical)
            .with_metadata("key".to_string(), serde_json::json!("value"))
            .as_transient(Some(Duration::from_secs(5)));

        assert_eq!(context.operation, "test_operation");
        assert_eq!(context.component, "test_component");
        assert_eq!(context.severity, ErrorSeverity::Critical);
        assert!(context.is_transient);
        assert_eq!(context.retry_after, Some(Duration::from_secs(5)));
        assert_eq!(context.metadata["key"], serde_json::json!("value"));
    }

    #[test]
    fn test_vm_error_recovery_hints() {
        let error = VmError::InsufficientResources {
            vm_name: "test-vm".to_string(),
            resource_type: "memory".to_string(),
            available: 1024,
            required: 2048,
        };

        let hints = error.recovery_hints();
        assert!(!hints.is_empty());
        assert!(hints.iter().any(|h| matches!(h.action, RecoveryAction::ScaleUp)));
    }

    #[test]
    fn test_error_severity_display() {
        assert_eq!(ErrorSeverity::Critical.to_string(), "CRITICAL");
        assert_eq!(ErrorSeverity::Error.to_string(), "ERROR");
        assert_eq!(ErrorSeverity::Warning.to_string(), "WARN");
    }

    #[test]
    fn test_enhanced_error_display() {
        let error = BlixardError::Internal {
            message: "test error".to_string(),
        };
        let context = ErrorContext::new("test_op", "test_comp")
            .with_severity(ErrorSeverity::Error);
        
        let enhanced = EnhancedBlixardError {
            error,
            context: Some(context.clone()),
        };

        let display = enhanced.to_string();
        assert!(display.contains(&context.error_id));
        assert!(display.contains("test_comp"));
        assert!(display.contains("test_op"));
    }
}
