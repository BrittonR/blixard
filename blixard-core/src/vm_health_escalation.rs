//! VM Health Check Escalation and Recovery Coordination
//!
//! This module provides advanced health monitoring capabilities with:
//! - Multi-level health check escalation (quick → deep → comprehensive)
//! - Automatic recovery action coordination
//! - Circuit breaker integration for external dependencies
//! - Graceful degradation when monitoring systems fail
//! - Comprehensive observability and metrics

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn, error};

use crate::{
    error::{BlixardError, BlixardResult},
    patterns::DegradationStats,
    node_shared::SharedNodeState,
    patterns::{CircuitBreaker, CircuitBreakerConfig, GracefulDegradation, ServiceLevel, RetryConfig, retry},
    vm_backend::VmManager,
    vm_health_types::{
        HealthCheck, HealthCheckPriority, HealthCheckResult, HealthState, 
        RecoveryAction, RecoveryEscalation, VmHealthStatus
    },
    types::VmConfig,
};

/// Escalation state for a specific VM's health monitoring
#[derive(Debug, Clone)]
struct VmEscalationState {
    /// Current health check priority level being executed
    current_priority: HealthCheckPriority,
    /// Number of escalations for this VM
    escalation_count: u32,
    /// Last escalation timestamp
    last_escalation: Option<Instant>,
    /// Active recovery operations
    active_recoveries: Vec<RecoveryOperation>,
    /// Circuit breakers for external dependencies
    circuit_breakers: HashMap<String, CircuitBreaker>,
    /// Last successful health check timestamp
    last_successful_check: Option<Instant>,
}

impl VmEscalationState {
    fn new() -> Self {
        Self {
            current_priority: HealthCheckPriority::Quick,
            escalation_count: 0,
            last_escalation: None,
            active_recoveries: Vec::new(),
            circuit_breakers: HashMap::new(),
            last_successful_check: None,
        }
    }

    /// Check if enough time has passed to reset escalation level
    fn should_reset_escalation(&self, reset_duration: Duration) -> bool {
        if let Some(last_escalation) = self.last_escalation {
            last_escalation.elapsed() > reset_duration
        } else {
            false
        }
    }

    /// Reset escalation to quick checks
    fn reset_escalation(&mut self) {
        self.current_priority = HealthCheckPriority::Quick;
        self.escalation_count = 0;
        self.last_escalation = None;
    }
}

/// Active recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    /// Unique identifier for this operation
    pub id: String,
    /// VM name being recovered
    pub vm_name: String,
    /// Recovery action being performed
    pub action: RecoveryAction,
    /// When the operation started
    pub started_at: Instant,
    /// Timeout for this operation
    pub timeout: Duration,
    /// Current status
    pub status: RecoveryStatus,
    /// Error message if failed
    pub error: Option<String>,
}

/// Status of a recovery operation
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStatus {
    /// Operation is running
    Running,
    /// Operation completed successfully
    Success,
    /// Operation failed
    Failed,
    /// Operation timed out
    TimedOut,
    /// Operation was cancelled
    Cancelled,
}

