//! Domain-specific error types with structured data and recovery patterns
//!
//! This module provides specialized error types for different domains within Blixard,
//! each with appropriate recovery strategies and structured error information.

use crate::error::context::{ErrorCategory, ErrorSeverity, RecoveryAction, RecoveryHint};
use crate::error::BlixardError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// Resource management errors with detailed context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceError {
    /// Resource quota exceeded
    QuotaExceeded {
        resource_type: String,
        tenant_id: String,
        limit: u64,
        requested: u64,
        current_usage: u64,
    },
    /// Resource not available
    Unavailable {
        resource_type: String,
        resource_id: String,
        reason: String,
        estimated_availability: Option<Duration>,
    },
    /// Resource exhausted on node
    Exhausted {
        resource_type: String,
        node_id: u64,
        available: u64,
        required: u64,
        alternatives: Vec<u64>, // Alternative nodes with capacity
    },
    /// Resource allocation failed
    AllocationFailed {
        resource_type: String,
        amount: u64,
        node_id: u64,
        reason: String,
    },
    /// Resource leak detected
    LeakDetected {
        resource_type: String,
        leaked_amount: u64,
        allocation_trace: Vec<String>,
    },
}

impl ErrorCategory for ResourceError {
    fn category(&self) -> &'static str {
        "resource_management"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            ResourceError::QuotaExceeded { tenant_id, resource_type, .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckResources,
                    description: format!("Review resource usage for tenant {}", tenant_id),
                    automated: false,
                    priority: 1,
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("tenant_id".to_string(), serde_json::json!(tenant_id));
                        params.insert("resource_type".to_string(), serde_json::json!(resource_type));
                        params
                    },
                },
                RecoveryHint {
                    action: RecoveryAction::ScaleUp,
                    description: "Increase resource quota or add capacity".to_string(),
                    automated: false,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            ResourceError::Exhausted { alternatives, node_id, .. } => {
                let mut hints = vec![
                    RecoveryHint {
                        action: RecoveryAction::ScaleUp,
                        description: "Add more capacity to the cluster".to_string(),
                        automated: false,
                        priority: 2,
                        parameters: HashMap::new(),
                    },
                ];
                if !alternatives.is_empty() {
                    hints.insert(0, RecoveryHint {
                        action: RecoveryAction::RetryWithParameters,
                        description: "Try allocating on alternative nodes".to_string(),
                        automated: true,
                        priority: 1,
                        parameters: {
                            let mut params = HashMap::new();
                            params.insert("alternative_nodes".to_string(), serde_json::json!(alternatives));
                            params.insert("exclude_node".to_string(), serde_json::json!(node_id));
                            params
                        },
                    });
                }
                hints
            }
            ResourceError::LeakDetected { allocation_trace, .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CleanupResources,
                    description: "Clean up leaked resources".to_string(),
                    automated: true,
                    priority: 1,
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("allocation_trace".to_string(), serde_json::json!(allocation_trace));
                        params
                    },
                },
                RecoveryHint {
                    action: RecoveryAction::CheckLogs,
                    description: "Investigate allocation trace for leak source".to_string(),
                    automated: false,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckResources,
                    description: "Check resource availability and usage".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
        }
    }

    fn is_transient_category(&self) -> bool {
        matches!(self, ResourceError::Unavailable { .. } | ResourceError::Exhausted { .. })
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            ResourceError::LeakDetected { .. } => ErrorSeverity::Critical,
            ResourceError::QuotaExceeded { .. } => ErrorSeverity::Error,
            ResourceError::Exhausted { .. } => ErrorSeverity::Warning,
            _ => ErrorSeverity::Error,
        }
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceError::QuotaExceeded { resource_type, tenant_id, limit, requested, current_usage } => {
                write!(f, "Quota exceeded for {} (tenant {}): requested {}, limit {}, current usage {}", 
                       resource_type, tenant_id, requested, limit, current_usage)
            }
            ResourceError::Unavailable { resource_type, resource_id, reason, estimated_availability } => {
                match estimated_availability {
                    Some(eta) => write!(f, "Resource {} '{}' unavailable: {} (estimated available in {:?})", 
                                       resource_type, resource_id, reason, eta),
                    None => write!(f, "Resource {} '{}' unavailable: {}", resource_type, resource_id, reason),
                }
            }
            ResourceError::Exhausted { resource_type, node_id, available, required, alternatives } => {
                write!(f, "Resource {} exhausted on node {}: {} available, {} required ({} alternatives)", 
                       resource_type, node_id, available, required, alternatives.len())
            }
            ResourceError::AllocationFailed { resource_type, amount, node_id, reason } => {
                write!(f, "Failed to allocate {} {} on node {}: {}", amount, resource_type, node_id, reason)
            }
            ResourceError::LeakDetected { resource_type, leaked_amount, allocation_trace } => {
                write!(f, "Resource leak detected: {} {} leaked ({} allocations)", 
                       leaked_amount, resource_type, allocation_trace.len())
            }
        }
    }
}

