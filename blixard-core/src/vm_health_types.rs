//! VM health check types and configuration
//!
//! This module defines the types of health checks that can be performed on VMs
//! and their configuration options.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Type of health check to perform on a VM
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HealthCheckType {
    /// HTTP health check - sends HTTP request to endpoint
    Http {
        /// URL to check (e.g., "http://localhost:8080/health")
        url: String,
        /// Expected HTTP status code (default: 200)
        #[serde(default = "default_expected_status")]
        expected_status: u16,
        /// Request timeout in seconds (default: 5)
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
        /// Optional headers to send
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<(String, String)>>,
    },

    /// TCP health check - attempts TCP connection
    Tcp {
        /// Host and port to connect to (e.g., "localhost:22")
        address: String,
        /// Connection timeout in seconds (default: 5)
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
    },

    /// Script health check - executes a script inside the VM
    Script {
        /// Command to execute (e.g., "/usr/local/bin/health-check.sh")
        command: String,
        /// Arguments to pass to the command
        #[serde(default)]
        args: Vec<String>,
        /// Expected exit code (default: 0)
        #[serde(default)]
        expected_exit_code: i32,
        /// Execution timeout in seconds (default: 10)
        #[serde(default = "default_script_timeout_secs")]
        timeout_secs: u64,
    },

    /// Console health check - monitors console output for patterns
    Console {
        /// Pattern to look for that indicates healthy state
        healthy_pattern: String,
        /// Pattern that indicates unhealthy state
        #[serde(skip_serializing_if = "Option::is_none")]
        unhealthy_pattern: Option<String>,
        /// How long to wait for pattern in seconds (default: 30)
        #[serde(default = "default_console_timeout_secs")]
        timeout_secs: u64,
    },

    /// Process health check - checks if specific process is running
    Process {
        /// Process name or command to check
        process_name: String,
        /// Minimum number of instances expected (default: 1)
        #[serde(default = "default_min_instances")]
        min_instances: u32,
    },

    /// Guest agent health check - communicates with guest agent
    GuestAgent {
        /// Timeout for agent response in seconds (default: 5)
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
    },
}

/// Health check configuration for a VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHealthCheckConfig {
    /// List of health checks to perform
    pub checks: Vec<HealthCheck>,

    /// How often to run health checks (in seconds)
    #[serde(default = "default_check_interval_secs")]
    pub check_interval_secs: u64,

    /// Number of consecutive failures before marking VM unhealthy
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Number of consecutive successes before marking VM healthy
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,

    /// Initial delay before starting health checks (in seconds)
    #[serde(default = "default_initial_delay_secs")]
    pub initial_delay_secs: u64,
}

/// Individual health check with its configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Unique name for this health check
    pub name: String,

    /// Type of health check
    pub check_type: HealthCheckType,

    /// Whether this check is critical (affects overall health status)
    #[serde(default = "default_critical")]
    pub critical: bool,

    /// Weight for this check in overall health score (0.0 - 1.0)
    #[serde(default = "default_weight")]
    pub weight: f32,

    /// Priority level for escalation (quick, deep, comprehensive)
    #[serde(default = "default_priority")]
    pub priority: HealthCheckPriority,

    /// Recovery actions to take if this check fails
    #[serde(default)]
    pub recovery_escalation: Option<RecoveryEscalation>,

    /// Minimum consecutive failures before triggering recovery
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
}

/// VM health status with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHealthStatus {
    /// Overall health state
    pub state: HealthState,

    /// Health score (0.0 - 1.0, where 1.0 is perfectly healthy)
    pub score: f32,

    /// Individual check results
    pub check_results: Vec<HealthCheckResult>,

    /// Last successful check timestamp (Unix timestamp in seconds)
    pub last_healthy_at_secs: Option<i64>,

    /// Consecutive failure count
    pub consecutive_failures: u32,

    /// Consecutive success count
    pub consecutive_successes: u32,
}

/// Overall health state of a VM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    /// VM is healthy and all critical checks pass
    Healthy,

    /// VM is degraded - some non-critical checks failing
    Degraded,

    /// VM is unhealthy - critical checks failing
    Unhealthy,

    /// VM is unresponsive - cannot perform health checks
    Unresponsive,

    /// VM requires immediate intervention - multiple failures
    Critical,

    /// Health status is unknown (e.g., checks not started yet)
    Unknown,
}

