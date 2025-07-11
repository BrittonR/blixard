//! Health check scheduler component
//!
//! This module provides the HealthCheckScheduler component that manages
//! periodic health checks for VMs using the LifecycleManager pattern.

use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info, warn};
use async_trait::async_trait;

use crate::{
    abstractions::time::Clock,
    error::{BlixardError, BlixardResult},
    patterns::LifecycleManager,
    types::VmStatus,
    vm_auto_recovery::VmAutoRecovery,
    vm_health_config::{HealthCheckSchedulerConfig, VmHealthMonitorDependencies},
    vm_health_types::{HealthState, VmHealthStatus},
};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, metrics};

/// Component responsible for scheduling and executing health checks
pub struct HealthCheckScheduler {
    config: HealthCheckSchedulerConfig,
    deps: VmHealthMonitorDependencies,
    auto_recovery: Arc<VmAutoRecovery>,
    handle: Option<JoinHandle<()>>,
    is_running: bool,
}

impl std::fmt::Debug for HealthCheckScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthCheckScheduler")
            .field("config", &self.config)
            .field("deps", &self.deps)
            .field("auto_recovery", &self.auto_recovery)
            .field("handle", &self.handle.as_ref().map(|_| "<JoinHandle>"))
            .field("is_running", &self.is_running)
            .finish()
    }
}

impl HealthCheckScheduler {
    /// Create new health check scheduler
    pub fn new(
        config: HealthCheckSchedulerConfig,
        deps: VmHealthMonitorDependencies,
        auto_recovery: Arc<VmAutoRecovery>,
    ) -> Self {
        Self {
            config,
            deps,
            auto_recovery,
            handle: None,
            is_running: false,
        }
    }