/// Security and authorization errors with detailed context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityError {
    /// Authentication failed
    AuthenticationFailed {
        user_id: String,
        reason: String,
        attempt_count: u32,
        lockout_until: Option<std::time::SystemTime>,
    },
    /// Authorization denied
    AuthorizationDenied {
        user_id: String,
        resource: String,
        action: String,
        required_permissions: Vec<String>,
    },
    /// Certificate validation failed
    CertificateValidationFailed {
        certificate_subject: String,
        reason: String,
        expiry_date: Option<std::time::SystemTime>,
    },
    /// Security policy violation
    PolicyViolation {
        policy_name: String,
        violation_type: String,
        details: String,
    },
    /// Encryption/decryption failed
    CryptographicFailure {
        operation: String,
        algorithm: String,
        reason: String,
    },
    /// Rate limit exceeded
    RateLimitExceeded {
        user_id: String,
        endpoint: String,
        limit: u32,
        window: Duration,
        reset_time: std::time::SystemTime,
    },
}

impl ErrorCategory for SecurityError {
    fn category(&self) -> &'static str {
        "security"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            SecurityError::AuthenticationFailed { lockout_until, .. } => {
                let mut hints = vec![
                    RecoveryHint {
                        action: RecoveryAction::CheckConfiguration,
                        description: "Verify credentials are correct".to_string(),
                        automated: false,
                        priority: 1,
                        parameters: HashMap::new(),
                    },
                ];
                if let Some(lockout) = lockout_until {
                    hints.push(RecoveryHint {
                        action: RecoveryAction::RetryWithBackoff,
                        description: "Wait for account lockout to expire".to_string(),
                        automated: true,
                        priority: 2,
                        parameters: {
                            let mut params = HashMap::new();
                            params.insert("lockout_until".to_string(), 
                                         serde_json::json!(lockout.duration_since(std::time::UNIX_EPOCH)
                                                          .unwrap_or_default().as_secs()));
                            params
                        },
                    });
                }
                hints
            }
            SecurityError::AuthorizationDenied { required_permissions, .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::ContactSupport,
                    description: format!("Request permissions: {}", required_permissions.join(", ")),
                    automated: false,
                    priority: 1,
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("required_permissions".to_string(), serde_json::json!(required_permissions));
                        params
                    },
                },
            ],
            SecurityError::CertificateValidationFailed { expiry_date, .. } => {
                let mut hints = vec![
                    RecoveryHint {
                        action: RecoveryAction::UpdateConfiguration,
                        description: "Update certificate configuration".to_string(),
                        automated: false,
                        priority: 1,
                        parameters: HashMap::new(),
                    },
                ];
                if expiry_date.is_some() {
                    hints.push(RecoveryHint {
                        action: RecoveryAction::UpdateConfiguration,
                        description: "Renew expired certificate".to_string(),
                        automated: false,
                        priority: 1,
                        parameters: HashMap::new(),
                    });
                }
                hints
            }
            SecurityError::RateLimitExceeded { reset_time, .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::RetryWithBackoff,
                    description: "Wait for rate limit to reset".to_string(),
                    automated: true,
                    priority: 1,
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("reset_time".to_string(), 
                                     serde_json::json!(reset_time.duration_since(std::time::UNIX_EPOCH)
                                                      .unwrap_or_default().as_secs()));
                        params
                    },
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::ContactSupport,
                    description: "Contact security administrator".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
        }
    }

    fn is_transient_category(&self) -> bool {
        matches!(self, SecurityError::RateLimitExceeded { .. })
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            SecurityError::PolicyViolation { .. } => ErrorSeverity::Critical,
            SecurityError::CryptographicFailure { .. } => ErrorSeverity::Critical,
            SecurityError::AuthorizationDenied { .. } => ErrorSeverity::Error,
            SecurityError::RateLimitExceeded { .. } => ErrorSeverity::Warning,
            _ => ErrorSeverity::Error,
        }
    }
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityError::AuthenticationFailed { user_id, reason, attempt_count, lockout_until } => {
                match lockout_until {
                    Some(_) => write!(f, "Authentication failed for user '{}': {} (attempt {}, account locked)", 
                                     user_id, reason, attempt_count),
                    None => write!(f, "Authentication failed for user '{}': {} (attempt {})", 
                                  user_id, reason, attempt_count),
                }
            }
            SecurityError::AuthorizationDenied { user_id, resource, action, required_permissions } => {
                write!(f, "User '{}' denied '{}' access to '{}' (requires: {})", 
                       user_id, action, resource, required_permissions.join(", "))
            }
            SecurityError::CertificateValidationFailed { certificate_subject, reason, expiry_date } => {
                match expiry_date {
                    Some(expiry) => write!(f, "Certificate validation failed for '{}': {} (expires: {:?})", 
                                          certificate_subject, reason, expiry),
                    None => write!(f, "Certificate validation failed for '{}': {}", certificate_subject, reason),
                }
            }
            SecurityError::PolicyViolation { policy_name, violation_type, details } => {
                write!(f, "Security policy '{}' violation ({}): {}", policy_name, violation_type, details)
            }
            SecurityError::CryptographicFailure { operation, algorithm, reason } => {
                write!(f, "Cryptographic operation '{}' failed using {}: {}", operation, algorithm, reason)
            }
            SecurityError::RateLimitExceeded { user_id, endpoint, limit, window, .. } => {
                write!(f, "Rate limit exceeded for user '{}' on '{}': {} requests per {:?}", 
                       user_id, endpoint, limit, window)
            }
        }
    }
}

