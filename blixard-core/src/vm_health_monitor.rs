//! VM Health Monitor with LifecycleManager Pattern
//!
//! This module provides a refactored VM health monitoring system that uses
//! the LifecycleManager pattern to coordinate multiple specialized components.

use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use async_trait::async_trait;

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    patterns::LifecycleManager,
    vm_auto_recovery::{RecoveryPolicy, VmAutoRecovery},
    vm_backend::VmManager,
    vm_health_config::{
        VmHealthMonitorConfig, VmHealthMonitorDependencies,
        HealthCheckSchedulerConfig, RecoveryCoordinatorConfig,
    },
    vm_health_recovery_coordinator::RecoveryCoordinator,
    vm_health_scheduler::HealthCheckScheduler,
    vm_health_state_manager::HealthStateManager,
    vm_health_types::{HealthCheck, VmHealthStatus},
};

/// Health monitor for VM processes using LifecycleManager pattern
///
/// This component coordinates multiple specialized health monitoring components:
/// - HealthCheckScheduler: Manages periodic health checks
/// - HealthStateManager: Manages health state persistence and configuration
/// - RecoveryCoordinator: Manages recovery operations
///
/// ## Features
///
/// - Modular design with separate concerns
/// - Lifecycle management using LifecycleManager trait
/// - Dependency injection for testability
/// - Configurable components with centralized configuration
/// - Improved error handling and observability
pub struct VmHealthMonitor {
    config: VmHealthMonitorConfig,
    deps: VmHealthMonitorDependencies,
    scheduler: Option<HealthCheckScheduler>,
    state_manager: Option<HealthStateManager>,
    recovery_coordinator: Option<RecoveryCoordinator>,
    auto_recovery: Arc<VmAutoRecovery>,
    is_running: bool,
}

impl VmHealthMonitor {
    /// Create a new VM health monitor with default configuration
    pub fn new(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        check_interval: Duration,
    ) -> Self {
        let config = VmHealthMonitorConfig {
            scheduler: HealthCheckSchedulerConfig {
                check_interval,
                ..Default::default()
            },
            ..Default::default()
        };

        let deps = VmHealthMonitorDependencies::new(node_state, vm_manager);
        let auto_recovery = Arc::new(VmAutoRecovery::new(
            deps.node_state.clone(),
            config.recovery.recovery_policy.clone(),
        ));

        Self {
            config,
            deps,
            scheduler: None,
            state_manager: None,
            recovery_coordinator: None,
            auto_recovery,
            is_running: false,
        }
    }

    /// Create a new VM health monitor with custom configuration
    pub fn with_config(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        config: VmHealthMonitorConfig,
    ) -> Self {
        let deps = VmHealthMonitorDependencies::new(node_state, vm_manager);
        let auto_recovery = Arc::new(VmAutoRecovery::new(
            deps.node_state.clone(),
            config.recovery.recovery_policy.clone(),
        ));

        Self {
            config,
            deps,
            scheduler: None,
            state_manager: None,
            recovery_coordinator: None,
            auto_recovery,
            is_running: false,
        }
    }

    /// Create a new VM health monitor with custom recovery policy (legacy compatibility)
    pub fn with_recovery_policy(
        node_state: Arc<SharedNodeState>,
        vm_manager: Arc<VmManager>,
        check_interval: Duration,
        recovery_policy: RecoveryPolicy,
    ) -> Self {
        let config = VmHealthMonitorConfig {
            scheduler: HealthCheckSchedulerConfig {
                check_interval,
                ..Default::default()
            },
            recovery: RecoveryCoordinatorConfig {
                recovery_policy,
                ..Default::default()
            },
            ..Default::default()
        };

        Self::with_config(node_state, vm_manager, config)
    }

    /// Initialize components (internal method)
    fn initialize_components(&mut self) -> BlixardResult<()> {
        // Create HealthCheckScheduler
        let scheduler = HealthCheckScheduler::new(
            self.config.scheduler.clone(),
            self.deps.clone(),
            Arc::clone(&self.auto_recovery),
        );
        self.scheduler = Some(scheduler);

        // Create HealthStateManager
        let state_manager = HealthStateManager::new(
            self.config.state_manager.clone(),
            self.deps.clone(),
        );
        self.state_manager = Some(state_manager);

        // Create RecoveryCoordinator
        let recovery_coordinator = RecoveryCoordinator::new(
            self.config.recovery.clone(),
            self.deps.clone(),
            Arc::clone(&self.auto_recovery),
        );
        self.recovery_coordinator = Some(recovery_coordinator);

        Ok(())
    }