/// Configuration for health check escalation
#[derive(Debug, Clone)]
pub struct EscalationConfig {
    /// Time between quick health checks
    pub quick_check_interval: Duration,
    /// Time between deep health checks  
    pub deep_check_interval: Duration,
    /// Time between comprehensive health checks
    pub comprehensive_check_interval: Duration,
    /// Time to wait before escalating to next level
    pub escalation_delay: Duration,
    /// Time to wait before resetting escalation level
    pub reset_escalation_after: Duration,
    /// Maximum number of escalations before triggering emergency procedures
    pub max_escalations: u32,
    /// Enable circuit breakers for external health check dependencies
    pub enable_circuit_breakers: bool,
    /// Enable graceful degradation of monitoring when systems fail
    pub enable_graceful_degradation: bool,
    /// Maximum concurrent recovery operations
    pub max_concurrent_recoveries: usize,
    /// Default timeout for recovery operations
    pub recovery_timeout: Duration,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            quick_check_interval: Duration::from_secs(15),
            deep_check_interval: Duration::from_secs(60),
            comprehensive_check_interval: Duration::from_secs(300),
            escalation_delay: Duration::from_secs(30),
            reset_escalation_after: Duration::from_secs(600), // 10 minutes
            max_escalations: 3,
            enable_circuit_breakers: true,
            enable_graceful_degradation: true,
            max_concurrent_recoveries: 5,
            recovery_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Advanced health monitoring with escalation and recovery
pub struct VmHealthEscalationManager {
    config: EscalationConfig,
    node_state: Arc<SharedNodeState>,
    vm_manager: Arc<VmManager>,
    escalation_states: Arc<RwLock<HashMap<String, VmEscalationState>>>,
    active_recoveries: Arc<RwLock<HashMap<String, RecoveryOperation>>>,
    graceful_degradation: Arc<GracefulDegradation>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
    shutdown_signal: Arc<tokio::sync::broadcast::Sender<()>>,
}

impl VmHealthEscalationManager {
    /// Create a new escalation manager
    pub fn new(
        config: EscalationConfig,
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
    ) -> Self {
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        let graceful_degradation = Arc::new(GracefulDegradation::new());

        // Register health monitoring services for graceful degradation
        let degradation = graceful_degradation.clone();
        tokio::spawn(async move {
            let _ = degradation.register_service("vm_health_monitoring", ServiceLevel::Critical).await;
            let _ = degradation.register_service("health_check_execution", ServiceLevel::Important).await;
            let _ = degradation.register_service("recovery_coordination", ServiceLevel::Important).await;
            let _ = degradation.register_service("metrics_collection", ServiceLevel::Optional).await;
        });

        Self {
            config,
            node_state,
            vm_manager,
            escalation_states: Arc::new(RwLock::new(HashMap::new())),
            active_recoveries: Arc::new(RwLock::new(HashMap::new())),
            graceful_degradation,
            background_tasks: Mutex::new(Vec::new()),
            shutdown_signal: Arc::new(shutdown_tx),
        }
    }

    /// Start the escalation manager
    pub async fn start(&self) -> BlixardResult<()> {
        info!("Starting VM health escalation manager");

        // Start background monitoring task
        let monitoring_task = self.start_monitoring_loop();
        
        // Start recovery coordination task
        let recovery_task = self.start_recovery_coordination();
        
        // Start graceful degradation monitoring
        let degradation_task = self.graceful_degradation.start_health_monitoring().await;

        // Store task handles
        let mut tasks = self.background_tasks.lock().await;
        tasks.push(monitoring_task);
        tasks.push(recovery_task);
        tasks.push(degradation_task);

        info!("VM health escalation manager started successfully");
        Ok(())
    }

    /// Stop the escalation manager
    pub async fn stop(&self) -> BlixardResult<()> {
        info!("Stopping VM health escalation manager");

        // Send shutdown signal
        let _ = self.shutdown_signal.send(());

        // Cancel all background tasks
        let tasks = {
            let mut tasks = self.background_tasks.lock().await;
            std::mem::take(&mut *tasks)
        };

        for task in tasks {
            task.abort();
        }

        // Cancel all active recovery operations
        {
            let mut recoveries = self.active_recoveries.write().await;
            for recovery in recoveries.values_mut() {
                recovery.status = RecoveryStatus::Cancelled;
            }
            recoveries.clear();
        }

        info!("VM health escalation manager stopped");
        Ok(())
    }