/// Network and connectivity errors with detailed context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkError {
    /// Connection timeout
    ConnectionTimeout {
        target: String,
        timeout: Duration,
        attempt_count: u32,
    },
    /// DNS resolution failed
    DnsResolutionFailed {
        hostname: String,
        nameserver: String,
        reason: String,
    },
    /// Protocol error
    ProtocolError {
        protocol: String,
        expected_version: String,
        actual_version: String,
    },
    /// Bandwidth limit exceeded
    BandwidthExceeded {
        interface: String,
        limit_mbps: u32,
        current_mbps: u32,
    },
    /// Network partition detected
    PartitionDetected {
        affected_nodes: Vec<u64>,
        partition_duration: Duration,
    },
    /// Connection pool exhausted
    ConnectionPoolExhausted {
        pool_name: String,
        max_connections: u32,
        current_connections: u32,
        queue_size: u32,
    },
}

impl ErrorCategory for NetworkError {
    fn category(&self) -> &'static str {
        "networking"
    }

    fn recovery_hints(&self) -> Vec<RecoveryHint> {
        match self {
            NetworkError::ConnectionTimeout { attempt_count, .. } => {
                let mut hints = vec![
                    RecoveryHint {
                        action: RecoveryAction::CheckConnectivity,
                        description: "Check network connectivity to target".to_string(),
                        automated: false,
                        priority: 1,
                        parameters: HashMap::new(),
                    },
                ];
                if *attempt_count < 3 {
                    hints.push(RecoveryHint {
                        action: RecoveryAction::RetryWithBackoff,
                        description: "Retry with exponential backoff".to_string(),
                        automated: true,
                        priority: 2,
                        parameters: HashMap::new(),
                    });
                }
                hints
            }
            NetworkError::ConnectionPoolExhausted { max_connections, .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::ScaleUp,
                    description: "Increase connection pool size".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: {
                        let mut params = HashMap::new();
                        params.insert("suggested_size".to_string(), serde_json::json!(max_connections * 2));
                        params
                    },
                },
                RecoveryHint {
                    action: RecoveryAction::ReduceLoad,
                    description: "Reduce concurrent connection usage".to_string(),
                    automated: true,
                    priority: 2,
                    parameters: HashMap::new(),
                },
            ],
            NetworkError::PartitionDetected { .. } => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConnectivity,
                    description: "Check network infrastructure for partition cause".to_string(),
                    automated: false,
                    priority: 1,
                    parameters: HashMap::new(),
                },
            ],
            _ => vec![
                RecoveryHint {
                    action: RecoveryAction::CheckConnectivity,
                    description: "Check network configuration and connectivity".to_string(),
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
            NetworkError::ConnectionTimeout { .. } 
            | NetworkError::BandwidthExceeded { .. }
            | NetworkError::ConnectionPoolExhausted { .. }
        )
    }

    fn default_severity(&self) -> ErrorSeverity {
        match self {
            NetworkError::PartitionDetected { .. } => ErrorSeverity::Critical,
            NetworkError::ConnectionPoolExhausted { .. } => ErrorSeverity::Error,
            NetworkError::BandwidthExceeded { .. } => ErrorSeverity::Warning,
            _ => ErrorSeverity::Error,
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::ConnectionTimeout { target, timeout, attempt_count } => {
                write!(f, "Connection timeout to '{}' after {:?} (attempt {})", target, timeout, attempt_count)
            }
            NetworkError::DnsResolutionFailed { hostname, nameserver, reason } => {
                write!(f, "DNS resolution failed for '{}' via {}: {}", hostname, nameserver, reason)
            }
            NetworkError::ProtocolError { protocol, expected_version, actual_version } => {
                write!(f, "Protocol {} version mismatch: expected {}, got {}", 
                       protocol, expected_version, actual_version)
            }
            NetworkError::BandwidthExceeded { interface, limit_mbps, current_mbps } => {
                write!(f, "Bandwidth exceeded on {}: {} Mbps (limit: {} Mbps)", 
                       interface, current_mbps, limit_mbps)
            }
            NetworkError::PartitionDetected { affected_nodes, partition_duration } => {
                write!(f, "Network partition detected affecting {} nodes for {:?}", 
                       affected_nodes.len(), partition_duration)
            }
            NetworkError::ConnectionPoolExhausted { pool_name, max_connections, current_connections, queue_size } => {
                write!(f, "Connection pool '{}' exhausted: {}/{} connections, {} queued", 
                       pool_name, current_connections, max_connections, queue_size)
            }
        }
    }
}

