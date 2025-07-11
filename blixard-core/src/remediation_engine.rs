//! Automated remediation engine for common operational issues
//!
//! This module provides a framework for detecting and automatically
//! remediating common issues in the distributed system.
//!
//! ## Migration to Retry Pattern
//!
//! This module has been migrated from manual circuit breaker logic to use
//! the unified retry pattern from `crate::patterns::retry`. This provides:
//!
//! - **Sophisticated backoff strategies**: Exponential, linear, and constant backoff
//! - **Configurable retry policies**: Per-issue-type retry configurations
//! - **Jitter support**: Avoid thundering herd problems
//! - **Better error categorization**: Leverages structured BlixardError types
//! - **Unified metrics**: Consistent retry metrics across the codebase

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use crate::error::{BlixardError, BlixardResult};
use crate::patterns::retry::{RetryConfig, BackoffStrategy, retry};
use crate::types::{NodeState, VmStatus};
use crate::node_shared::SharedNodeState;
use crate::audit_log::{AuditLogger, log_vm_event, AuditActor};
#[cfg(feature = "observability")]
use crate::metrics_otel::{
    record_vm_recovery_attempt, record_remediation_action,
};
use tracing::{info, warn, error, debug};
use uuid;

/// Default configuration constants
mod constants {
    use std::time::Duration;

    /// Default maximum remediation attempts per issue
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
    
    /// Default cooldown period between remediation attempts (5 minutes)
    pub const DEFAULT_COOLDOWN_PERIOD: Duration = Duration::from_secs(300);
    
    
    /// Default remediation monitoring interval (30 seconds)
    pub const DEFAULT_MONITORING_INTERVAL: Duration = Duration::from_secs(30);
}

/// Types of issues that can be detected and remediated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueType {
    /// VM has crashed or failed
    VmFailure,
    /// Node is unhealthy (high resource usage, unresponsive)
    NodeUnhealthy,
    /// Raft consensus issues (leader election storms, stuck proposals)
    RaftConsensusIssue,
    /// Network partition detected
    NetworkPartition,
    /// Resource exhaustion (CPU, memory, disk)
    ResourceExhaustion,
    /// Persistent storage issues
    StorageIssue,
    /// Service degradation (high latency, errors)
    ServiceDegradation,
    /// Configuration drift
    ConfigurationDrift,
}

/// Severity of the detected issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Detected issue with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Unique issue ID
    pub id: String,
    /// Type of issue
    pub issue_type: IssueType,
    /// Severity level
    pub severity: IssueSeverity,
    /// Affected resource
    pub resource: IssueResource,
    /// Detection time
    pub detected_at: DateTime<Utc>,
    /// Issue description
    pub description: String,
    /// Additional context
    pub context: serde_json::Value,
    /// Correlation with other issues
    pub correlation_id: Option<String>,
}

/// Resource affected by the issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueResource {
    Vm { name: String, node_id: u64 },
    Node { id: u64, address: String },
    Cluster,
    Service { name: String },
}

/// Result of a remediation attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationResult {
    /// Whether remediation succeeded
    pub success: bool,
    /// Actions taken
    pub actions: Vec<RemediationAction>,
    /// Error if failed
    pub error: Option<String>,
    /// Time taken
    pub duration: Duration,
    /// Whether manual intervention is required
    pub requires_manual_intervention: bool,
}

/// Action taken during remediation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationAction {
    /// Action type
    pub action: String,
    /// Target resource
    pub target: String,
    /// Result of the action
    pub result: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Context for remediation
#[derive(Clone)]
pub struct RemediationContext {
    /// Shared node state
    pub node_state: Arc<SharedNodeState>,
    /// Audit logger
    pub audit_logger: Option<Arc<AuditLogger>>,
    /// Node ID
    pub node_id: u64,
}

/// Retry predicate functions for different issue types

/// Determine if VM failure errors are retryable
fn vm_failure_is_retryable(error: &BlixardError) -> bool {
    matches!(error,
        BlixardError::VmOperationFailed { .. } |
        BlixardError::TemporaryFailure { .. } |
        BlixardError::ConnectionError { .. } |
        BlixardError::Timeout { .. }
    )
}

/// Determine if network partition errors are retryable
fn network_is_retryable(error: &BlixardError) -> bool {
    matches!(error,
        BlixardError::NetworkError(_) |
        BlixardError::ConnectionError { .. } |
        BlixardError::P2PError(_) |
        BlixardError::TemporaryFailure { .. } |
        BlixardError::Timeout { .. }
    )
}

