//! Configuration structures for VM health monitoring components
//!
//! This module provides centralized configuration management for the VM health monitoring
//! system, supporting the LifecycleManager pattern and dependency injection.

use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::vm_auto_recovery::RecoveryPolicy;

/// Comprehensive configuration for VM health monitoring system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHealthMonitorConfig {
    /// Health check scheduler configuration
    pub scheduler: HealthCheckSchedulerConfig,
    /// Health state management configuration
    pub state_manager: HealthStateManagerConfig,
    /// Recovery coordinator configuration
    pub recovery: RecoveryCoordinatorConfig,
    /// Global feature flags
    pub features: HealthMonitorFeatures,
}

impl Default for VmHealthMonitorConfig {
    fn default() -> Self {
        Self {
            scheduler: HealthCheckSchedulerConfig::default(),
            state_manager: HealthStateManagerConfig::default(),
            recovery: RecoveryCoordinatorConfig::default(),
            features: HealthMonitorFeatures::default(),
        }
    }
}

/// Configuration for the health check scheduler component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckSchedulerConfig {
    /// Interval between health check cycles
    pub check_interval: Duration,
    /// Maximum number of concurrent health checks
    pub max_concurrent_checks: usize,
    /// Timeout for individual health check operations
    pub health_check_timeout: Duration,
    /// Whether to enable process status checking
    pub enable_process_checks: bool,
    /// Whether to enable detailed health checks
    pub enable_detailed_checks: bool,
}

impl Default for HealthCheckSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            max_concurrent_checks: 10,
            health_check_timeout: Duration::from_secs(10),
            enable_process_checks: true,
            enable_detailed_checks: true,
        }
    }
}

/// Configuration for health state management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStateManagerConfig {
    /// Maximum age for health state entries
    pub max_state_age: Duration,
    /// Interval for cleanup of old state entries
    pub cleanup_interval: Duration,
    /// Maximum number of health check results to retain per VM
    pub max_results_per_vm: usize,
    /// Whether to persist health state to disk
    pub enable_persistence: bool,
}

impl Default for HealthStateManagerConfig {
    fn default() -> Self {
        Self {
            max_state_age: Duration::from_hours(24),
            cleanup_interval: Duration::from_hours(1),
            max_results_per_vm: 50,
            enable_persistence: false,
        }
    }
}

/// Configuration for recovery coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryCoordinatorConfig {
    /// Recovery policy for failed VMs
    pub recovery_policy: RecoveryPolicy,
    /// Maximum number of concurrent recovery operations
    pub max_concurrent_recoveries: usize,
    /// Whether to enable automatic recovery
    pub enable_auto_recovery: bool,
    /// Whether to enable VM migration on recovery failures
    pub enable_migration: bool,
    /// Timeout for recovery operations
    pub recovery_timeout: Duration,
}

impl Default for RecoveryCoordinatorConfig {
    fn default() -> Self {
        Self {
            recovery_policy: RecoveryPolicy::default(),
            max_concurrent_recoveries: 3,
            enable_auto_recovery: true,
            enable_migration: true,
            recovery_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Feature flags for health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMonitorFeatures {
    /// Enable comprehensive metrics collection
    pub enable_metrics: bool,
    /// Enable tracing for debugging
    pub enable_tracing: bool,
    /// Enable health history retention
    pub enable_history: bool,
    /// Enable alerting for health state changes
    pub enable_alerting: bool,
}

impl Default for HealthMonitorFeatures {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            enable_tracing: true,
            enable_history: true,
            enable_alerting: false,
        }
    }
}

/// Dependencies required for VM health monitoring components
#[derive(Clone)]
pub struct VmHealthMonitorDependencies {
    /// Shared node state for consensus operations
    pub node_state: std::sync::Arc<crate::node_shared::SharedNodeState>,
    /// VM manager for backend operations
    pub vm_manager: std::sync::Arc<crate::vm_backend::VmManager>,
    /// Clock abstraction for testability
    pub clock: std::sync::Arc<dyn crate::abstractions::time::Clock>,
}

impl VmHealthMonitorDependencies {
    /// Create new dependencies with default clock
    pub fn new(
        node_state: std::sync::Arc<crate::node_shared::SharedNodeState>,
        vm_manager: std::sync::Arc<crate::vm_backend::VmManager>,
    ) -> Self {
        Self {
            node_state,
            vm_manager,
            clock: std::sync::Arc::new(crate::abstractions::time::SystemClock::new()),
        }
    }
    
    /// Create dependencies with custom clock (useful for testing)
    pub fn with_clock(
        node_state: std::sync::Arc<crate::node_shared::SharedNodeState>,
        vm_manager: std::sync::Arc<crate::vm_backend::VmManager>,
        clock: std::sync::Arc<dyn crate::abstractions::time::Clock>,
    ) -> Self {
        Self {
            node_state,
            vm_manager,
            clock,
        }
    }
}