    /// Perform health check with escalation logic
    pub async fn perform_escalated_health_check(
        &self,
        vm_name: &str,
        checks: &[HealthCheck],
    ) -> BlixardResult<VmHealthStatus> {
        // Get or create escalation state for this VM
        let current_priority = {
            let mut states = self.escalation_states.write().await;
            let state = states.entry(vm_name.to_string()).or_insert_with(VmEscalationState::new);
            
            // Check if we should reset escalation
            if state.should_reset_escalation(self.config.reset_escalation_after) {
                state.reset_escalation();
                info!("Reset health check escalation for VM: {}", vm_name);
            }
            
            state.current_priority
        };

        // Filter checks by current priority level
        let applicable_checks: Vec<&HealthCheck> = checks
            .iter()
            .filter(|check| check.priority <= current_priority)
            .collect();

        if applicable_checks.is_empty() {
            warn!("No health checks applicable for VM {} at priority level {}", vm_name, current_priority.name());
            return Ok(VmHealthStatus::default());
        }

        let mut health_status = VmHealthStatus::default();
        let mut check_results = Vec::new();

        // Execute health checks with circuit breakers and retry logic
        for check in applicable_checks {
            let result = self.execute_health_check_with_resilience(vm_name, check).await?;
            check_results.push(result);
        }

        health_status.check_results = check_results;
        health_status.calculate_score();

        // Determine if escalation is needed
        if health_status.should_escalate_checks(current_priority) {
            self.escalate_health_monitoring(vm_name).await?;
        }

        // Check if recovery actions should be triggered
        for check in &checks {
            if health_status.should_trigger_recovery(&check.name, check.failure_threshold) {
                if let Some(ref escalation) = check.recovery_escalation {
                    self.trigger_recovery_escalation(vm_name, check, escalation).await?;
                }
            }
        }

        Ok(health_status)
    }

    /// Execute a single health check with circuit breaker and retry logic
    async fn execute_health_check_with_resilience(
        &self,
        vm_name: &str,
        check: &HealthCheck,
    ) -> BlixardResult<HealthCheckResult> {
        let check_id = format!("{}_{}", vm_name, check.name);
        
        // Check if monitoring is available (graceful degradation)
        if !self.graceful_degradation.is_service_available("health_check_execution").await {
            warn!("Health check execution degraded for VM {}, using fallback", vm_name);
            return Ok(self.create_fallback_result(check));
        }

        // Get or create circuit breaker for this check
        let circuit_breaker = {
            let mut states = self.escalation_states.write().await;
            let state = states.entry(vm_name.to_string()).or_insert_with(VmEscalationState::new);
            
            state.circuit_breakers
                .entry(check_id.clone())
                .or_insert_with(|| {
                    let config = CircuitBreakerConfig {
                        failure_threshold: 3,
                        reset_timeout: Duration::from_secs(60),
                        operation_timeout: Some(Duration::from_secs(
                            (check.check_type.timeout() as f32 * check.priority.timeout_multiplier()) as u64
                        )),
                        ..Default::default()
                    };
                    CircuitBreaker::new(check_id.clone(), config)
                })
                .clone()
        };

        // Configure retry logic based on check priority
        let retry_config = match check.priority {
            HealthCheckPriority::Quick => RetryConfig::for_network_operations(&format!("quick_check_{}", check.name)),
            HealthCheckPriority::Deep => RetryConfig::for_vm_operations(&format!("deep_check_{}", check.name)),
            HealthCheckPriority::Comprehensive => RetryConfig::for_critical_operations(&format!("comprehensive_check_{}", check.name)),
        }.with_logging(false); // Disable retry logging to avoid spam

        // Execute with circuit breaker and retry
        let start_time = Instant::now();
        
        let result = circuit_breaker.call(|| {
            Box::new(retry(retry_config.clone(), || {
                Box::pin(self.execute_single_health_check(vm_name, check))
            }))
        }).await;

        let duration = start_time.elapsed();

        match result {
            Ok(_) => {
                // Update escalation state on success
                {
                    let mut states = self.escalation_states.write().await;
                    if let Some(state) = states.get_mut(vm_name) {
                        state.last_successful_check = Some(Instant::now());
                    }
                }

                Ok(HealthCheckResult {
                    check_name: check.name.clone(),
                    success: true,
                    message: "Health check passed".to_string(),
                    duration_ms: duration.as_millis() as u64,
                    timestamp_secs: chrono::Utc::now().timestamp(),
                    error: None,
                    priority: check.priority,
                    critical: check.critical,
                    recovery_action: None,
                })
            }
            Err(e) => {
                error!("Health check failed for VM {} check {}: {}", vm_name, check.name, e);
                
                Ok(HealthCheckResult {
                    check_name: check.name.clone(),
                    success: false,
                    message: format!("Health check failed: {}", e),
                    duration_ms: duration.as_millis() as u64,
                    timestamp_secs: chrono::Utc::now().timestamp(),
                    error: Some(e.to_string()),
                    priority: check.priority,
                    critical: check.critical,
                    recovery_action: None,
                })
            }
        }
    }