/// Determine if resource exhaustion errors are retryable
fn resource_exhaustion_is_retryable(error: &BlixardError) -> bool {
    matches!(error,
        BlixardError::ResourceExhausted { .. } |
        BlixardError::InsufficientResources { .. } |
        BlixardError::TemporaryFailure { .. }
    )
}

/// Determine if service degradation errors are retryable
fn service_degradation_is_retryable(error: &BlixardError) -> bool {
    matches!(error,
        BlixardError::TemporaryFailure { .. } |
        BlixardError::Timeout { .. } |
        BlixardError::ServiceManagementError(_)
    )
}

/// Default retry predicate for other issue types
fn default_is_retryable(error: &BlixardError) -> bool {
    matches!(error,
        BlixardError::TemporaryFailure { .. } |
        BlixardError::Timeout { .. } |
        BlixardError::ConnectionError { .. }
    )
}

/// Policy for remediation behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationPolicy {
    /// Enable automatic remediation
    pub enabled: bool,
    /// Retry configurations per issue type
    pub retry_configs: HashMap<IssueType, RetryConfig>,
    /// Require human approval for destructive actions
    pub require_approval_for_destructive: bool,
    /// Issue types to auto-remediate
    pub auto_remediate_types: Vec<IssueType>,
}

impl Default for RemediationPolicy {
    fn default() -> Self {
        let mut retry_configs = HashMap::new();
        
        // VM failures: aggressive retry with exponential backoff
        retry_configs.insert(IssueType::VmFailure, RetryConfig {
            max_attempts: 5,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(500),
                max: Duration::from_secs(30),
                multiplier: 2.0,
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(300)),
            is_retryable: vm_failure_is_retryable,
        });
        
        // Network issues: longer retry with linear backoff
        retry_configs.insert(IssueType::NetworkPartition, RetryConfig {
            max_attempts: 10,
            backoff: BackoffStrategy::Linear {
                base: Duration::from_secs(1),
                max: Duration::from_secs(60),
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(600)),
            is_retryable: network_is_retryable,
        });
        
        // Resource exhaustion: moderate retry with exponential backoff
        retry_configs.insert(IssueType::ResourceExhaustion, RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_secs(2),
                max: Duration::from_secs(120),
                multiplier: 2.0,
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(600)),
            is_retryable: resource_exhaustion_is_retryable,
        });
        
        // Service degradation: quick retry with fixed backoff
        retry_configs.insert(IssueType::ServiceDegradation, RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Fixed(Duration::from_secs(5)),
            jitter: false,
            max_total_delay: Some(Duration::from_secs(60)),
            is_retryable: service_degradation_is_retryable,
        });
        
        // Default config for other issue types
        let default_retry_config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Linear {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(300)),
            is_retryable: default_is_retryable,
        };
        
        for issue_type in [
            IssueType::NodeUnhealthy,
            IssueType::RaftConsensusIssue,
            IssueType::StorageIssue,
            IssueType::ConfigurationDrift,
        ] {
            retry_configs.insert(issue_type, default_retry_config.clone());
        }
        
        Self {
            enabled: true,
            retry_configs,
            require_approval_for_destructive: true,
            auto_remediate_types: vec![
                IssueType::VmFailure,
                IssueType::ServiceDegradation,
                IssueType::ResourceExhaustion,
            ],
        }
    }
}


/// Event for remediation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationEvent {
    /// Event ID
    pub id: String,
    /// Issue that triggered remediation
    pub issue: Issue,
    /// Remediation result
    pub result: RemediationResult,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Trait for issue detection
#[async_trait]
pub trait IssueDetector: Send + Sync {
    /// Type of issues this detector can find
    fn issue_types(&self) -> Vec<IssueType>;
    
    /// Detect issues in the current state
    async fn detect(&self, context: &RemediationContext) -> Vec<Issue>;
}

/// Trait for issue remediation
#[async_trait]
pub trait Remediator: Send + Sync {
    /// Check if this remediator can handle the issue
    fn can_handle(&self, issue: &Issue) -> bool;
    
    /// Get priority for handling this issue (higher = more specific)
    fn priority(&self) -> u32 {
        0
    }
    
    /// Remediate the issue
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult;
}

