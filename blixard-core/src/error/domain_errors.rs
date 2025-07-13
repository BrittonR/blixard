//! Domain-specific error types for Blixard components
//!
//! This module provides type-safe error enums for different system components,
//! each with specific error variants, recovery patterns, and structured context.

use thiserror::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// Error Severity and Classification
// =============================================================================

/// Error severity levels for monitoring and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Low impact, informational errors
    Info = 0,
    /// Warnings that may require attention
    Warning = 1,
    /// Errors requiring user action or investigation
    Error = 2,
    /// Critical errors requiring immediate attention
    Critical = 3,
    /// Fatal errors that prevent system operation
    Fatal = 4,
}

/// Error classification for observability and metrics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Infrastructure failures (storage, network, IO)
    Infrastructure,
    /// Application logic errors (validation, configuration)
    Application,
    /// Resource management issues (quotas, exhaustion)
    Resource,
    /// Security and authorization failures
    Security,
    /// Distributed system coordination failures
    Distributed,
    /// Temporary/transient failures
    Transient,
    /// External service failures
    External,
}

/// Error recovery strategy recommendations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry with exponential backoff
    Retry { max_attempts: u32, base_delay: Duration },
    /// Fail over to alternative resource/service
    Failover { alternatives: Vec<String> },
    /// Graceful degradation with reduced functionality
    Degrade { fallback_mode: String },
    /// Manual intervention required
    Manual { action_required: String },
    /// No recovery possible, fail fast
    Abort,
}

/// Structured error metadata for debugging and correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetadata {
    /// Machine-readable error code
    pub code: String,
    /// Error severity level
    pub severity: ErrorSeverity,
    /// Error category for classification
    pub category: ErrorCategory,
    /// Recommended recovery strategy
    pub recovery: RecoveryStrategy,
    /// Additional context data
    pub context: HashMap<String, serde_json::Value>,
    /// Error correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Component that originated the error
    pub component: String,
    /// Operation that was being performed
    pub operation: String,
}

impl ErrorMetadata {
    /// Create new error metadata
    pub fn new(
        code: impl Into<String>,
        severity: ErrorSeverity,
        category: ErrorCategory,
        recovery: RecoveryStrategy,
        component: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            category,
            recovery,
            context: HashMap::new(),
            correlation_id: None,
            component: component.into(),
            operation: operation.into(),
        }
    }

    /// Add context data
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Set correlation ID for distributed tracing
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

// =============================================================================
// VM Operations Error Types
// =============================================================================

/// Specific error types for VM operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum VmError {
    #[error("VM configuration invalid: {field} - {message}")]
    InvalidConfiguration {
        field: String,
        message: String,
        provided_value: Option<String>,
    },

    #[error("VM '{vm_name}' not found")]
    VmNotFound { vm_name: String },

    #[error("VM '{vm_name}' already exists")]
    VmAlreadyExists { vm_name: String },

    #[error("VM '{vm_name}' is in invalid state for operation '{operation}': current state is {current_state}")]
    InvalidState {
        vm_name: String,
        operation: String,
        current_state: String,
        expected_states: Vec<String>,
    },

    #[error("VM backend '{backend}' failed during '{operation}': {details}")]
    BackendFailure {
        backend: String,
        operation: String,
        details: String,
        exit_code: Option<i32>,
    },

    #[error("VM '{vm_name}' resource constraint violated: {constraint}")]
    ResourceConstraint {
        vm_name: String,
        constraint: String,
        requested: String,
        available: String,
    },

    #[error("VM '{vm_name}' process operation failed: {operation}")]
    ProcessFailure {
        vm_name: String,
        operation: String,
        pid: Option<u32>,
        signal: Option<i32>,
    },

    #[error("VM '{vm_name}' network configuration failed: {details}")]
    NetworkConfiguration {
        vm_name: String,
        interface_name: String,
        details: String,
    },

    #[error("VM '{vm_name}' storage operation failed: {operation} on {volume}")]
    StorageFailure {
        vm_name: String,
        operation: String,
        volume: String,
        filesystem_error: Option<String>,
    },
}