    /// Create a fallback result when health checking is degraded
    fn create_fallback_result(&self, check: &HealthCheck) -> HealthCheckResult {
        HealthCheckResult {
            check_name: check.name.clone(),
            success: false,
            message: "Health check skipped due to system degradation".to_string(),
            duration_ms: 0,
            timestamp_secs: chrono::Utc::now().timestamp(),
            error: Some("Monitoring system degraded".to_string()),
            priority: check.priority,
            critical: check.critical,
            recovery_action: None,
        }
    }

    /// Execute the actual health check (placeholder - would be implemented based on check type)
    async fn execute_single_health_check(
        &self,
        vm_name: &str,
        check: &HealthCheck,
    ) -> BlixardResult<()> {
        // This is a placeholder - in a real implementation, this would:
        // 1. Execute HTTP/TCP/Script/Console checks based on check.check_type
        // 2. Validate VM state through vm_manager
        // 3. Return success/failure based on actual check results
        
        debug!("Executing {} health check for VM {}: {}", 
               check.priority.name(), vm_name, check.name);
               
        // Simulate check execution time
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Placeholder logic - randomly fail some checks for testing
        if check.name.contains("fail") {
            return Err(BlixardError::VmOperationFailed {
                operation: format!("health_check_{}", check.name),
                details: "Simulated health check failure".to_string(),
            });
        }
        
        Ok(())
    }

    /// Escalate health monitoring to the next priority level
    async fn escalate_health_monitoring(&self, vm_name: &str) -> BlixardResult<()> {
        let mut states = self.escalation_states.write().await;
        let state = states.entry(vm_name.to_string()).or_insert_with(VmEscalationState::new);

        let old_priority = state.current_priority;
        
        // Escalate to next level
        state.current_priority = match state.current_priority {
            HealthCheckPriority::Quick => HealthCheckPriority::Deep,
            HealthCheckPriority::Deep => HealthCheckPriority::Comprehensive,
            HealthCheckPriority::Comprehensive => HealthCheckPriority::Comprehensive, // Stay at highest
        };

        if state.current_priority != old_priority {
            state.escalation_count += 1;
            state.last_escalation = Some(Instant::now());
            
            warn!("Escalated health monitoring for VM {} from {} to {} (escalation #{})",
                  vm_name, old_priority.name(), state.current_priority.name(), state.escalation_count);
            
            // Check if we've exceeded maximum escalations
            if state.escalation_count >= self.config.max_escalations {
                error!("VM {} has exceeded maximum escalations ({}), triggering emergency procedures",
                       vm_name, self.config.max_escalations);
                       
                // Mark monitoring system as degraded
                self.graceful_degradation
                    .mark_service_unhealthy("vm_health_monitoring", 
                                          &format!("VM {} exceeded max escalations", vm_name)).await?;
            }
        }

        Ok(())
    }