    /// Execute a single health check cycle
    async fn run_health_check_cycle(&self) -> BlixardResult<()> {
        let metrics = metrics();
        let node_id = self.deps.node_state.get_id();

        // Get all VMs from the database (source of truth)
        let vms = self.deps.vm_manager.list_vms().await?;

        let mut checked_count = 0;
        let mut unhealthy_count = 0;

        for (vm_config, stored_status) in vms {
            // Only check VMs that are assigned to this node
            let vm_node_id = match self.get_vm_node_id(&vm_config.name).await {
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

            // Check if we should perform health checks based on configuration
            if !self.config.enable_process_checks && !self.config.enable_detailed_checks {
                continue;
            }

            // Get VM process status
            let process_status = match self.deps.vm_manager.get_vm_status(&vm_config.name).await? {
                Some((_, status)) => status,
                None => {
                    warn!(
                        "VM '{}' not found in backend during health check",
                        vm_config.name
                    );
                    VmStatus::Failed
                }
            };

            // If process checks are enabled and process is not running, handle the failure
            if self.config.enable_process_checks && process_status != VmStatus::Running {
                if stored_status == VmStatus::Running {
                    info!("VM '{}' process is no longer running", vm_config.name);

                    // Update status through Raft
                    if let Err(e) = self.deps.node_state
                        .update_vm_status_through_raft(
                            &vm_config.name,
                            process_status,
                        )
                        .await
                    {
                        error!("Failed to update VM '{}' status: {}", vm_config.name, e);
                    }

                    // Trigger auto-recovery
                    if let Err(e) = self.auto_recovery
                        .trigger_recovery(&vm_config.name, &vm_config)
                        .await
                    {
                        error!("Auto-recovery failed for VM '{}': {}", vm_config.name, e);
                    }
                }
                continue;
            }

            // Perform detailed health checks (if enabled and VM is running)
            if self.config.enable_detailed_checks && process_status == VmStatus::Running {
                match self.perform_vm_health_checks(&vm_config.name, &vm_config).await {
                    Ok(health_status) => {
                        // Record metrics based on health state
                        self.record_health_metrics(&vm_config.name, &health_status);

                        // Handle unhealthy VMs
                        if matches!(health_status.state, HealthState::Unhealthy | HealthState::Unresponsive) {
                            unhealthy_count += 1;

                            // Trigger auto-recovery for unhealthy VMs
                            if health_status.consecutive_failures >= 3 {
                                info!("VM '{}' is unhealthy, triggering recovery", vm_config.name);
                                if let Err(e) = self.auto_recovery
                                    .trigger_recovery(&vm_config.name, &vm_config)
                                    .await
                                {
                                    error!(
                                        "Auto-recovery failed for VM '{}': {}",
                                        vm_config.name, e
                                    );
                                }
                            }
                        }

                        // Store health status in backend
                        if let Err(e) = self.deps.vm_manager
                            .backend()
                            .update_vm_health_status(&vm_config.name, health_status)
                            .await
                        {
                            warn!(
                                "Failed to update health status for VM '{}': {}",
                                vm_config.name, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to perform health checks for VM '{}': {}",
                            vm_config.name, e
                        );
                        metrics.vm_health_check_failed.add(
                            1,
                            &[
                                attributes::vm_name(&vm_config.name),
                                attributes::error(true),
                            ],
                        );
                    }
                }
            }
        }

        // Record overall metrics
        metrics
            .vm_health_checks_total
            .add(checked_count, &[attributes::node_id(node_id)]);

        if unhealthy_count > 0 {
            metrics
                .vm_unhealthy_total
                .add(unhealthy_count, &[attributes::node_id(node_id)]);
        }

        Ok(())
    }

    /// Get the node ID that owns a specific VM
    async fn get_vm_node_id(&self, vm_name: &str) -> BlixardResult<Option<u64>> {
        let database = self.deps.node_state
            .get_database()
            .await
            .ok_or_else(|| BlixardError::Internal {
                message: "Database not initialized".to_string(),
            })?;

        let read_txn = database.begin_read()?;
        let table = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE)?;

        if let Some(data) = table.get(vm_name)? {
            let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
            Ok(Some(vm_state.node_id))
        } else {
            Ok(None)
        }
    }

    /// Perform health checks for a specific VM
    async fn perform_vm_health_checks(
        &self,
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

        // Perform each configured health check with timeout
        for check in &health_config.checks {
            let start_time = self.deps.clock.now();

            let result = tokio::time::timeout(
                self.config.health_check_timeout,
                self.deps.vm_manager
                    .backend()
                    .perform_health_check(vm_name, &check.name, &check.check_type)
            ).await;

            let check_result = match result {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    // Health check failed
                    crate::vm_health_types::HealthCheckResult {
                        check_name: check.name.clone(),
                        success: false,
                        message: format!("Health check failed: {}", e),
                        duration_ms: self.deps.clock.now().duration_since(start_time).as_millis() as u64,
                        timestamp_secs: self.deps.clock.now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or(Duration::from_secs(0))
                            .as_secs() as i64,
                        error: Some(e.to_string()),
                    }
                }
                Err(_timeout) => {
                    // Health check timed out
                    crate::vm_health_types::HealthCheckResult {
                        check_name: check.name.clone(),
                        success: false,
                        message: format!("Health check timed out after {:?}", self.config.health_check_timeout),
                        duration_ms: self.config.health_check_timeout.as_millis() as u64,
                        timestamp_secs: self.deps.clock.now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or(Duration::from_secs(0))
                            .as_secs() as i64,
                        error: Some("Timeout".to_string()),
                    }
                }
            };

            health_status.check_results.push(check_result);
        }

        // Calculate overall health score and state
        health_status.calculate_score();
        health_status.update_state(health_config);

        Ok(health_status)
    }