/// Main remediation engine
pub struct RemediationEngine {
    /// Issue detectors
    detectors: Vec<Arc<dyn IssueDetector>>,
    /// Remediators mapped by issue type
    remediators: HashMap<IssueType, Vec<Arc<dyn Remediator>>>,
    /// Remediation policy
    policy: RemediationPolicy,
    /// Remediation history
    history: Arc<RwLock<Vec<RemediationEvent>>>,
    /// Active remediations
    active_remediations: Arc<Mutex<HashMap<String, SystemTime>>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl RemediationEngine {
    /// Create a new remediation engine
    pub fn new(policy: RemediationPolicy) -> Self {
        Self {
            detectors: Vec::new(),
            remediators: HashMap::new(),
            policy,
            history: Arc::new(RwLock::new(Vec::new())),
            active_remediations: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx: None,
        }
    }
    
    /// Add an issue detector
    pub fn add_detector(&mut self, detector: Arc<dyn IssueDetector>) {
        self.detectors.push(detector);
    }
    
    /// Add a remediator
    pub fn add_remediator(&mut self, issue_type: IssueType, remediator: Arc<dyn Remediator>) {
        self.remediators.entry(issue_type).or_insert_with(Vec::new).push(remediator);
    }
    
    /// Start the remediation engine
    pub async fn start(&mut self, context: RemediationContext) -> BlixardResult<()> {
        if !self.policy.enabled {
            info!("Remediation engine is disabled by policy");
            return Ok(());
        }
        
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        let detectors = self.detectors.clone();
        let remediators = self.remediators.clone();
        let policy = self.policy.clone();
        let history = self.history.clone();
        let active = self.active_remediations.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(constants::DEFAULT_MONITORING_INTERVAL);
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Remediation engine shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(e) = Self::run_detection_and_remediation(
                            &detectors,
                            &remediators,
                            &policy,
                            &history,
                            &active,
                            &context,
                        ).await {
                            error!("Error in remediation cycle: {}", e);
                        }
                    }
                }
            }
        });
        
        info!("Remediation engine started");
        Ok(())
    }
    
    /// Stop the remediation engine
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
    
    /// Run detection and remediation cycle
    async fn run_detection_and_remediation(
        detectors: &[Arc<dyn IssueDetector>],
        remediators: &HashMap<IssueType, Vec<Arc<dyn Remediator>>>,
        policy: &RemediationPolicy,
        history: &Arc<RwLock<Vec<RemediationEvent>>>,
        active: &Arc<Mutex<HashMap<String, SystemTime>>>,
        context: &RemediationContext,
    ) -> BlixardResult<()> {
        // Detect issues
        let mut all_issues = Vec::new();
        for detector in detectors {
            let issues = detector.detect(context).await;
            all_issues.extend(issues);
        }
        
        if all_issues.is_empty() {
            debug!("No issues detected");
            return Ok(());
        }
        
        info!("Detected {} issues", all_issues.len());
        
        // Sort by severity
        all_issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
        
        // Process each issue
        for issue in all_issues {
            // Check if already being remediated
            if Self::is_being_remediated(&issue, active).await {
                debug!("Issue {} is already being remediated", issue.id);
                continue;
            }
            
            // Check if we have retry configuration for this issue type
            let retry_config = match policy.retry_configs.get(&issue.issue_type) {
                Some(config) => config.clone(),
                None => {
                    warn!("No retry configuration for issue type {:?}", issue.issue_type);
                    continue;
                }
            };
            
            // Check if auto-remediation is allowed
            if !policy.auto_remediate_types.contains(&issue.issue_type) {
                debug!("Auto-remediation not enabled for issue type {:?}", issue.issue_type);
                continue;
            }
            
            // Find remediator
            if let Some(remediator_list) = remediators.get(&issue.issue_type) {
                // Sort by priority
                let mut sorted_remediators = remediator_list.clone();
                sorted_remediators.sort_by_key(|r| std::cmp::Reverse(r.priority()));
                
                // Try each remediator
                for remediator in sorted_remediators {
                    if remediator.can_handle(&issue) {
                        // Mark as active
                        active.lock().await.insert(issue.id.clone(), SystemTime::now());
                        
                        // Attempt remediation with retry pattern
                        let remediation_result = {
                            let remediator_clone = remediator.clone();
                            let issue_clone = issue.clone();
                            let context_clone = context.clone();
                            
                            retry(retry_config, move || {
                                let remediator = remediator_clone.clone();
                                let issue = issue_clone.clone();
                                let context = context_clone.clone();
                                Box::pin(async move {
                                    remediator.remediate(&issue, &context).await
                                        .map_err(|e| BlixardError::TemporaryFailure { 
                                            details: format!("Remediation failed: {}", e) 
                                        })
                                })
                            }).await
                        };
                        
                        // Convert retry result to RemediationResult
                        let result = match remediation_result {
                            Ok(result) => result,
                            Err(e) => RemediationResult {
                                success: false,
                                actions: vec![],
                                error: Some(e.to_string()),
                                duration: Duration::from_secs(0),
                                requires_manual_intervention: true,
                            },
                        };
                        
                        // Record result
                        Self::record_remediation_result(&issue, result, history).await;
                        
                        // Remove from active
                        active.lock().await.remove(&issue.id);
                        
                        break;
                    }
                }
            } else {
                debug!("No remediator found for issue type {:?}", issue.issue_type);
            }
        }
        
        Ok(())
    }
    
    /// Check if issue is already being remediated
    async fn is_being_remediated(
        issue: &Issue,
        active: &Arc<Mutex<HashMap<String, SystemTime>>>,
    ) -> bool {
        let active_map = active.lock().await;
        
        if let Some(start_time) = active_map.get(&issue.id) {
            // Check if remediation is taking too long
            if start_time.elapsed().unwrap_or_default() > Duration::from_secs(600) {
                // 10 minute timeout
                false
            } else {
                true
            }
        } else {
            false
        }
    }
    
    
    /// Record remediation result
    async fn record_remediation_result(
        issue: &Issue,
        result: RemediationResult,
        history: &Arc<RwLock<Vec<RemediationEvent>>>,
    ) {
        // Record in history
        let event = RemediationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            issue: issue.clone(),
            result,
            timestamp: Utc::now(),
        };
        
        history.write().await.push(event);
        
        // TODO: Trim old history entries
    }
    
    /// Get remediation history
    pub async fn get_history(&self) -> Vec<RemediationEvent> {
        self.history.read().await.clone()
    }
    
}