    /// Trigger recovery escalation for a failing health check
    async fn trigger_recovery_escalation(
        &self,
        vm_name: &str,
        check: &HealthCheck,
        escalation: &RecoveryEscalation,
    ) -> BlixardResult<()> {
        // Check if recovery coordination is available
        if !self.graceful_degradation.is_service_available("recovery_coordination").await {
            warn!("Recovery coordination degraded, skipping recovery for VM {}", vm_name);
            return Ok(());
        }

        // Check if we're already at maximum concurrent recoveries
        {
            let recoveries = self.active_recoveries.read().await;
            if recoveries.len() >= self.config.max_concurrent_recoveries {
                warn!("Maximum concurrent recoveries reached, queuing recovery for VM {}", vm_name);
                return Ok(());
            }
        }

        // Find the appropriate recovery action based on current failure state
        let action = if let Some((action, _delay)) = escalation.actions.first() {
            action.clone()
        } else {
            warn!("No recovery actions configured for check {} on VM {}", check.name, vm_name);
            return Ok(());
        };

        let operation_id = format!("{}_{}_{}_{}", vm_name, check.name, 
                                   action.name(), chrono::Utc::now().timestamp());

        let recovery_op = RecoveryOperation {
            id: operation_id.clone(),
            vm_name: vm_name.to_string(),
            action: action.clone(),
            started_at: Instant::now(),
            timeout: self.config.recovery_timeout,
            status: RecoveryStatus::Running,
            error: None,
        };

        // Store the recovery operation
        {
            let mut recoveries = self.active_recoveries.write().await;
            recoveries.insert(operation_id.clone(), recovery_op);
        }

        info!("Triggered recovery action {:?} for VM {} check {}", action, vm_name, check.name);

        // Execute recovery action asynchronously
        let vm_manager = Arc::clone(&self.vm_manager);
        let recoveries = Arc::clone(&self.active_recoveries);
        let degradation = Arc::clone(&self.graceful_degradation);
        
        tokio::spawn(async move {
            let result = Self::execute_recovery_action(&vm_manager, vm_name, &action).await;
            
            // Update recovery operation status
            {
                let mut recoveries = recoveries.write().await;
                if let Some(recovery) = recoveries.get_mut(&operation_id) {
                    match result {
                        Ok(_) => {
                            recovery.status = RecoveryStatus::Success;
                            info!("Recovery action {:?} succeeded for VM {}", action, vm_name);
                        }
                        Err(e) => {
                            recovery.status = RecoveryStatus::Failed;
                            recovery.error = Some(e.to_string());
                            error!("Recovery action {:?} failed for VM {}: {}", action, vm_name, e);
                            
                            // Mark recovery service as degraded on failure
                            let _ = degradation.mark_service_unhealthy("recovery_coordination", 
                                                                    &format!("Recovery failed for VM {}", vm_name)).await;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Execute a specific recovery action
    async fn execute_recovery_action(
        vm_manager: &VmManager,
        vm_name: &str,
        action: &RecoveryAction,
    ) -> BlixardResult<()> {
        match action {
            RecoveryAction::None => {
                debug!("No-op recovery action for VM {}", vm_name);
                Ok(())
            }
            RecoveryAction::RestartVm => {
                info!("Restarting VM {}", vm_name);
                vm_manager.stop_vm(vm_name).await?;
                tokio::time::sleep(Duration::from_secs(5)).await;
                vm_manager.start_vm(vm_name).await?;
                Ok(())
            }
            RecoveryAction::StopVm => {
                warn!("Stopping VM {} as recovery action", vm_name);
                vm_manager.stop_vm(vm_name).await?;
                Ok(())
            }
            RecoveryAction::RestartService { service_name } => {
                info!("Restarting service {} on VM {}", service_name, vm_name);
                // This would integrate with VM guest agent to restart specific services
                // For now, return success as placeholder
                Ok(())
            }
            RecoveryAction::MigrateVm { target_node: _ } => {
                info!("Migrating VM {} (not implemented)", vm_name);
                // This would integrate with VM migration system
                Err(BlixardError::NotImplemented {
                    feature: "VM migration".to_string(),
                })
            }
            RecoveryAction::CustomScript { script_path, args } => {
                info!("Executing custom recovery script {} for VM {}", script_path, vm_name);
                // This would execute custom recovery scripts
                // For now, return success as placeholder
                debug!("Custom script args: {:?}", args);
                Ok(())
            }
        }
    }

    /// Start the background monitoring loop
    fn start_monitoring_loop(&self) -> JoinHandle<()> {
        let escalation_states = Arc::clone(&self.escalation_states);
        let quick_interval = self.config.quick_check_interval;
        let deep_interval = self.config.deep_check_interval;
        let comprehensive_interval = self.config.comprehensive_check_interval;
        let mut shutdown_rx = self.shutdown_signal.subscribe();

        tokio::spawn(async move {
            let mut quick_timer = tokio::time::interval(quick_interval);
            let mut deep_timer = tokio::time::interval(deep_interval);
            let mut comprehensive_timer = tokio::time::interval(comprehensive_interval);

            loop {
                tokio::select! {
                    _ = quick_timer.tick() => {
                        debug!("Quick health check timer triggered");
                        // Quick health checks would be triggered here
                    }
                    _ = deep_timer.tick() => {
                        debug!("Deep health check timer triggered");
                        // Deep health checks would be triggered here
                    }
                    _ = comprehensive_timer.tick() => {
                        debug!("Comprehensive health check timer triggered");
                        // Comprehensive health checks would be triggered here
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Health monitoring loop shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Start recovery coordination background task
    fn start_recovery_coordination(&self) -> JoinHandle<()> {
        let active_recoveries = Arc::clone(&self.active_recoveries);
        let recovery_timeout = self.config.recovery_timeout;
        let mut shutdown_rx = self.shutdown_signal.subscribe();

        tokio::spawn(async move {
            let mut cleanup_timer = tokio::time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = cleanup_timer.tick() => {
                        // Clean up completed and timed out recovery operations
                        let mut recoveries = active_recoveries.write().await;
                        let mut to_remove = Vec::new();

                        for (id, recovery) in recoveries.iter_mut() {
                            if recovery.status == RecoveryStatus::Running && 
                               recovery.started_at.elapsed() > recovery_timeout {
                                recovery.status = RecoveryStatus::TimedOut;
                                warn!("Recovery operation {} timed out", id);
                            }

                            if recovery.status != RecoveryStatus::Running {
                                to_remove.push(id.clone());
                            }
                        }

                        for id in to_remove {
                            recoveries.remove(&id);
                            debug!("Cleaned up recovery operation {}", id);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Recovery coordination shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Get current escalation status for all VMs
    pub async fn get_escalation_status(&self) -> HashMap<String, (HealthCheckPriority, u32)> {
        let states = self.escalation_states.read().await;
        states
            .iter()
            .map(|(vm_name, state)| {
                (vm_name.clone(), (state.current_priority, state.escalation_count))
            })
            .collect()
    }

    /// Get current active recovery operations
    pub async fn get_active_recoveries(&self) -> Vec<RecoveryOperation> {
        let recoveries = self.active_recoveries.read().await;
        recoveries.values().cloned().collect()
    }

    /// Get graceful degradation status
    pub async fn get_degradation_status(&self) -> DegradationStats {
        self.graceful_degradation.stats().await
    }
}

// Helper trait for getting timeout from health check types
trait HealthCheckTypeExt {
    fn timeout(&self) -> u64;
}

impl HealthCheckTypeExt for crate::vm_health_types::HealthCheckType {
    fn timeout(&self) -> u64 {
        match self {
            crate::vm_health_types::HealthCheckType::Http { timeout_secs, .. } => *timeout_secs,
            crate::vm_health_types::HealthCheckType::Tcp { timeout_secs, .. } => *timeout_secs,
            crate::vm_health_types::HealthCheckType::Script { timeout_secs, .. } => *timeout_secs,
            crate::vm_health_types::HealthCheckType::Console { timeout_secs, .. } => *timeout_secs,
            crate::vm_health_types::HealthCheckType::Process { .. } => 5, // Default timeout
            crate::vm_health_types::HealthCheckType::GuestAgent { timeout_secs, .. } => *timeout_secs,
        }
    }
}

// Helper for recovery action names
impl RecoveryAction {
    fn name(&self) -> &'static str {
        match self {
            RecoveryAction::None => "none",
            RecoveryAction::RestartService { .. } => "restart_service",
            RecoveryAction::RestartVm => "restart_vm",
            RecoveryAction::MigrateVm { .. } => "migrate_vm",
            RecoveryAction::StopVm => "stop_vm",
            RecoveryAction::CustomScript { .. } => "custom_script",
        }
    }
}