impl VmError {
    /// Get error metadata for this VM error
    pub fn metadata(&self) -> ErrorMetadata {
        match self {
            VmError::InvalidConfiguration { field, .. } => ErrorMetadata::new(
                "VM_INVALID_CONFIG",
                ErrorSeverity::Error,
                ErrorCategory::Application,
                RecoveryStrategy::Manual {
                    action_required: format!("Fix configuration field: {}", field),
                },
                "vm_manager",
                "validate_configuration",
            ),
            VmError::VmNotFound { vm_name } => ErrorMetadata::new(
                "VM_NOT_FOUND",
                ErrorSeverity::Warning,
                ErrorCategory::Resource,
                RecoveryStrategy::Manual {
                    action_required: format!("Create VM '{}' or check VM name", vm_name),
                },
                "vm_manager",
                "lookup_vm",
            ),
            VmError::VmAlreadyExists { .. } => ErrorMetadata::new(
                "VM_ALREADY_EXISTS",
                ErrorSeverity::Warning,
                ErrorCategory::Application,
                RecoveryStrategy::Manual {
                    action_required: "Use different VM name or delete existing VM".to_string(),
                },
                "vm_manager",
                "create_vm",
            ),
            VmError::InvalidState { operation, .. } => ErrorMetadata::new(
                "VM_INVALID_STATE",
                ErrorSeverity::Error,
                ErrorCategory::Application,
                RecoveryStrategy::Manual {
                    action_required: format!("Transition VM to valid state before performing '{}'", operation),
                },
                "vm_manager",
                operation.clone(),
            ),
            VmError::BackendFailure { operation, .. } => ErrorMetadata::new(
                "VM_BACKEND_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_secs(1),
                },
                "vm_backend",
                operation.clone(),
            ),
            VmError::ResourceConstraint { .. } => ErrorMetadata::new(
                "VM_RESOURCE_CONSTRAINT",
                ErrorSeverity::Error,
                ErrorCategory::Resource,
                RecoveryStrategy::Manual {
                    action_required: "Reduce resource requirements or add more capacity".to_string(),
                },
                "vm_scheduler",
                "allocate_resources",
            ),
            VmError::ProcessFailure { operation, .. } => ErrorMetadata::new(
                "VM_PROCESS_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay: Duration::from_secs(2),
                },
                "vm_process_manager",
                operation.clone(),
            ),
            VmError::NetworkConfiguration { .. } => ErrorMetadata::new(
                "VM_NETWORK_CONFIG",
                ErrorSeverity::Error,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Manual {
                    action_required: "Check network configuration and host networking setup".to_string(),
                },
                "vm_network",
                "configure_networking",
            ),
            VmError::StorageFailure { operation, .. } => ErrorMetadata::new(
                "VM_STORAGE_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay: Duration::from_secs(1),
                },
                "vm_storage",
                operation.clone(),
            ),
        }
    }

    /// Get actionable recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            VmError::InvalidConfiguration { field, message, .. } => vec![
                format!("Check the configuration for field '{}'", field),
                format!("Error details: {}", message),
                "Refer to VM configuration documentation".to_string(),
            ],
            VmError::VmNotFound { vm_name } => vec![
                format!("Verify VM name '{}' is correct", vm_name),
                "List available VMs with 'vm list' command".to_string(),
                "Create the VM if it doesn't exist".to_string(),
            ],
            VmError::VmAlreadyExists { vm_name } => vec![
                format!("Choose a different name for the VM (current: '{}')", vm_name),
                format!("Delete existing VM '{}' if no longer needed", vm_name),
                "Use 'vm list' to see existing VMs".to_string(),
            ],
            VmError::InvalidState { vm_name, operation, current_state, expected_states } => vec![
                format!("VM '{}' is currently in '{}' state", vm_name, current_state),
                format!("Operation '{}' requires VM to be in one of: {:?}", operation, expected_states),
                format!("Transition VM '{}' to appropriate state first", vm_name),
            ],
            VmError::BackendFailure { backend, operation, details, exit_code } => {
                let mut suggestions = vec![
                    format!("Backend '{}' failed during '{}'", backend, operation),
                    format!("Error details: {}", details),
                ];
                if let Some(code) = exit_code {
                    suggestions.push(format!("Process exited with code: {}", code));
                }
                suggestions.extend(vec![
                    "Check backend logs for more details".to_string(),
                    "Verify backend dependencies are installed".to_string(),
                    "Try the operation again after a short delay".to_string(),
                ]);
                suggestions
            }
            VmError::ResourceConstraint { requested, available, .. } => vec![
                format!("Requested: {}", requested),
                format!("Available: {}", available),
                "Reduce resource requirements in VM configuration".to_string(),
                "Add more capacity to the cluster".to_string(),
                "Wait for other VMs to release resources".to_string(),
            ],
            VmError::ProcessFailure { vm_name, operation, pid, signal } => {
                let mut suggestions = vec![
                    format!("VM '{}' process failed during '{}'", vm_name, operation),
                ];
                if let Some(p) = pid {
                    suggestions.push(format!("Process ID: {}", p));
                }
                if let Some(s) = signal {
                    suggestions.push(format!("Terminated by signal: {}", s));
                }
                suggestions.extend(vec![
                    "Check system logs for process termination details".to_string(),
                    "Verify system resources (memory, disk space)".to_string(),
                    "Try restarting the VM".to_string(),
                ]);
                suggestions
            }
            VmError::NetworkConfiguration { vm_name, interface_name, details } => vec![
                format!("Network configuration failed for VM '{}' interface '{}'", vm_name, interface_name),
                format!("Details: {}", details),
                "Check host network configuration".to_string(),
                "Verify tap interface permissions and setup".to_string(),
                "Review VM network configuration in flake.nix".to_string(),
            ],
            VmError::StorageFailure { vm_name, operation, volume, filesystem_error } => {
                let mut suggestions = vec![
                    format!("Storage operation '{}' failed for VM '{}' volume '{}'", operation, vm_name, volume),
                ];
                if let Some(fs_err) = filesystem_error {
                    suggestions.push(format!("Filesystem error: {}", fs_err));
                }
                suggestions.extend(vec![
                    "Check disk space on host system".to_string(),
                    "Verify volume permissions and ownership".to_string(),
                    "Check for filesystem corruption".to_string(),
                ]);
                suggestions
            }
        }
    }
}

// =============================================================================
// Raft Consensus Error Types
// =============================================================================

/// Specific error types for Raft consensus operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum RaftError {
    #[error("Not leader for operation '{operation}': current leader is {leader_id:?}, term {term}")]
    NotLeader {
        operation: String,
        leader_id: Option<u64>,
        term: u64,
        hint: Option<String>,
    },

    #[error("Raft proposal '{proposal_type}' rejected: {reason}")]
    ProposalRejected {
        proposal_type: String,
        reason: String,
        retry_after: Option<Duration>,
    },

    #[error("Raft configuration change failed: {operation} for node {node_id}")]
    ConfigurationChange {
        operation: String,
        node_id: u64,
        current_members: Vec<u64>,
        reason: String,
    },

    #[error("Raft consensus timeout: {operation} after {duration:?}")]
    ConsensusTimeout {
        operation: String,
        duration: Duration,
        pending_entries: u64,
    },

    #[error("Raft storage corruption detected: {details}")]
    StorageCorruption {
        details: String,
        last_valid_index: Option<u64>,
        recovery_possible: bool,
    },

    #[error("Raft snapshot operation failed: {operation}")]
    SnapshotFailure {
        operation: String,
        snapshot_index: Option<u64>,
        snapshot_term: Option<u64>,
        reason: String,
    },

    #[error("Raft network partition detected: isolated from {peer_count} peers")]
    NetworkPartition {
        peer_count: usize,
        last_contact: HashMap<u64, Duration>,
        quorum_size: usize,
    },

    #[error("Raft log compaction failed: {reason}")]
    LogCompaction {
        reason: String,
        current_size: u64,
        target_size: u64,
    },
}