/// Health check priority levels for escalation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HealthCheckPriority {
    /// Quick health check - basic connectivity/process checks
    Quick = 0,
    /// Deep health check - comprehensive application-level checks
    Deep = 1,
    /// Comprehensive health check - full system validation
    Comprehensive = 2,
}

impl HealthCheckPriority {
    /// Get display name for the priority level
    pub fn name(&self) -> &'static str {
        match self {
            HealthCheckPriority::Quick => "quick",
            HealthCheckPriority::Deep => "deep",
            HealthCheckPriority::Comprehensive => "comprehensive",
        }
    }

    /// Get timeout multiplier for this priority level
    pub fn timeout_multiplier(&self) -> f32 {
        match self {
            HealthCheckPriority::Quick => 1.0,
            HealthCheckPriority::Deep => 2.0,
            HealthCheckPriority::Comprehensive => 3.0,
        }
    }
}

/// Recovery action to take when health checks fail
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// No action - just log the failure
    None,
    /// Restart the failing service within the VM
    RestartService { service_name: String },
    /// Restart the entire VM
    RestartVm,
    /// Migrate VM to another node
    MigrateVm { target_node: Option<u64> },
    /// Stop the VM (last resort)
    StopVm,
    /// Custom recovery script
    CustomScript { script_path: String, args: Vec<String> },
}

/// Recovery escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEscalation {
    /// Actions to try in order, with delays between attempts
    pub actions: Vec<(RecoveryAction, Duration)>,
    /// Maximum number of recovery attempts before giving up
    pub max_attempts: u32,
    /// Reset escalation level after this duration without failures
    pub reset_after: Duration,
}

impl Default for RecoveryEscalation {
    fn default() -> Self {
        Self {
            actions: vec![
                (RecoveryAction::None, Duration::from_secs(30)),
                (RecoveryAction::RestartVm, Duration::from_secs(300)),
                (RecoveryAction::MigrateVm { target_node: None }, Duration::from_secs(600)),
            ],
            max_attempts: 3,
            reset_after: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Result of an individual health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Name of the health check
    pub check_name: String,

    /// Whether the check passed
    pub success: bool,

    /// Detailed message about the check result
    pub message: String,

    /// How long the check took (in milliseconds)
    pub duration_ms: u64,

    /// When the check was performed (Unix timestamp in seconds)
    pub timestamp_secs: i64,

    /// Error if the check failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Priority level of this health check
    pub priority: HealthCheckPriority,

    /// Whether this check is critical
    pub critical: bool,

    /// Recovery action taken (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<RecoveryAction>,
}

impl Default for VmHealthCheckConfig {
    fn default() -> Self {
        Self {
            checks: vec![],
            check_interval_secs: default_check_interval_secs(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            initial_delay_secs: default_initial_delay_secs(),
        }
    }
}

impl Default for VmHealthStatus {
    fn default() -> Self {
        Self {
            state: HealthState::Unknown,
            score: 0.0,
            check_results: vec![],
            last_healthy_at_secs: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
        }
    }
}

// Default value functions for serde
fn default_expected_status() -> u16 {
    200
}
fn default_timeout_secs() -> u64 {
    5
}
fn default_script_timeout_secs() -> u64 {
    10
}
fn default_console_timeout_secs() -> u64 {
    30
}
fn default_min_instances() -> u32 {
    1
}
fn default_check_interval_secs() -> u64 {
    30
}
fn default_failure_threshold() -> u32 {
    3
}
fn default_success_threshold() -> u32 {
    2
}
fn default_initial_delay_secs() -> u64 {
    60
}
fn default_critical() -> bool {
    true
}
fn default_weight() -> f32 {
    1.0
}
fn default_priority() -> HealthCheckPriority {
    HealthCheckPriority::Quick
}

impl HealthState {
    /// Check if the health state indicates the VM needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(self, HealthState::Unhealthy | HealthState::Unresponsive | HealthState::Critical)
    }

    /// Check if the health state indicates the VM is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, HealthState::Healthy | HealthState::Degraded)
    }

    /// Get escalation level for recovery actions
    pub fn escalation_level(&self) -> u32 {
        match self {
            HealthState::Healthy => 0,
            HealthState::Degraded => 1,
            HealthState::Unhealthy => 2,
            HealthState::Unresponsive => 3,
            HealthState::Critical => 4,
            HealthState::Unknown => 0,
        }
    }

    /// Check if immediate action is required
    pub fn requires_immediate_action(&self) -> bool {
        matches!(self, HealthState::Critical | HealthState::Unresponsive)
    }
}

impl VmHealthStatus {
    /// Calculate overall health score based on check results
    pub fn calculate_score(&mut self) {
        if self.check_results.is_empty() {
            self.score = 0.0;
            return;
        }

        let total_weight: f32 = self.check_results.len() as f32;
        let success_weight: f32 = self.check_results.iter().filter(|r| r.success).count() as f32;

        self.score = success_weight / total_weight;
    }

