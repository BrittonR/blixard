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
    
    /// Health status is unknown (e.g., checks not started yet)
    Unknown,
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
fn default_expected_status() -> u16 { 200 }
fn default_timeout_secs() -> u64 { 5 }
fn default_script_timeout_secs() -> u64 { 10 }
fn default_console_timeout_secs() -> u64 { 30 }
fn default_min_instances() -> u32 { 1 }
fn default_check_interval_secs() -> u64 { 30 }
fn default_failure_threshold() -> u32 { 3 }
fn default_success_threshold() -> u32 { 2 }
fn default_initial_delay_secs() -> u64 { 60 }
fn default_critical() -> bool { true }
fn default_weight() -> f32 { 1.0 }

impl HealthState {
    /// Check if the health state indicates the VM needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(self, HealthState::Unhealthy | HealthState::Unresponsive)
    }
    
    /// Check if the health state indicates the VM is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, HealthState::Healthy | HealthState::Degraded)
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
        let success_weight: f32 = self.check_results
            .iter()
            .filter(|r| r.success)
            .count() as f32;
        
        self.score = success_weight / total_weight;
    }
    
    /// Update health state based on check results and thresholds
    pub fn update_state(&mut self, config: &VmHealthCheckConfig) {
        // Check if any critical checks failed
        let critical_failures = self.check_results
            .iter()
            .any(|r| !r.success && r.check_name.contains("critical"));
        
        if critical_failures {
            self.state = HealthState::Unhealthy;
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
                    .as_secs() as i64
            );
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
        }
    }
}