impl RaftError {
    /// Get error metadata for this Raft error
    pub fn metadata(&self) -> ErrorMetadata {
        match self {
            RaftError::NotLeader { operation, .. } => ErrorMetadata::new(
                "RAFT_NOT_LEADER",
                ErrorSeverity::Warning,
                ErrorCategory::Distributed,
                RecoveryStrategy::Failover {
                    alternatives: vec!["redirect_to_leader".to_string()],
                },
                "raft_manager",
                operation.clone(),
            ),
            RaftError::ProposalRejected { .. } => ErrorMetadata::new(
                "RAFT_PROPOSAL_REJECTED",
                ErrorSeverity::Error,
                ErrorCategory::Distributed,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_millis(100),
                },
                "raft_manager",
                "propose_entry",
            ),
            RaftError::ConfigurationChange { .. } => ErrorMetadata::new(
                "RAFT_CONFIG_CHANGE",
                ErrorSeverity::Error,
                ErrorCategory::Distributed,
                RecoveryStrategy::Manual {
                    action_required: "Review cluster membership requirements".to_string(),
                },
                "raft_manager",
                "change_configuration",
            ),
            RaftError::ConsensusTimeout { .. } => ErrorMetadata::new(
                "RAFT_CONSENSUS_TIMEOUT",
                ErrorSeverity::Error,
                ErrorCategory::Distributed,
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay: Duration::from_secs(1),
                },
                "raft_manager",
                "wait_for_consensus",
            ),
            RaftError::StorageCorruption { recovery_possible, .. } => ErrorMetadata::new(
                "RAFT_STORAGE_CORRUPTION",
                ErrorSeverity::Fatal,
                ErrorCategory::Infrastructure,
                if *recovery_possible {
                    RecoveryStrategy::Manual {
                        action_required: "Restore from backup or rebuild from snapshot".to_string(),
                    }
                } else {
                    RecoveryStrategy::Abort
                },
                "raft_storage",
                "verify_integrity",
            ),
            RaftError::SnapshotFailure { .. } => ErrorMetadata::new(
                "RAFT_SNAPSHOT_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay: Duration::from_secs(5),
                },
                "raft_manager",
                "create_snapshot",
            ),
            RaftError::NetworkPartition { .. } => ErrorMetadata::new(
                "RAFT_NETWORK_PARTITION",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Manual {
                    action_required: "Check network connectivity and resolve partition".to_string(),
                },
                "raft_transport",
                "maintain_connectivity",
            ),
            RaftError::LogCompaction { .. } => ErrorMetadata::new(
                "RAFT_LOG_COMPACTION",
                ErrorSeverity::Error,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_secs(10),
                },
                "raft_manager",
                "compact_log",
            ),
        }
    }

    /// Get actionable recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            RaftError::NotLeader { operation, leader_id, hint, .. } => {
                let mut suggestions = vec![
                    format!("Operation '{}' must be performed on the leader", operation),
                ];
                if let Some(id) = leader_id {
                    suggestions.push(format!("Current leader is node {}", id));
                    suggestions.push(format!("Redirect request to node {}", id));
                } else {
                    suggestions.push("No current leader - cluster may be in election".to_string());
                    suggestions.push("Wait for leader election to complete".to_string());
                }
                if let Some(h) = hint {
                    suggestions.push(format!("Hint: {}", h));
                }
                suggestions
            }
            RaftError::ProposalRejected { proposal_type, reason, retry_after } => {
                let mut suggestions = vec![
                    format!("Proposal '{}' was rejected", proposal_type),
                    format!("Reason: {}", reason),
                ];
                if let Some(delay) = retry_after {
                    suggestions.push(format!("Retry after {:?}", delay));
                } else {
                    suggestions.push("Retry with exponential backoff".to_string());
                }
                suggestions.push("Check cluster health and connectivity".to_string());
                suggestions
            }
            RaftError::ConfigurationChange { operation, node_id, current_members, reason } => vec![
                format!("Configuration change '{}' failed for node {}", operation, node_id),
                format!("Reason: {}", reason),
                format!("Current cluster members: {:?}", current_members),
                "Ensure cluster has sufficient healthy nodes".to_string(),
                "Verify network connectivity to all members".to_string(),
            ],
            RaftError::ConsensusTimeout { operation, duration, pending_entries } => vec![
                format!("Consensus timeout for '{}' after {:?}", operation, duration),
                format!("Pending log entries: {}", pending_entries),
                "Check network latency and connectivity".to_string(),
                "Verify cluster has majority of nodes online".to_string(),
                "Consider increasing consensus timeout configuration".to_string(),
            ],
            RaftError::StorageCorruption { details, last_valid_index, recovery_possible } => {
                let mut suggestions = vec![
                    format!("Storage corruption detected: {}", details),
                ];
                if let Some(index) = last_valid_index {
                    suggestions.push(format!("Last valid log index: {}", index));
                }
                if *recovery_possible {
                    suggestions.extend(vec![
                        "Recovery may be possible from backup".to_string(),
                        "Try restoring from latest snapshot".to_string(),
                        "Rebuild node state from cluster if needed".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "Storage corruption is severe - recovery not possible".to_string(),
                        "Node must be replaced and rejoined to cluster".to_string(),
                    ]);
                }
                suggestions
            }
            RaftError::SnapshotFailure { operation, snapshot_index, snapshot_term, reason } => {
                let mut suggestions = vec![
                    format!("Snapshot operation '{}' failed", operation),
                    format!("Reason: {}", reason),
                ];
                if let (Some(index), Some(term)) = (snapshot_index, snapshot_term) {
                    suggestions.push(format!("Target snapshot: index {}, term {}", index, term));
                }
                suggestions.extend(vec![
                    "Check disk space for snapshot storage".to_string(),
                    "Verify file permissions for snapshot directory".to_string(),
                    "Try manual snapshot creation".to_string(),
                ]);
                suggestions
            }
            RaftError::NetworkPartition { peer_count, quorum_size, .. } => vec![
                format!("Network partition detected - isolated from {} peers", peer_count),
                format!("Cluster requires {} nodes for quorum", quorum_size),
                "Check network connectivity to cluster peers".to_string(),
                "Verify firewall rules and routing".to_string(),
                "Monitor for partition healing".to_string(),
            ],
            RaftError::LogCompaction { reason, current_size, target_size } => vec![
                format!("Log compaction failed: {}", reason),
                format!("Current log size: {} bytes", current_size),
                format!("Target log size: {} bytes", target_size),
                "Check disk space for compaction operation".to_string(),
                "Verify log file permissions".to_string(),
                "Try manual log compaction".to_string(),
            ],
        }
    }
}