    /// Update health state based on check results and thresholds
    pub fn update_state(&mut self, config: &VmHealthCheckConfig) {
        // Check if any critical checks failed
        let critical_failures = self
            .check_results
            .iter()
            .filter(|r| r.critical && !r.success)
            .count();

        let _total_critical = self
            .check_results
            .iter()
            .filter(|r| r.critical)
            .count();

        let comprehensive_failures = self
            .check_results
            .iter()
            .filter(|r| r.priority == HealthCheckPriority::Comprehensive && !r.success)
            .count();

        let total_comprehensive = self
            .check_results
            .iter()
            .filter(|r| r.priority == HealthCheckPriority::Comprehensive)
            .count();

        // Escalate health state based on failure patterns
        if critical_failures > 0 && self.consecutive_failures >= config.failure_threshold * 2 {
            self.state = HealthState::Critical;
        } else if critical_failures > 0 || comprehensive_failures == total_comprehensive && total_comprehensive > 0 {
            self.state = HealthState::Unhealthy;
        } else if self.check_results.is_empty() || self.consecutive_failures >= config.failure_threshold {
            self.state = HealthState::Unresponsive;
        } else if self.score >= 0.9 {
            self.state = HealthState::Healthy;
        } else if self.score >= 0.5 {
            self.state = HealthState::Degraded;
        } else {
            self.state = HealthState::Unhealthy;
        }

        // Update consecutive counters
        if self.state == HealthState::Healthy {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;
            self.last_healthy_at_secs = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            );
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
        }
    }

    /// Get the highest priority level that has failing checks
    pub fn get_highest_failing_priority(&self) -> Option<HealthCheckPriority> {
        let mut highest_priority = None;
        
        for result in &self.check_results {
            if !result.success {
                match highest_priority {
                    None => highest_priority = Some(result.priority),
                    Some(current) => {
                        if result.priority > current {
                            highest_priority = Some(result.priority);
                        }
                    }
                }
            }
        }
        
        highest_priority
    }

    /// Check if we should escalate to the next level of health checks
    pub fn should_escalate_checks(&self, current_priority: HealthCheckPriority) -> bool {
        match current_priority {
            HealthCheckPriority::Quick => {
                // Escalate to deep checks if quick checks are failing
                self.check_results
                    .iter()
                    .any(|r| r.priority == HealthCheckPriority::Quick && !r.success)
            }
            HealthCheckPriority::Deep => {
                // Escalate to comprehensive checks if deep checks are failing
                self.check_results
                    .iter()
                    .any(|r| r.priority == HealthCheckPriority::Deep && !r.success)
            }
            HealthCheckPriority::Comprehensive => {
                // Already at highest level
                false
            }
        }
    }

    /// Determine if recovery action should be triggered
    pub fn should_trigger_recovery(&self, check_name: &str, failure_threshold: u32) -> bool {
        // Count consecutive failures for this specific check
        let mut consecutive_failures = 0;
        
        // Look at recent results for this check (in reverse chronological order)
        for result in self.check_results.iter().rev() {
            if result.check_name == check_name {
                if result.success {
                    break; // Reset count on success
                } else {
                    consecutive_failures += 1;
                }
            }
        }
        
        consecutive_failures >= failure_threshold
    }
}