    /// Record health metrics for a VM
    fn record_health_metrics(&self, vm_name: &str, health_status: &VmHealthStatus) {
        let metrics = metrics();

        match health_status.state {
            HealthState::Healthy => {
                metrics.vm_health_state.add(
                    1,
                    &[
                        attributes::vm_name(vm_name),
                        attributes::health_state("healthy"),
                    ],
                );
            }
            HealthState::Degraded => {
                metrics.vm_health_state.add(
                    1,
                    &[
                        attributes::vm_name(vm_name),
                        attributes::health_state("degraded"),
                    ],
                );
            }
            HealthState::Unhealthy | HealthState::Unresponsive => {
                metrics.vm_health_state.add(
                    1,
                    &[
                        attributes::vm_name(vm_name),
                        attributes::health_state("unhealthy"),
                    ],
                );
            }
            HealthState::Unknown => {
                // No metrics for unknown state
            }
        }
    }
}

#[async_trait]
impl LifecycleManager for HealthCheckScheduler {
    async fn start(&mut self) -> BlixardResult<()> {
        if self.is_running {
            warn!("HealthCheckScheduler is already running");
            return Ok(());
        }

        let config = self.config.clone();
        let deps = self.deps.clone();
        let auto_recovery = Arc::clone(&self.auto_recovery);

        let handle = tokio::spawn(async move {
            info!(
                "Starting VM health check scheduler with interval: {:?}",
                config.check_interval
            );

            let scheduler = HealthCheckScheduler::new(config.clone(), deps, auto_recovery);
            let mut interval = interval(config.check_interval);

            loop {
                interval.tick().await;

                if let Err(e) = scheduler.run_health_check_cycle().await {
                    error!("Health check cycle failed: {}", e);
                }
            }
        });

        self.handle = Some(handle);
        self.is_running = true;

        info!("HealthCheckScheduler started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            self.is_running = false;
            info!("HealthCheckScheduler stopped");
        }
        Ok(())
    }

    async fn restart(&mut self) -> BlixardResult<()> {
        self.stop().await?;
        self.start().await
    }

    fn is_running(&self) -> bool {
        self.is_running
    }

    fn name(&self) -> &'static str {
        "HealthCheckScheduler"
    }
}

impl Drop for HealthCheckScheduler {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::time::MockClock;
    use crate::vm_auto_recovery::RecoveryPolicy;
    use std::time::SystemTime;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_health_check_scheduler_lifecycle() {
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

        let node_state = Arc::new(crate::node_shared::SharedNodeState::new(config));
        let database = Arc::new(redb::Database::create(temp_dir.path().join("test.db")).unwrap());
        node_state.set_database(database.clone()).await;

        let vm_backend = Arc::new(crate::vm_backend::MockVmBackend::new(database.clone()));
        let vm_manager = Arc::new(crate::vm_backend::VmManager::new(
            database,
            vm_backend,
            node_state.clone(),
        ));

        let clock = Arc::new(MockClock::new());

        let deps = VmHealthMonitorDependencies::with_clock(
            node_state,
            vm_manager,
            clock,
        );

        let recovery_policy = RecoveryPolicy::default();
        let auto_recovery = Arc::new(VmAutoRecovery::new(deps.node_state.clone(), recovery_policy));

        let scheduler_config = HealthCheckSchedulerConfig {
            check_interval: Duration::from_millis(100),
            max_concurrent_checks: 5,
            health_check_timeout: Duration::from_secs(1),
            enable_process_checks: true,
            enable_detailed_checks: true,
        };

        let mut scheduler = HealthCheckScheduler::new(scheduler_config, deps, auto_recovery);

        // Test lifecycle
        assert!(!scheduler.is_running());
        assert_eq!(scheduler.name(), "HealthCheckScheduler");

        // Start should succeed
        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());

        // Starting again should not fail
        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());

        // Stop should succeed
        scheduler.stop().await.unwrap();
        assert!(!scheduler.is_running());

        // Restart should work
        scheduler.restart().await.unwrap();
        assert!(scheduler.is_running());

        scheduler.stop().await.unwrap();
    }
}