// =============================================================================
// P2P Network Error Types
// =============================================================================

/// Specific error types for P2P networking operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum P2pError {
    #[error("Peer '{peer_id}' connection failed: {reason}")]
    ConnectionFailed {
        peer_id: String,
        address: Option<String>,
        reason: String,
        retry_count: u32,
    },

    #[error("Peer '{peer_id}' unreachable: last seen {last_seen:?} ago")]
    PeerUnreachable {
        peer_id: String,
        last_seen: Duration,
        connection_attempts: u32,
    },

    #[error("P2P message serialization failed for '{message_type}': {details}")]
    MessageSerialization {
        message_type: String,
        details: String,
        message_size: Option<usize>,
    },

    #[error("P2P transport error on '{transport}': {operation} failed")]
    TransportError {
        transport: String,
        operation: String,
        local_address: Option<String>,
        remote_address: Option<String>,
        error_details: String,
    },

    #[error("P2P discovery failed: {method} could not find peers")]
    DiscoveryFailure {
        method: String,
        searched_networks: Vec<String>,
        timeout: Duration,
    },

    #[error("P2P bandwidth limit exceeded: {current_usage}/{limit} bytes/sec")]
    BandwidthLimit {
        current_usage: u64,
        limit: u64,
        peer_id: Option<String>,
    },

    #[error("P2P protocol version mismatch with '{peer_id}': local {local_version}, remote {remote_version}")]
    ProtocolMismatch {
        peer_id: String,
        local_version: String,
        remote_version: String,
        compatible: bool,
    },

    #[error("P2P authentication failed with '{peer_id}': {reason}")]
    AuthenticationFailed {
        peer_id: String,
        method: String,
        reason: String,
        certificate_error: Option<String>,
    },
}

impl P2pError {
    /// Get error metadata for this P2P error
    pub fn metadata(&self) -> ErrorMetadata {
        match self {
            P2pError::ConnectionFailed { .. } => ErrorMetadata::new(
                "P2P_CONNECTION_FAILED",
                ErrorSeverity::Error,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 5,
                    base_delay: Duration::from_secs(1),
                },
                "p2p_transport",
                "establish_connection",
            ),
            P2pError::PeerUnreachable { .. } => ErrorMetadata::new(
                "P2P_PEER_UNREACHABLE",
                ErrorSeverity::Warning,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_secs(5),
                },
                "p2p_transport",
                "peer_communication",
            ),
            P2pError::MessageSerialization { .. } => ErrorMetadata::new(
                "P2P_MESSAGE_SERIALIZATION",
                ErrorSeverity::Error,
                ErrorCategory::Application,
                RecoveryStrategy::Manual {
                    action_required: "Check message format and protocol compatibility".to_string(),
                },
                "p2p_protocol",
                "serialize_message",
            ),
            P2pError::TransportError { .. } => ErrorMetadata::new(
                "P2P_TRANSPORT_ERROR",
                ErrorSeverity::Error,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_millis(500),
                },
                "p2p_transport",
                "transport_operation",
            ),
            P2pError::DiscoveryFailure { .. } => ErrorMetadata::new(
                "P2P_DISCOVERY_FAILURE",
                ErrorSeverity::Warning,
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    base_delay: Duration::from_secs(2),
                },
                "p2p_discovery",
                "discover_peers",
            ),
            P2pError::BandwidthLimit { .. } => ErrorMetadata::new(
                "P2P_BANDWIDTH_LIMIT",
                ErrorSeverity::Warning,
                ErrorCategory::Resource,
                RecoveryStrategy::Degrade {
                    fallback_mode: "reduced_bandwidth".to_string(),
                },
                "p2p_transport",
                "enforce_bandwidth_limits",
            ),
            P2pError::ProtocolMismatch { compatible, .. } => ErrorMetadata::new(
                "P2P_PROTOCOL_MISMATCH",
                if *compatible { ErrorSeverity::Warning } else { ErrorSeverity::Error },
                ErrorCategory::Application,
                if *compatible {
                    RecoveryStrategy::Degrade {
                        fallback_mode: "compatibility_mode".to_string(),
                    }
                } else {
                    RecoveryStrategy::Manual {
                        action_required: "Upgrade protocols to compatible versions".to_string(),
                    }
                },
                "p2p_protocol",
                "version_negotiation",
            ),
            P2pError::AuthenticationFailed { .. } => ErrorMetadata::new(
                "P2P_AUTHENTICATION_FAILED",
                ErrorSeverity::Error,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Check certificates and authentication configuration".to_string(),
                },
                "p2p_security",
                "authenticate_peer",
            ),
        }
    }

    /// Get actionable recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            P2pError::ConnectionFailed { peer_id, address, reason, retry_count } => {
                let mut suggestions = vec![
                    format!("Connection to peer '{}' failed", peer_id),
                ];
                if let Some(addr) = address {
                    suggestions.push(format!("Target address: {}", addr));
                }
                suggestions.extend(vec![
                    format!("Reason: {}", reason),
                    format!("Retry attempt: {}", retry_count),
                    "Check network connectivity to peer".to_string(),
                    "Verify peer is online and listening".to_string(),
                    "Check firewall rules for P2P ports".to_string(),
                ]);
                suggestions
            }
            P2pError::PeerUnreachable { peer_id, last_seen, connection_attempts } => vec![
                format!("Peer '{}' has been unreachable for {:?}", peer_id, last_seen),
                format!("Connection attempts made: {}", connection_attempts),
                "Check if peer is still online".to_string(),
                "Verify network routes to peer".to_string(),
                "Consider removing peer if permanently offline".to_string(),
            ],
            P2pError::MessageSerialization { message_type, details, message_size } => {
                let mut suggestions = vec![
                    format!("Message serialization failed for type '{}'", message_type),
                    format!("Details: {}", details),
                ];
                if let Some(size) = message_size {
                    suggestions.push(format!("Message size: {} bytes", size));
                    if *size > 1024 * 1024 {
                        suggestions.push("Message may be too large - consider splitting".to_string());
                    }
                }
                suggestions.extend(vec![
                    "Check message format and structure".to_string(),
                    "Verify protocol version compatibility".to_string(),
                ]);
                suggestions
            }
            P2pError::TransportError { transport, operation, local_address, remote_address, error_details } => {
                let mut suggestions = vec![
                    format!("Transport '{}' failed during '{}'", transport, operation),
                    format!("Error: {}", error_details),
                ];
                if let Some(local) = local_address {
                    suggestions.push(format!("Local address: {}", local));
                }
                if let Some(remote) = remote_address {
                    suggestions.push(format!("Remote address: {}", remote));
                }
                suggestions.extend(vec![
                    "Check network interface configuration".to_string(),
                    "Verify port availability and permissions".to_string(),
                    "Monitor network quality and latency".to_string(),
                ]);
                suggestions
            }
            P2pError::DiscoveryFailure { method, searched_networks, timeout } => vec![
                format!("Peer discovery using '{}' failed after {:?}", method, timeout),
                format!("Searched networks: {:?}", searched_networks),
                "Check network configuration for discovery".to_string(),
                "Verify discovery service is running".to_string(),
                "Try alternative discovery methods".to_string(),
                "Check for network isolation or firewall issues".to_string(),
            ],
            P2pError::BandwidthLimit { current_usage, limit, peer_id } => {
                let mut suggestions = vec![
                    format!("Bandwidth limit exceeded: {} / {} bytes/sec", current_usage, limit),
                ];
                if let Some(id) = peer_id {
                    suggestions.push(format!("Peer: {}", id));
                }
                suggestions.extend(vec![
                    "Reduce message frequency or size".to_string(),
                    "Implement message batching".to_string(),
                    "Consider increasing bandwidth limits".to_string(),
                    "Prioritize critical messages".to_string(),
                ]);
                suggestions
            }
            P2pError::ProtocolMismatch { peer_id, local_version, remote_version, compatible } => {
                let mut suggestions = vec![
                    format!("Protocol version mismatch with peer '{}'", peer_id),
                    format!("Local version: {}, Remote version: {}", local_version, remote_version),
                ];
                if *compatible {
                    suggestions.extend(vec![
                        "Versions are compatible - using fallback mode".to_string(),
                        "Consider upgrading to latest version".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "Versions are incompatible - communication not possible".to_string(),
                        "Upgrade both nodes to compatible versions".to_string(),
                        "Check protocol migration guide".to_string(),
                    ]);
                }
                suggestions
            }
            P2pError::AuthenticationFailed { peer_id, method, reason, certificate_error } => {
                let mut suggestions = vec![
                    format!("Authentication with peer '{}' failed", peer_id),
                    format!("Method: {}", method),
                    format!("Reason: {}", reason),
                ];
                if let Some(cert_err) = certificate_error {
                    suggestions.push(format!("Certificate error: {}", cert_err));
                    suggestions.extend(vec![
                        "Check certificate validity and expiration".to_string(),
                        "Verify certificate chain and CA".to_string(),
                    ]);
                }
                suggestions.extend(vec![
                    "Check authentication configuration".to_string(),
                    "Verify peer identity and credentials".to_string(),
                ]);
                suggestions
            }
        }
    }
}

