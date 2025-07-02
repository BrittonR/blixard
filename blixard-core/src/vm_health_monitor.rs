use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio::task::JoinHandle;
use tracing::{info, warn, error};

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    vm_backend::VmManager,
    vm_auto_recovery::{VmAutoRecovery, RecoveryPolicy},
    types::VmStatus,
    vm_health_types::{VmHealthStatus, HealthState, HealthCheckResult, HealthCheck, VmHealthCheckConfig},
    metrics_otel::{metrics, attributes},
};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Health monitor for VM processes
/// 
/// This component periodically checks the health of all VMs on this node
/// and updates their status through Raft consensus if changes are detected.
/// 
/// ## Features
/// 
/// - Periodic health checks for all local VMs
/// - Automatic status updates through Raft consensus
/// - Configurable check intervals
/// - Metrics for monitoring health check performance
/// - Auto-recovery trigger for failed VMs
pub struct VmHealthMonitor {
    node_state: Arc<SharedNodeState>,
    vm_manager: Arc<VmManager>,
    check_interval: Duration,
    auto_recovery: Arc<VmAutoRecovery>,
    handle: Option<JoinHandle<()>>,
    /// Per-VM health check configurations
    health_configs: Arc<RwLock<HashMap<String, VmHealthCheckConfig>>>,
    /// Per-VM health status tracking
    health_statuses: Arc<RwLock<HashMap<String, VmHealthStatus>>>,
    /// Health monitoring enabled status per VM
    monitoring_enabled: Arc<RwLock<HashMap<String, bool>>>,
}