/// Conversion traits from domain errors to BlixardError
impl From<ResourceError> for BlixardError {
    fn from(error: ResourceError) -> Self {
        match error {
            ResourceError::QuotaExceeded { resource_type, limit, requested, .. } => {
                BlixardError::QuotaExceeded {
                    resource: resource_type,
                    limit,
                    requested,
                }
            }
            ResourceError::Unavailable { resource_type, resource_id, reason, .. } => {
                BlixardError::ResourceUnavailable {
                    resource_type,
                    message: format!("{}: {}", resource_id, reason),
                }
            }
            ResourceError::Exhausted { resource_type, .. } => {
                BlixardError::ResourceExhausted { resource: resource_type }
            }
            _ => BlixardError::Internal {
                message: error.to_string(),
            },
        }
    }
}

impl From<SecurityError> for BlixardError {
    fn from(error: SecurityError) -> Self {
        match error {
            SecurityError::AuthenticationFailed { user_id, reason, .. } => {
                BlixardError::AuthorizationError {
                    message: format!("Authentication failed for {}: {}", user_id, reason),
                }
            }
            SecurityError::AuthorizationDenied { user_id, resource, action, .. } => {
                BlixardError::AuthorizationError {
                    message: format!("User {} denied {} access to {}", user_id, action, resource),
                }
            }
            _ => BlixardError::Security {
                message: error.to_string(),
            },
        }
    }
}

impl From<NetworkError> for BlixardError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::ConnectionTimeout { target, timeout, .. } => {
                BlixardError::Timeout {
                    operation: format!("connect to {}", target),
                    duration: timeout,
                }
            }
            NetworkError::ConnectionPoolExhausted { pool_name, .. } => {
                BlixardError::ResourceExhausted {
                    resource: format!("connection pool {}", pool_name),
                }
            }
            _ => BlixardError::NetworkError(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_error_recovery_hints() {
        let error = ResourceError::QuotaExceeded {
            resource_type: "memory".to_string(),
            tenant_id: "tenant1".to_string(),
            limit: 1024,
            requested: 2048,
            current_usage: 512,
        };

        let hints = error.recovery_hints();
        assert!(!hints.is_empty());
        assert!(hints.iter().any(|h| matches!(h.action, RecoveryAction::CheckResources)));
        assert!(hints.iter().any(|h| matches!(h.action, RecoveryAction::ScaleUp)));
    }

    #[test]
    fn test_security_error_severity() {
        let error = SecurityError::PolicyViolation {
            policy_name: "test".to_string(),
            violation_type: "test".to_string(),
            details: "test".to_string(),
        };
        assert_eq!(error.default_severity(), ErrorSeverity::Critical);
    }

    #[test]
    fn test_network_error_transient() {
        let error = NetworkError::ConnectionTimeout {
            target: "test".to_string(),
            timeout: Duration::from_secs(5),
            attempt_count: 1,
        };
        assert!(error.is_transient_category());
    }

    #[test]
    fn test_error_conversion() {
        let resource_error = ResourceError::Exhausted {
            resource_type: "cpu".to_string(),
            node_id: 1,
            available: 2,
            required: 4,
            alternatives: vec![2, 3],
        };

        let blixard_error: BlixardError = resource_error.into();
        match blixard_error {
            BlixardError::ResourceExhausted { resource } => {
                assert_eq!(resource, "cpu");
            }
            _ => panic!("Unexpected error type"),
        }
    }
}