// =============================================================================
// Storage Error Types
// =============================================================================

/// Specific error types for storage operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum StorageError {
    #[error("Database operation '{operation}' failed: {details}")]
    DatabaseOperation {
        operation: String,
        table: Option<String>,
        details: String,
        recoverable: bool,
    },

    #[error("Storage transaction failed: {operation} in {isolation_level}")]
    TransactionFailure {
        operation: String,
        isolation_level: String,
        conflict_type: Option<String>,
        retry_possible: bool,
    },

    #[error("Storage corruption detected in '{location}': {details}")]
    DataCorruption {
        location: String,
        details: String,
        checksum_mismatch: bool,
        backup_available: bool,
    },

    #[error("Storage quota exceeded: {used}/{limit} {unit}")]
    QuotaExceeded {
        used: u64,
        limit: u64,
        unit: String,
        resource_type: String,
    },

    #[error("Storage backend '{backend}' unavailable: {reason}")]
    BackendUnavailable {
        backend: String,
        reason: String,
        fallback_available: bool,
        estimated_recovery: Option<Duration>,
    },

    #[error("Storage consistency violation: {operation} violated constraint '{constraint}'")]
    ConsistencyViolation {
        operation: String,
        constraint: String,
        details: String,
        conflict_resolution: Option<String>,
    },

    #[error("Storage backup operation failed: {operation} for {target}")]
    BackupFailure {
        operation: String,
        target: String,
        bytes_processed: Option<u64>,
        partial_success: bool,
    },

    #[error("Storage migration error: {phase} failed during version {from_version} -> {to_version}")]
    MigrationError {
        phase: String,
        from_version: String,
        to_version: String,
        rollback_possible: bool,
    },
}