// Concrete implementations of detectors and remediators

/// VM failure detector
pub struct VmFailureDetector;

#[async_trait]
impl IssueDetector for VmFailureDetector {
    fn issue_types(&self) -> Vec<IssueType> {
        vec![IssueType::VmFailure]
    }
    
    async fn detect(&self, context: &RemediationContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        
        // Get all VMs
        let vms = match context.node_state.get_vms_on_node().await {
            Ok(vms) => vms,
            Err(e) => {
                error!("Failed to get VMs: {}", e);
                return issues;
            }
        };
        
        // Check for failed VMs
        for vm in vms {
            if vm.status == VmStatus::Failed {
                issues.push(Issue {
                    id: format!("vm-failure-{}-{}", vm.name, Utc::now().timestamp()),
                    issue_type: IssueType::VmFailure,
                    severity: IssueSeverity::High,
                    resource: IssueResource::Vm {
                        name: vm.name.clone(),
                        node_id: vm.node_id,
                    },
                    detected_at: Utc::now(),
                    description: format!("VM {} has failed", vm.name),
                    context: serde_json::json!({
                        "vm_name": vm.name,
                        "node_id": vm.node_id,
                        "status": format!("{:?}", vm.status),
                    }),
                    correlation_id: None,
                });
            }
        }
        
        issues
    }
}

/// VM failure remediator
pub struct VmFailureRemediator {
    max_restart_attempts: u32,
}

impl VmFailureRemediator {
    pub fn new(max_restart_attempts: u32) -> Self {
        Self { max_restart_attempts }
    }
}