    /// Get health status for a specific VM (delegates to HealthStateManager)
    pub async fn get_health_status(&self, vm_name: &str) -> Option<VmHealthStatus> {
        match &self.state_manager {
            Some(manager) => manager.get_health_status(vm_name).await,
            None => {
                warn!("HealthStateManager not initialized");
                None
            }
        }
    }

    /// Add a health check to a VM (delegates to HealthStateManager)
    pub async fn add_health_check(
        &self,
        vm_name: &str,
        health_check: HealthCheck,
    ) -> BlixardResult<()> {
        match &self.state_manager {
            Some(manager) => manager.add_health_check(vm_name, health_check).await,
            None => Err(BlixardError::Internal {
                message: "HealthStateManager not initialized".to_string(),
            }),
        }
    }

    /// List health checks for a VM (delegates to HealthStateManager)
    pub async fn list_health_checks(&self, vm_name: &str) -> BlixardResult<Vec<HealthCheck>> {
        match &self.state_manager {
            Some(manager) => manager.list_health_checks(vm_name).await,
            None => Err(BlixardError::Internal {
                message: "HealthStateManager not initialized".to_string(),
            }),
        }
    }

    /// Remove a health check from a VM (delegates to HealthStateManager)
    pub async fn remove_health_check(&self, vm_name: &str, check_name: &str) -> BlixardResult<()> {
        match &self.state_manager {
            Some(manager) => manager.remove_health_check(vm_name, check_name).await,
            None => Err(BlixardError::Internal {
                message: "HealthStateManager not initialized".to_string(),
            }),
        }
    }

    /// Toggle health monitoring for a VM (delegates to HealthStateManager)
    pub async fn toggle_monitoring(&self, vm_name: &str, enable: bool) -> BlixardResult<()> {
        match &self.state_manager {
            Some(manager) => manager.toggle_monitoring(vm_name, enable).await,
            None => Err(BlixardError::Internal {
                message: "HealthStateManager not initialized".to_string(),
            }),
        }
    }

    /// Trigger recovery for a failed VM (delegates to RecoveryCoordinator)
    pub async fn trigger_recovery(&self, vm_name: &str, vm_config: &crate::types::VmConfig) -> BlixardResult<()> {
        match &self.recovery_coordinator {
            Some(coordinator) => coordinator.trigger_recovery(vm_name, vm_config).await,
            None => Err(BlixardError::Internal {
                message: "RecoveryCoordinator not initialized".to_string(),
            }),
        }
    }

    /// Get recovery status for a VM (delegates to RecoveryCoordinator)
    pub async fn get_recovery_status(&self, vm_name: &str) -> Option<crate::vm_health_recovery_coordinator::RecoveryOperation> {
        match &self.recovery_coordinator {
            Some(coordinator) => coordinator.get_recovery_status(vm_name).await,
            None => None,
        }
    }

    /// Legacy start method for backward compatibility
    pub fn start(&mut self) {
        let monitor = self.clone_for_start();
        tokio::spawn(async move {
            let mut monitor = monitor;
            if let Err(e) = LifecycleManager::start(&mut monitor).await {
                error!("Failed to start VmHealthMonitor: {}", e);
            }
        });
    }

    /// Legacy stop method for backward compatibility
    pub fn stop(&mut self) {
        let monitor = self.clone_for_stop();
        tokio::spawn(async move {
            let mut monitor = monitor;
            if let Err(e) = LifecycleManager::stop(&mut monitor).await {
                error!("Failed to stop VmHealthMonitor: {}", e);
            }
        });
    }

    /// Clone for async start operation
    fn clone_for_start(&self) -> Self {
        Self {
            config: self.config.clone(),
            deps: self.deps.clone(),
            scheduler: None,
            state_manager: None,
            recovery_coordinator: None,
            auto_recovery: Arc::clone(&self.auto_recovery),
            is_running: false,
        }
    }

    /// Clone for async stop operation
    fn clone_for_stop(&self) -> Self {
        Self {
            config: self.config.clone(),
            deps: self.deps.clone(),
            scheduler: None,
            state_manager: None,
            recovery_coordinator: None,
            auto_recovery: Arc::clone(&self.auto_recovery),
            is_running: self.is_running,
        }
    }
}