impl StorageError {
    /// Get error metadata for this storage error
    pub fn metadata(&self) -> ErrorMetadata {
        match self {
            StorageError::DatabaseOperation { recoverable, .. } => ErrorMetadata::new(
                "STORAGE_DB_OPERATION",
                ErrorSeverity::Error,
                ErrorCategory::Infrastructure,
                if *recoverable {
                    RecoveryStrategy::Retry {
                        max_attempts: 3,
                        base_delay: Duration::from_millis(100),
                    }
                } else {
                    RecoveryStrategy::Manual {
                        action_required: "Check database integrity and configuration".to_string(),
                    }
                },
                "storage_engine",
                "database_operation",
            ),
            StorageError::TransactionFailure { retry_possible, .. } => ErrorMetadata::new(
                "STORAGE_TRANSACTION_FAILURE",
                ErrorSeverity::Warning,
                ErrorCategory::Transient,
                if *retry_possible {
                    RecoveryStrategy::Retry {
                        max_attempts: 5,
                        base_delay: Duration::from_millis(50),
                    }
                } else {
                    RecoveryStrategy::Abort
                },
                "storage_engine",
                "transaction_processing",
            ),
            StorageError::DataCorruption { backup_available, .. } => ErrorMetadata::new(
                "STORAGE_DATA_CORRUPTION",
                ErrorSeverity::Fatal,
                ErrorCategory::Infrastructure,
                if *backup_available {
                    RecoveryStrategy::Manual {
                        action_required: "Restore from backup and verify data integrity".to_string(),
                    }
                } else {
                    RecoveryStrategy::Abort
                },
                "storage_engine",
                "data_verification",
            ),
            StorageError::QuotaExceeded { .. } => ErrorMetadata::new(
                "STORAGE_QUOTA_EXCEEDED",
                ErrorSeverity::Error,
                ErrorCategory::Resource,
                RecoveryStrategy::Manual {
                    action_required: "Free up space or increase quota limits".to_string(),
                },
                "storage_quotas",
                "quota_enforcement",
            ),
            StorageError::BackendUnavailable { fallback_available, .. } => ErrorMetadata::new(
                "STORAGE_BACKEND_UNAVAILABLE",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                if *fallback_available {
                    RecoveryStrategy::Failover {
                        alternatives: vec!["fallback_backend".to_string()],
                    }
                } else {
                    RecoveryStrategy::Manual {
                        action_required: "Restore backend service availability".to_string(),
                    }
                },
                "storage_backend",
                "backend_health_check",
            ),
            StorageError::ConsistencyViolation { .. } => ErrorMetadata::new(
                "STORAGE_CONSISTENCY_VIOLATION",
                ErrorSeverity::Error,
                ErrorCategory::Application,
                RecoveryStrategy::Manual {
                    action_required: "Resolve data conflicts and ensure consistency".to_string(),
                },
                "storage_engine",
                "consistency_check",
            ),
            StorageError::BackupFailure { partial_success, .. } => ErrorMetadata::new(
                "STORAGE_BACKUP_FAILURE",
                if *partial_success { ErrorSeverity::Warning } else { ErrorSeverity::Error },
                ErrorCategory::Infrastructure,
                RecoveryStrategy::Retry {
                    max_attempts: 2,
                    base_delay: Duration::from_secs(30),
                },
                "backup_manager",
                "backup_operation",
            ),
            StorageError::MigrationError { rollback_possible, .. } => ErrorMetadata::new(
                "STORAGE_MIGRATION_ERROR",
                ErrorSeverity::Critical,
                ErrorCategory::Infrastructure,
                if *rollback_possible {
                    RecoveryStrategy::Manual {
                        action_required: "Rollback migration and fix issues before retrying".to_string(),
                    }
                } else {
                    RecoveryStrategy::Manual {
                        action_required: "Manual recovery required - check migration logs".to_string(),
                    }
                },
                "storage_migration",
                "migrate_schema",
            ),
        }
    }

    /// Get actionable recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            StorageError::DatabaseOperation { operation, table, details, recoverable } => {
                let mut suggestions = vec![
                    format!("Database operation '{}' failed", operation),
                ];
                if let Some(t) = table {
                    suggestions.push(format!("Table: {}", t));
                }
                suggestions.push(format!("Details: {}", details));
                if *recoverable {
                    suggestions.extend(vec![
                        "Operation may succeed on retry".to_string(),
                        "Check for temporary resource constraints".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "Operation requires manual intervention".to_string(),
                        "Check database configuration and permissions".to_string(),
                        "Verify database schema integrity".to_string(),
                    ]);
                }
                suggestions
            }
            StorageError::TransactionFailure { operation, isolation_level, conflict_type, retry_possible } => {
                let mut suggestions = vec![
                    format!("Transaction '{}' failed at isolation level '{}'", operation, isolation_level),
                ];
                if let Some(conflict) = conflict_type {
                    suggestions.push(format!("Conflict type: {}", conflict));
                }
                if *retry_possible {
                    suggestions.extend(vec![
                        "Transaction can be retried safely".to_string(),
                        "Use exponential backoff for retries".to_string(),
                        "Consider reducing transaction scope".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "Transaction cannot be retried".to_string(),
                        "Review transaction logic and constraints".to_string(),
                    ]);
                }
                suggestions
            }
            StorageError::DataCorruption { location, details, checksum_mismatch, backup_available } => {
                let mut suggestions = vec![
                    format!("Data corruption detected in '{}'", location),
                    format!("Details: {}", details),
                ];
                if *checksum_mismatch {
                    suggestions.push("Checksum verification failed".to_string());
                }
                if *backup_available {
                    suggestions.extend(vec![
                        "Backup is available for restoration".to_string(),
                        "Restore from latest verified backup".to_string(),
                        "Verify backup integrity before restoration".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "No backup available for restoration".to_string(),
                        "Data may be permanently lost".to_string(),
                        "Consider emergency recovery procedures".to_string(),
                    ]);
                }
                suggestions
            }
            StorageError::QuotaExceeded { used, limit, unit, resource_type } => vec![
                format!("Storage quota exceeded for {}", resource_type),
                format!("Used: {} {}, Limit: {} {}", used, unit, limit, unit),
                "Free up storage space".to_string(),
                "Archive or delete old data".to_string(),
                "Request quota increase if needed".to_string(),
                "Review data retention policies".to_string(),
            ],
            StorageError::BackendUnavailable { backend, reason, fallback_available, estimated_recovery } => {
                let mut suggestions = vec![
                    format!("Storage backend '{}' is unavailable", backend),
                    format!("Reason: {}", reason),
                ];
                if *fallback_available {
                    suggestions.push("Fallback backend is available".to_string());
                    suggestions.push("Operations will continue with reduced performance".to_string());
                } else {
                    suggestions.push("No fallback backend available".to_string());
                    suggestions.push("Storage operations are blocked".to_string());
                }
                if let Some(recovery_time) = estimated_recovery {
                    suggestions.push(format!("Estimated recovery time: {:?}", recovery_time));
                }
                suggestions.extend(vec![
                    "Check backend service health".to_string(),
                    "Verify network connectivity to backend".to_string(),
                ]);
                suggestions
            }
            StorageError::ConsistencyViolation { operation, constraint, details, conflict_resolution } => {
                let mut suggestions = vec![
                    format!("Consistency violation in operation '{}'", operation),
                    format!("Violated constraint: {}", constraint),
                    format!("Details: {}", details),
                ];
                if let Some(resolution) = conflict_resolution {
                    suggestions.push(format!("Suggested resolution: {}", resolution));
                } else {
                    suggestions.extend(vec![
                        "Manual conflict resolution required".to_string(),
                        "Review data consistency requirements".to_string(),
                        "Check for concurrent modifications".to_string(),
                    ]);
                }
                suggestions
            }
            StorageError::BackupFailure { operation, target, bytes_processed, partial_success } => {
                let mut suggestions = vec![
                    format!("Backup operation '{}' failed for target '{}'", operation, target),
                ];
                if let Some(bytes) = bytes_processed {
                    suggestions.push(format!("Bytes processed: {}", bytes));
                }
                if *partial_success {
                    suggestions.extend(vec![
                        "Partial backup was successful".to_string(),
                        "Resume from last checkpoint".to_string(),
                    ]);
                } else {
                    suggestions.push("Complete backup failure".to_string());
                }
                suggestions.extend(vec![
                    "Check disk space for backup destination".to_string(),
                    "Verify backup storage permissions".to_string(),
                    "Review backup configuration".to_string(),
                ]);
                suggestions
            }
            StorageError::MigrationError { phase, from_version, to_version, rollback_possible } => {
                let mut suggestions = vec![
                    format!("Migration failed during '{}' phase", phase),
                    format!("Migrating from version {} to {}", from_version, to_version),
                ];
                if *rollback_possible {
                    suggestions.extend(vec![
                        "Rollback to previous version is possible".to_string(),
                        "Execute rollback procedure immediately".to_string(),
                        "Fix migration issues before retrying".to_string(),
                    ]);
                } else {
                    suggestions.extend(vec![
                        "Rollback is not possible".to_string(),
                        "Manual recovery required".to_string(),
                        "Contact support for migration assistance".to_string(),
                    ]);
                }
                suggestions.push("Review migration logs for detailed error information".to_string());
                suggestions
            }
        }
    }
}