impl VmHealthMonitor {
    /// Create a new VM health monitor
    pub fn new(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        check_interval: Duration,
    ) -> Self {
        // Create auto-recovery with default policy
        let recovery_policy = RecoveryPolicy::default();
        let auto_recovery = Arc::new(VmAutoRecovery::new(node_state.clone(), recovery_policy));
        
        Self {
            node_state,
            vm_manager,
            check_interval,
            auto_recovery,
            handle: None,
            health_configs: Arc::new(RwLock::new(HashMap::new())),
            health_statuses: Arc::new(RwLock::new(HashMap::new())),
            monitoring_enabled: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create a new VM health monitor with custom recovery policy
    pub fn with_recovery_policy(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        check_interval: Duration,
        recovery_policy: RecoveryPolicy,
    ) -> Self {
        let auto_recovery = Arc::new(VmAutoRecovery::new(node_state.clone(), recovery_policy));
        
        Self {
            node_state,
            vm_manager,
            check_interval,
            auto_recovery,
            handle: None,
            health_configs: Arc::new(RwLock::new(HashMap::new())),
            health_statuses: Arc::new(RwLock::new(HashMap::new())),
            monitoring_enabled: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Start the health monitoring task
    pub fn start(&mut self) {
        if self.handle.is_some() {
            warn!("VM health monitor is already running");
            return;
        }
        
        let node_state = Arc::clone(&self.node_state);
        let vm_manager = Arc::clone(&self.vm_manager);
        let auto_recovery = Arc::clone(&self.auto_recovery);
        let check_interval = self.check_interval;
        
        let handle = tokio::spawn(async move {
            info!("Starting VM health monitor with interval: {:?}", check_interval);
            let mut interval = interval(check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::run_health_checks_v2(&node_state, &vm_manager, &auto_recovery).await {
                    error!("Health check cycle failed: {}", e);
                }
            }
        });
        
        self.handle = Some(handle);
    }
    
    /// Stop the health monitoring task
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            info!("VM health monitor stopped");
        }
    }
    
    /// Run health checks for all VMs on this node
    async fn run_health_checks(
        node_state: &Arc<SharedNodeState>,
        vm_manager: &Arc<VmManager>,
        auto_recovery: &Arc<VmAutoRecovery>,
    ) -> BlixardResult<()> {
        let metrics = metrics();
        let node_id = node_state.get_id();
        
        // Get all VMs from the database (source of truth)
        let vms = vm_manager.list_vms().await?;
        
        let mut checked_count = 0;
        let mut status_changes = 0;
        
        for (vm_config, stored_status) in vms {
            // Only check VMs that are assigned to this node
            let vm_node_id = match Self::get_vm_node_id(node_state, &vm_config.name).await {
                Ok(Some(id)) => id,
                Ok(None) => continue, // VM not found, skip
                Err(e) => {
                    warn!("Failed to get node ID for VM '{}': {}", vm_config.name, e);
                    continue;
                }
            };
            
            if vm_node_id != node_id {
                continue; // VM is on another node
            }
            
            checked_count += 1;
            
            // Get actual status from the VM backend
            let actual_status = match vm_manager.get_vm_status(&vm_config.name).await? {
                Some((_, status)) => status,
                None => {
                    warn!("VM '{}' not found in backend during health check", vm_config.name);
                    VmStatus::Failed
                }
            };
            
            // Check if status has changed
            if actual_status != stored_status {
                info!(
                    "VM '{}' status changed: {:?} -> {:?}", 
                    vm_config.name, stored_status, actual_status
                );
                
                status_changes += 1;
                
                // Update status through Raft consensus
                if let Err(e) = node_state.update_vm_status_through_raft(
                    vm_config.name.clone(),
                    actual_status,
                    node_id
                ).await {
                    error!("Failed to update VM '{}' status: {}", vm_config.name, e);
                    metrics.vm_health_check_failed.add(1, &[
                        attributes::vm_name(&vm_config.name),
                        attributes::error(true),
                    ]);
                }
                
                // Trigger auto-recovery if VM has failed
                if actual_status == VmStatus::Failed && stored_status == VmStatus::Running {
                    info!("Triggering auto-recovery for failed VM '{}'", vm_config.name);
                    if let Err(e) = auto_recovery.trigger_recovery(&vm_config.name, &vm_config).await {
                        error!("Auto-recovery failed for VM '{}': {}", vm_config.name, e);
                    }
                }
            }
        }
        
        // Record metrics
        metrics.vm_health_checks_total.add(checked_count, &[
            attributes::node_id(node_id),
        ]);
        
        if status_changes > 0 {
            metrics.vm_status_changes.add(status_changes, &[
                attributes::node_id(node_id),
            ]);
        }
        
        Ok(())
    }
    
    /// Get the node ID that owns a specific VM
    async fn get_vm_node_id(
        node_state: &Arc<SharedNodeState>,
        vm_name: &str,
    ) -> BlixardResult<Option<u64>> {
        let database = node_state.get_database().await
            .ok_or_else(|| BlixardError::Internal {
                message: "Database not initialized".to_string(),
            })?;
        
        let read_txn = database.begin_read()?;
        let table = read_txn.open_table(crate::storage::VM_STATE_TABLE)?;
        
        if let Some(data) = table.get(vm_name)? {
            let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
            Ok(Some(vm_state.node_id))
        } else {
            Ok(None)
        }
    }
    
    /// Perform health checks for a specific VM
    async fn perform_vm_health_checks(
        vm_manager: &Arc<VmManager>,
        vm_name: &str,
        vm_config: &crate::types::VmConfig,
    ) -> BlixardResult<VmHealthStatus> {
        let mut health_status = VmHealthStatus::default();
        
        // If no health check config, return unknown state
        let health_config = match &vm_config.health_check_config {
            Some(config) => config,
            None => {
                health_status.state = HealthState::Unknown;
                return Ok(health_status);
            }
        };
        
        // Perform each configured health check
        for check in &health_config.checks {
            let start_time = std::time::Instant::now();
            
            let result = match vm_manager.backend().perform_health_check(
                vm_name,
                &check.name,
                &check.check_type,
            ).await {
                Ok(result) => result,
                Err(e) => {
                    // Create a failed result for the check
                    HealthCheckResult {
                        check_name: check.name.clone(),
                        success: false,
                        message: format!("Health check failed: {}", e),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        timestamp_secs: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        error: Some(e.to_string()),
                    }
                }
            };
            
            health_status.check_results.push(result);
        }
        
        // Calculate overall health score and state
        health_status.calculate_score();
        health_status.update_state(health_config);
        
        Ok(health_status)
    }
    
    /// Update the run_health_checks to perform actual health checks
    async fn run_health_checks_v2(
        node_state: &Arc<SharedNodeState>,
        vm_manager: &Arc<VmManager>,
        auto_recovery: &Arc<VmAutoRecovery>,
    ) -> BlixardResult<()> {
        let metrics = metrics();
        let node_id = node_state.get_id();
        
        // Get all VMs from the database (source of truth)
        let vms = vm_manager.list_vms().await?;
        
        let mut checked_count = 0;
        let mut unhealthy_count = 0;
        
        for (vm_config, stored_status) in vms {
            // Only check VMs that are assigned to this node
            let vm_node_id = match Self::get_vm_node_id(node_state, &vm_config.name).await {
                Ok(Some(id)) => id,
                Ok(None) => continue,
                Err(e) => {
                    warn!("Failed to get node ID for VM '{}': {}", vm_config.name, e);
                    continue;
                }
            };
            
            if vm_node_id != node_id {
                continue;
            }
            
            checked_count += 1;
            
            // First check if VM process is running
            let process_status = match vm_manager.get_vm_status(&vm_config.name).await? {
                Some((_, status)) => status,
                None => {
                    warn!("VM '{}' not found in backend during health check", vm_config.name);
                    VmStatus::Failed
                }
            };
            
            // If process is not running, skip health checks
            if process_status != VmStatus::Running {
                if stored_status == VmStatus::Running {
                    // VM has stopped unexpectedly
                    info!("VM '{}' process is no longer running", vm_config.name);
                    
                    // Update status through Raft
                    if let Err(e) = node_state.update_vm_status_through_raft(
                        vm_config.name.clone(),
                        process_status,
                        node_id
                    ).await {
                        error!("Failed to update VM '{}' status: {}", vm_config.name, e);
                    }
                    
                    // Trigger auto-recovery
                    if let Err(e) = auto_recovery.trigger_recovery(&vm_config.name, &vm_config).await {
                        error!("Auto-recovery failed for VM '{}': {}", vm_config.name, e);
                    }
                }
                continue;
            }
            
            // Perform actual health checks
            match Self::perform_vm_health_checks(vm_manager, &vm_config.name, &vm_config).await {
                Ok(health_status) => {
                    // Record metrics based on health state
                    match health_status.state {
                        HealthState::Healthy => {
                            metrics.vm_health_state.add(1, &[
                                attributes::vm_name(&vm_config.name),
                                attributes::health_state("healthy"),
                            ]);
                        }
                        HealthState::Degraded => {
                            metrics.vm_health_state.add(1, &[
                                attributes::vm_name(&vm_config.name),
                                attributes::health_state("degraded"),
                            ]);
                        }
                        HealthState::Unhealthy | HealthState::Unresponsive => {
                            unhealthy_count += 1;
                            metrics.vm_health_state.add(1, &[
                                attributes::vm_name(&vm_config.name),
                                attributes::health_state("unhealthy"),
                            ]);
                            
                            // Trigger auto-recovery for unhealthy VMs
                            if health_status.consecutive_failures >= 3 {
                                info!("VM '{}' is unhealthy, triggering recovery", vm_config.name);
                                if let Err(e) = auto_recovery.trigger_recovery(&vm_config.name, &vm_config).await {
                                    error!("Auto-recovery failed for VM '{}': {}", vm_config.name, e);
                                }
                            }
                        }
                        HealthState::Unknown => {
                            // No health checks configured
                        }
                    }
                    
                    // Store health status in backend
                    if let Err(e) = vm_manager.backend().update_vm_health_status(&vm_config.name, health_status).await {
                        warn!("Failed to update health status for VM '{}': {}", vm_config.name, e);
                    }
                }
                Err(e) => {
                    error!("Failed to perform health checks for VM '{}': {}", vm_config.name, e);
                    metrics.vm_health_check_failed.add(1, &[
                        attributes::vm_name(&vm_config.name),
                        attributes::error(true),
                    ]);
                }
            }
        }
        
        // Record overall metrics
        metrics.vm_health_checks_total.add(checked_count, &[
            attributes::node_id(node_id),
        ]);
        
        if unhealthy_count > 0 {
            metrics.vm_unhealthy_total.add(unhealthy_count, &[
                attributes::node_id(node_id),
            ]);
        }
        
        Ok(())
    }
    
    // Public health monitoring methods for VM health configuration
    
    /// Get health status for a specific VM
    pub async fn get_health_status(&self, vm_name: &str) -> Option<VmHealthStatus> {
        self.health_statuses.read().await.get(vm_name).cloned()
    }
    
    /// Add a health check to a VM
    pub async fn add_health_check(&self, vm_name: &str, health_check: HealthCheck) -> BlixardResult<()> {
        let mut configs = self.health_configs.write().await;
        let config = configs.entry(vm_name.to_string()).or_insert_with(VmHealthCheckConfig::default);
        
        // Check if a health check with the same name already exists
        if config.checks.iter().any(|c| c.name == health_check.name) {
            return Err(BlixardError::ServiceAlreadyExists(format!("Health check '{}' already exists for VM '{}'", health_check.name, vm_name)));
        }
        
        config.checks.push(health_check);
        
        // Enable monitoring for this VM by default
        self.monitoring_enabled.write().await.insert(vm_name.to_string(), true);
        
        info!("Added health check to VM '{}'", vm_name);
        Ok(())
    }
    
    /// List health checks for a VM
    pub async fn list_health_checks(&self, vm_name: &str) -> BlixardResult<Vec<HealthCheck>> {
        let configs = self.health_configs.read().await;
        Ok(configs.get(vm_name).map(|c| c.checks.clone()).unwrap_or_default())
    }
    
    /// Remove a health check from a VM
    pub async fn remove_health_check(&self, vm_name: &str, check_name: &str) -> BlixardResult<()> {
        let mut configs = self.health_configs.write().await;
        if let Some(config) = configs.get_mut(vm_name) {
            config.checks.retain(|c| c.name != check_name);
            info!("Removed health check '{}' from VM '{}'", check_name, vm_name);
            Ok(())
        } else {
            Err(BlixardError::NotFound { resource: format!("VM '{}' has no health checks configured", vm_name) })
        }
    }
    
    /// Toggle health monitoring for a VM
    pub async fn toggle_monitoring(&self, vm_name: &str, enable: bool) -> BlixardResult<()> {
        self.monitoring_enabled.write().await.insert(vm_name.to_string(), enable);
        info!("Health monitoring {} for VM '{}'", if enable { "enabled" } else { "disabled" }, vm_name);
        Ok(())
    }
}

impl Drop for VmHealthMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_health_monitor_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::types::NodeConfig {
            id: 1,
            bind_addr: "127.0.0.1:7001".parse().unwrap(),
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: Default::default(),
        };
        
        let node_state = Arc::new(SharedNodeState::new(config));
        let database = Arc::new(redb::Database::create(temp_dir.path().join("test.db")).unwrap());
        node_state.set_database(database.clone()).await;
        
        let vm_backend = Arc::new(crate::vm_backend::MockVmBackend::new(database.clone()));
        let vm_manager = Arc::new(crate::vm_backend::VmManager::new(database, vm_backend, node_state.clone()));
        
        let mut monitor = VmHealthMonitor::new(
            node_state,
            vm_manager,
            Duration::from_secs(5),
        );
        
        // Start should succeed
        monitor.start();
        
        // Starting again should log a warning but not panic
        monitor.start();
        
        // Stop should succeed
        monitor.stop();
        
        // Stopping again should be safe
        monitor.stop();
    }
}