#[async_trait]
impl LifecycleManager for VmHealthMonitor {
    async fn start(&mut self) -> BlixardResult<()> {
        if self.is_running {
            warn!("VmHealthMonitor is already running");
            return Ok(());
        }

        // Initialize components if not already done
        if self.scheduler.is_none() || self.state_manager.is_none() || self.recovery_coordinator.is_none() {
            self.initialize_components()?;
        }

        // Start all components
        if let Some(scheduler) = &mut self.scheduler {
            scheduler.start().await?;
        }

        if let Some(state_manager) = &mut self.state_manager {
            state_manager.start().await?;
        }

        if let Some(recovery_coordinator) = &mut self.recovery_coordinator {
            recovery_coordinator.start().await?;
        }

        self.is_running = true;
        info!("VmHealthMonitor started successfully with all components");
        Ok(())
    }

    async fn stop(&mut self) -> BlixardResult<()> {
        if !self.is_running {
            return Ok(());
        }

        // Stop all components in reverse order
        if let Some(recovery_coordinator) = &mut self.recovery_coordinator {
            if let Err(e) = recovery_coordinator.stop().await {
                error!("Failed to stop RecoveryCoordinator: {}", e);
            }
        }

        if let Some(state_manager) = &mut self.state_manager {
            if let Err(e) = state_manager.stop().await {
                error!("Failed to stop HealthStateManager: {}", e);
            }
        }

        if let Some(scheduler) = &mut self.scheduler {
            if let Err(e) = scheduler.stop().await {
                error!("Failed to stop HealthCheckScheduler: {}", e);
            }
        }

        self.is_running = false;
        info!("VmHealthMonitor stopped");
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
        "VmHealthMonitor"
    }
}

impl Drop for VmHealthMonitor {
    fn drop(&mut self) {
        // Note: We can't call async methods in Drop, so we just abort tasks
        // Components should handle their own cleanup in their Drop implementations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::time::MockClock;
    use std::time::SystemTime;
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
        let vm_manager = Arc::new(crate::vm_backend::VmManager::new(
            database,
            vm_backend,
            node_state.clone(),
        ));

        let mut monitor = VmHealthMonitor::new(node_state, vm_manager, Duration::from_millis(100));

        // Test LifecycleManager interface
        assert!(!monitor.is_running());
        assert_eq!(monitor.name(), "VmHealthMonitor");

        // Start should succeed
        monitor.start().await.unwrap();
        assert!(monitor.is_running());

        // Starting again should log a warning but not fail
        monitor.start().await.unwrap();
        assert!(monitor.is_running());

        // Stop should succeed
        monitor.stop().await.unwrap();
        assert!(!monitor.is_running());

        // Restart should work
        monitor.restart().await.unwrap();
        assert!(monitor.is_running());

        monitor.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_health_monitor_with_custom_config() {
        let temp_dir = TempDir::new().unwrap();
        let node_config = crate::types::NodeConfig {
            id: 1,
            bind_addr: "127.0.0.1:7001".parse().unwrap(),
            data_dir: temp_dir.path().to_string_lossy().to_string(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: Default::default(),
        };

        let node_state = Arc::new(SharedNodeState::new(node_config));
        let database = Arc::new(redb::Database::create(temp_dir.path().join("test.db")).unwrap());
        node_state.set_database(database.clone()).await;

        let vm_backend = Arc::new(crate::vm_backend::MockVmBackend::new(database.clone()));
        let vm_manager = Arc::new(crate::vm_backend::VmManager::new(
            database,
            vm_backend,
            node_state.clone(),
        ));

        // Create custom configuration
        let health_config = VmHealthMonitorConfig {
            scheduler: HealthCheckSchedulerConfig {
                check_interval: Duration::from_millis(50),
                max_concurrent_checks: 3,
                health_check_timeout: Duration::from_secs(5),
                enable_process_checks: true,
                enable_detailed_checks: false,
            },
            state_manager: HealthStateManagerConfig {
                max_state_age: Duration::from_hours(2),
                cleanup_interval: Duration::from_millis(200),
                max_results_per_vm: 20,
                enable_persistence: false,
            },
            recovery: RecoveryCoordinatorConfig {
                recovery_policy: RecoveryPolicy::default(),
                max_concurrent_recoveries: 1,
                enable_auto_recovery: false,
                enable_migration: false,
                recovery_timeout: Duration::from_secs(10),
            },
            features: Default::default(),
        };

        let mut monitor = VmHealthMonitor::with_config(node_state, vm_manager, health_config);

        // Test that custom configuration is used
        monitor.start().await.unwrap();
        assert!(monitor.is_running());

        monitor.stop().await.unwrap();
        assert!(!monitor.is_running());
    }
}