// =============================================================================
// Security Error Types
// =============================================================================

/// Specific error types for security operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum SecurityError {
    #[error("Authentication failed for user '{user}' using method '{method}': {reason}")]
    AuthenticationFailed {
        user: String,
        method: String,
        reason: String,
        attempts_remaining: Option<u32>,
    },

    #[error("Authorization denied for user '{user}' on resource '{resource}': {action} not permitted")]
    AuthorizationDenied {
        user: String,
        resource: String,
        action: String,
        required_permissions: Vec<String>,
    },

    #[error("Certificate error: {operation} failed for '{subject}'")]
    CertificateError {
        operation: String,
        subject: String,
        error_type: String,
        expiry_date: Option<chrono::DateTime<chrono::Utc>>,
    },

    #[error("Token error: {operation} failed for token type '{token_type}'")]
    TokenError {
        operation: String,
        token_type: String,
        reason: String,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },

    #[error("Cryptographic operation '{operation}' failed: {details}")]
    CryptographicFailure {
        operation: String,
        algorithm: String,
        details: String,
        key_id: Option<String>,
    },

    #[error("Security policy violation: {policy} violated by {action}")]
    PolicyViolation {
        policy: String,
        action: String,
        severity: String,
        remediation: Option<String>,
    },

    #[error("Rate limit exceeded for '{resource}': {current}/{limit} per {window}")]
    RateLimitExceeded {
        resource: String,
        current: u64,
        limit: u64,
        window: Duration,
        reset_time: chrono::DateTime<chrono::Utc>,
    },

    #[error("Security audit event failed: {event_type} could not be recorded")]
    AuditFailure {
        event_type: String,
        user: Option<String>,
        resource: Option<String>,
        reason: String,
    },
}