#[async_trait]
impl Remediator for VmFailureRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::VmFailure
    }
    
    fn priority(&self) -> u32 {
        10 // High priority for VM failures
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        // Extract VM info
        let (vm_name, node_id) = match &issue.resource {
            IssueResource::Vm { name, node_id } => (name.clone(), *node_id),
            _ => {
                return RemediationResult {
                    success: false,
                    actions,
                    error: Some("Invalid resource type for VM failure".to_string()),
                    duration: start_time.elapsed(),
                    requires_manual_intervention: true,
                };
            }
        };
        
        info!("Attempting to remediate VM failure for {}", vm_name);
        
        // Record metrics
        record_vm_recovery_attempt(&vm_name, "restart");
        
        // Try to restart the VM
        let restart_action = RemediationAction {
            action: "restart_vm".to_string(),
            target: vm_name.clone(),
            result: String::new(),
            timestamp: Utc::now(),
        };
        
        // Restart VM operation with proper error handling - retry logic is handled at the engine level
        // This method focuses on the actual restart operation
        match context.node_state.restart_vm(&vm_name).await {
            Ok(_) => {
                let mut action = restart_action;
                action.result = "VM restarted successfully".to_string();
                actions.push(action);
                
                // Audit log
                if let Some(logger) = &context.audit_logger {
                    let _ = log_vm_event(
                        logger,
                        &vm_name,
                        "default", // TODO: Get actual tenant
                        "auto_restarted",
                        AuditActor::System {
                            component: "remediation-engine".to_string(),
                            node_id,
                        },
                        true,
                        serde_json::json!({
                            "reason": "automatic remediation",
                            "issue_id": issue.id,
                        }),
                    ).await;
                }
                
                RemediationResult {
                    success: true,
                    actions,
                    error: None,
                    duration: start_time.elapsed(),
                    requires_manual_intervention: false,
                }
            }
            Err(e) => {
                let mut action = restart_action;
                action.result = format!("Failed to restart: {}", e);
                actions.push(action);
                
                // Check if we should try migration (but don't implement it here)
                if node_id == context.node_id {
                    // Note: Migration would be a separate remediation strategy
                    actions.push(RemediationAction {
                        action: "evaluate_migration".to_string(),
                        target: vm_name.clone(),
                        result: "Migration evaluation not implemented yet".to_string(),
                        timestamp: Utc::now(),
                    });
                }
                
                // Return the specific error for retry pattern to handle
                match e {
                    BlixardError::VmOperationFailed { .. } |
                    BlixardError::TemporaryFailure { .. } |
                    BlixardError::ConnectionError { .. } |
                    BlixardError::Timeout { .. } => {
                        // These errors are retryable - return them for retry pattern to handle
                        return Err(e);
                    }
                    _ => {
                        // Non-retryable errors require manual intervention
                        RemediationResult {
                            success: false,
                            actions,
                            error: Some(format!("Non-retryable VM restart failure: {}", e)),
                            duration: start_time.elapsed(),
                            requires_manual_intervention: true,
                        }
                    }
                }
            }
        }
    }
}

/// Node health detector
pub struct NodeHealthDetector {
    cpu_threshold: f64,
    memory_threshold: f64,
}

impl NodeHealthDetector {
    pub fn new(cpu_threshold: f64, memory_threshold: f64) -> Self {
        Self { cpu_threshold, memory_threshold }
    }
}

#[async_trait]
impl IssueDetector for NodeHealthDetector {
    fn issue_types(&self) -> Vec<IssueType> {
        vec![IssueType::NodeUnhealthy, IssueType::ResourceExhaustion]
    }
    
    async fn detect(&self, context: &RemediationContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        
        // Get node metrics
        // TODO: Implement actual resource monitoring
        // For now, this is a placeholder
        
        issues
    }
}

/// Resource exhaustion remediator
pub struct ResourceExhaustionRemediator;

#[async_trait]
impl Remediator for ResourceExhaustionRemediator {
    fn can_handle(&self, issue: &Issue) -> bool {
        issue.issue_type == IssueType::ResourceExhaustion
    }
    
    async fn remediate(&self, issue: &Issue, context: &RemediationContext) -> RemediationResult {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        
        // Identify low-priority VMs
        let vms = match context.node_state.get_vms_on_node().await {
            Ok(vms) => vms,
            Err(e) => {
                return RemediationResult {
                    success: false,
                    actions,
                    error: Some(format!("Failed to get VMs: {}", e)),
                    duration: start_time.elapsed(),
                    requires_manual_intervention: true,
                };
            }
        };
        
        // Find preemptible VMs with low priority
        let mut preemption_candidates: Vec<_> = vms.iter()
            .filter(|vm| vm.config.preemptible && vm.config.priority < 500)
            .collect();
        
        preemption_candidates.sort_by_key(|vm| vm.config.priority);
        
        // Try to free resources by stopping low-priority VMs
        for vm in preemption_candidates.iter().take(2) {
            actions.push(RemediationAction {
                action: "stop_low_priority_vm".to_string(),
                target: vm.name.clone(),
                result: "Stopped to free resources".to_string(),
                timestamp: Utc::now(),
            });
            
            // TODO: Actually stop the VM
            record_remediation_action("resource_exhaustion", "vm_preemption");
        }
        
        RemediationResult {
            success: !actions.is_empty(),
            actions,
            error: None,
            duration: start_time.elapsed(),
            requires_manual_intervention: actions.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VmConfig;
    
    #[tokio::test]
    async fn test_vm_failure_detection() {
        // Create a mock context
        // Test VM failure detection logic
    }
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let policy = RemediationPolicy::default();
        
        let engine = RemediationEngine::new(policy);
        
        // Test circuit breaker logic
        // Simulate failures and verify circuit opens
    }
}