impl SecurityError {
    /// Get error metadata for this security error
    pub fn metadata(&self) -> ErrorMetadata {
        match self {
            SecurityError::AuthenticationFailed { .. } => ErrorMetadata::new(
                "SECURITY_AUTH_FAILED",
                ErrorSeverity::Warning,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Check credentials and authentication method".to_string(),
                },
                "authentication",
                "authenticate_user",
            ),
            SecurityError::AuthorizationDenied { .. } => ErrorMetadata::new(
                "SECURITY_AUTHZ_DENIED",
                ErrorSeverity::Warning,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Request appropriate permissions or use different account".to_string(),
                },
                "authorization",
                "check_permissions",
            ),
            SecurityError::CertificateError { .. } => ErrorMetadata::new(
                "SECURITY_CERT_ERROR",
                ErrorSeverity::Error,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Renew or replace certificate".to_string(),
                },
                "certificate_manager",
                "certificate_operation",
            ),
            SecurityError::TokenError { .. } => ErrorMetadata::new(
                "SECURITY_TOKEN_ERROR",
                ErrorSeverity::Warning,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Refresh or regenerate token".to_string(),
                },
                "token_manager",
                "token_operation",
            ),
            SecurityError::CryptographicFailure { .. } => ErrorMetadata::new(
                "SECURITY_CRYPTO_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Check cryptographic configuration and key availability".to_string(),
                },
                "crypto_engine",
                "cryptographic_operation",
            ),
            SecurityError::PolicyViolation { severity, .. } => ErrorMetadata::new(
                "SECURITY_POLICY_VIOLATION",
                match severity.as_str() {
                    "high" | "critical" => ErrorSeverity::Critical,
                    "medium" => ErrorSeverity::Error,
                    _ => ErrorSeverity::Warning,
                },
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Review and comply with security policy".to_string(),
                },
                "policy_engine",
                "policy_evaluation",
            ),
            SecurityError::RateLimitExceeded { .. } => ErrorMetadata::new(
                "SECURITY_RATE_LIMIT",
                ErrorSeverity::Warning,
                ErrorCategory::Resource,
                RecoveryStrategy::Retry {
                    max_attempts: 1,
                    base_delay: Duration::from_secs(60),
                },
                "rate_limiter",
                "rate_limit_check",
            ),
            SecurityError::AuditFailure { .. } => ErrorMetadata::new(
                "SECURITY_AUDIT_FAILURE",
                ErrorSeverity::Critical,
                ErrorCategory::Security,
                RecoveryStrategy::Manual {
                    action_required: "Ensure audit system is operational".to_string(),
                },
                "audit_logger",
                "record_audit_event",
            ),
        }
    }

    /// Get actionable recovery suggestions for this error
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            SecurityError::AuthenticationFailed { user, method, reason, attempts_remaining } => {
                let mut suggestions = vec![
                    format!("Authentication failed for user '{}'", user),
                    format!("Method: {}", method),
                    format!("Reason: {}", reason),
                ];
                if let Some(attempts) = attempts_remaining {
                    if *attempts > 0 {
                        suggestions.push(format!("Attempts remaining: {}", attempts));
                    } else {
                        suggestions.push("Account may be locked due to too many failed attempts".to_string());
                    }
                }
                suggestions.extend(vec![
                    "Verify username and password".to_string(),
                    "Check if account is active and not expired".to_string(),
                    "Try alternative authentication method if available".to_string(),
                ]);
                suggestions
            }
            SecurityError::AuthorizationDenied { user, resource, action, required_permissions } => vec![
                format!("User '{}' denied access to resource '{}'", user, resource),
                format!("Attempted action: {}", action),
                format!("Required permissions: {:?}", required_permissions),
                "Request appropriate permissions from administrator".to_string(),
                "Use account with sufficient privileges".to_string(),
                "Check resource access policies".to_string(),
            ],
            SecurityError::CertificateError { operation, subject, error_type, expiry_date } => {
                let mut suggestions = vec![
                    format!("Certificate operation '{}' failed for '{}'", operation, subject),
                    format!("Error type: {}", error_type),
                ];
                if let Some(expiry) = expiry_date {
                    suggestions.push(format!("Certificate expires: {}", expiry));
                    if *expiry < chrono::Utc::now() {
                        suggestions.push("Certificate has expired".to_string());
                    } else if *expiry < chrono::Utc::now() + chrono::Duration::days(30) {
                        suggestions.push("Certificate expires soon".to_string());
                    }
                }
                suggestions.extend(vec![
                    "Renew certificate before expiration".to_string(),
                    "Verify certificate chain and CA".to_string(),
                    "Check certificate format and encoding".to_string(),
                ]);
                suggestions
            }
            SecurityError::TokenError { operation, token_type, reason, expires_at } => {
                let mut suggestions = vec![
                    format!("Token operation '{}' failed for type '{}'", operation, token_type),
                    format!("Reason: {}", reason),
                ];
                if let Some(expiry) = expires_at {
                    suggestions.push(format!("Token expires: {}", expiry));
                    if *expiry < chrono::Utc::now() {
                        suggestions.push("Token has expired".to_string());
                    }
                }
                suggestions.extend(vec![
                    "Refresh or regenerate token".to_string(),
                    "Check token format and signature".to_string(),
                    "Verify token scope and permissions".to_string(),
                ]);
                suggestions
            }
            SecurityError::CryptographicFailure { operation, algorithm, details, key_id } => {
                let mut suggestions = vec![
                    format!("Cryptographic operation '{}' failed", operation),
                    format!("Algorithm: {}", algorithm),
                    format!("Details: {}", details),
                ];
                if let Some(id) = key_id {
                    suggestions.push(format!("Key ID: {}", id));
                }
                suggestions.extend(vec![
                    "Verify cryptographic keys are available".to_string(),
                    "Check algorithm compatibility".to_string(),
                    "Review cryptographic configuration".to_string(),
                    "Ensure sufficient entropy for key generation".to_string(),
                ]);
                suggestions
            }
            SecurityError::PolicyViolation { policy, action, severity, remediation } => {
                let mut suggestions = vec![
                    format!("Security policy '{}' violated", policy),
                    format!("Action: {}", action),
                    format!("Severity: {}", severity),
                ];
                if let Some(remedy) = remediation {
                    suggestions.push(format!("Recommended remediation: {}", remedy));
                } else {
                    suggestions.extend(vec![
                        "Review security policy requirements".to_string(),
                        "Modify action to comply with policy".to_string(),
                        "Request policy exception if justified".to_string(),
                    ]);
                }
                suggestions
            }
            SecurityError::RateLimitExceeded { resource, current, limit, window, reset_time } => vec![
                format!("Rate limit exceeded for resource '{}'", resource),
                format!("Current: {}, Limit: {} per {:?}", current, limit, window),
                format!("Rate limit resets at: {}", reset_time),
                "Wait for rate limit window to reset".to_string(),
                "Reduce request frequency".to_string(),
                "Implement request batching if possible".to_string(),
                "Request rate limit increase if needed".to_string(),
            ],
            SecurityError::AuditFailure { event_type, user, resource, reason } => {
                let mut suggestions = vec![
                    format!("Audit event '{}' could not be recorded", event_type),
                    format!("Reason: {}", reason),
                ];
                if let Some(u) = user {
                    suggestions.push(format!("User: {}", u));
                }
                if let Some(r) = resource {
                    suggestions.push(format!("Resource: {}", r));
                }
                suggestions.extend(vec![
                    "Check audit system health and connectivity".to_string(),
                    "Verify audit storage capacity".to_string(),
                    "Review audit configuration".to_string(),
                    "Critical: Security events may not be recorded".to_string(),
                ]);
                suggestions
            }
        }
    }
}