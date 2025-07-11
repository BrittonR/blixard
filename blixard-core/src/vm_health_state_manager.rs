//! Health state management component
//!
//! This module provides the HealthStateManager component that manages
//! VM health state persistence and cleanup using the LifecycleManager pattern.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use async_trait::async_trait;

use crate::{
    abstractions::time::Clock,
    error::{BlixardError, BlixardResult},
    patterns::LifecycleManager,
    vm_health_config::{HealthStateManagerConfig, VmHealthMonitorDependencies},
    vm_health_types::{HealthCheck, VmHealthCheckConfig, VmHealthStatus},
};

/// Component responsible for managing VM health state
pub struct HealthStateManager {
    config: HealthStateManagerConfig,
    deps: VmHealthMonitorDependencies,
    /// Per-VM health check configurations
    health_configs: Arc<RwLock<HashMap<String, VmHealthCheckConfig>>>,
    /// Per-VM health status tracking
    health_statuses: Arc<RwLock<HashMap<String, VmHealthStatus>>>,
    /// Health monitoring enabled status per VM
    monitoring_enabled: Arc<RwLock<HashMap<String, bool>>>,
    cleanup_handle: Option<JoinHandle<()>>,
    is_running: bool,
}

impl std::fmt::Debug for HealthStateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthStateManager")
            .field("config", &self.config)
            .field("deps", &self.deps)
            .field("health_configs", &self.health_configs)
            .field("health_statuses", &self.health_statuses)
            .field("monitoring_enabled", &self.monitoring_enabled)
            .field("cleanup_handle", &self.cleanup_handle.as_ref().map(|_| "<JoinHandle>"))
            .field("is_running", &self.is_running)
            .finish()
    }
}

impl HealthStateManager {
    /// Create new health state manager
    pub fn new(
        config: HealthStateManagerConfig,
        deps: VmHealthMonitorDependencies,
    ) -> Self {
        Self {
            config,
            deps,
            health_configs: Arc::new(RwLock::new(HashMap::new())),
            health_statuses: Arc::new(RwLock::new(HashMap::new())),
            monitoring_enabled: Arc::new(RwLock::new(HashMap::new())),
            cleanup_handle: None,
            is_running: false,
        }
    }

    /// Get health status for a specific VM
    pub async fn get_health_status(&self, vm_name: &str) -> Option<VmHealthStatus> {
        self.health_statuses.read().await.get(vm_name).cloned()
    }

    /// Update health status for a VM
    pub async fn update_health_status(&self, vm_name: &str, status: VmHealthStatus) -> BlixardResult<()> {
        let mut statuses = self.health_statuses.write().await;
        
        // Check if we should store the status based on configuration
        if let Some(existing) = statuses.get(vm_name) {
            // Limit the number of stored statuses per VM
            if statuses.len() >= self.config.max_results_per_vm && !statuses.contains_key(vm_name) {
                warn!("Health status storage limit reached for VM '{}'", vm_name);
                return Ok(());
            }
        }

        statuses.insert(vm_name.to_string(), status);
        
        // If persistence is enabled, store to backend
        if self.config.enable_persistence {
            if let Err(e) = self.persist_health_status(vm_name, &statuses[vm_name]).await {
                error!("Failed to persist health status for VM '{}': {}", vm_name, e);
            }
        }

        Ok(())
    }

    /// Add a health check to a VM
    pub async fn add_health_check(
        &self,
        vm_name: &str,
        health_check: HealthCheck,
    ) -> BlixardResult<()> {
        let mut configs = self.health_configs.write().await;
        let config = configs
            .entry(vm_name.to_string())
            .or_insert_with(VmHealthCheckConfig::default);

        // Check if a health check with the same name already exists
        if config.checks.iter().any(|c| c.name == health_check.name) {
            return Err(BlixardError::ServiceAlreadyExists(format!(
                "Health check '{}' already exists for VM '{}'",
                health_check.name, vm_name
            )));
        }

        config.checks.push(health_check);

        // Enable monitoring for this VM by default
        self.monitoring_enabled
            .write()
            .await
            .insert(vm_name.to_string(), true);

        info!("Added health check to VM '{}'", vm_name);
        Ok(())
    }

    /// List health checks for a VM
    pub async fn list_health_checks(&self, vm_name: &str) -> BlixardResult<Vec<HealthCheck>> {
        let configs = self.health_configs.read().await;
        Ok(configs
            .get(vm_name)
            .map(|c| c.checks.clone())
            .unwrap_or_default())
    }

    /// Remove a health check from a VM
    pub async fn remove_health_check(&self, vm_name: &str, check_name: &str) -> BlixardResult<()> {
        let mut configs = self.health_configs.write().await;
        if let Some(config) = configs.get_mut(vm_name) {
            config.checks.retain(|c| c.name != check_name);
            info!(
                "Removed health check '{}' from VM '{}'",
                check_name, vm_name
            );
            Ok(())
        } else {
            Err(BlixardError::NotFound {
                resource: format!("VM '{}' has no health checks configured", vm_name),
            })
        }
    }

    /// Toggle health monitoring for a VM
    pub async fn toggle_monitoring(&self, vm_name: &str, enable: bool) -> BlixardResult<()> {
        self.monitoring_enabled
            .write()
            .await
            .insert(vm_name.to_string(), enable);
        info!(
            "Health monitoring {} for VM '{}'",
            if enable { "enabled" } else { "disabled" },
            vm_name
        );
        Ok(())
    }

    /// Check if monitoring is enabled for a VM
    pub async fn is_monitoring_enabled(&self, vm_name: &str) -> bool {
        self.monitoring_enabled
            .read()
            .await
            .get(vm_name)
            .copied()
            .unwrap_or(true)
    }

    /// Clean up old health state entries
    async fn cleanup_old_entries(&self) -> BlixardResult<()> {
        let current_time = self.deps.clock.now();
        let max_age = self.config.max_state_age;

        let mut statuses = self.health_statuses.write().await;
        let mut to_remove = Vec::new();

        for (vm_name, status) in statuses.iter() {
            let status_age = if let Some(last_healthy_at) = status.last_healthy_at_secs {
                current_time.duration_since(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(last_healthy_at as u64)
                ).unwrap_or(Duration::from_secs(0))
            } else {
                // If no last healthy time, consider it very old
                Duration::from_secs(u64::MAX / 2)
            };

            if status_age > max_age {
                to_remove.push(vm_name.clone());
            }
        }

        for vm_name in to_remove {
            statuses.remove(&vm_name);
            debug!("Cleaned up old health status for VM '{}'", vm_name);
        }

        Ok(())
    }

    /// Persist health status to backend storage
    async fn persist_health_status(&self, vm_name: &str, status: &VmHealthStatus) -> BlixardResult<()> {
        // Store to VM backend if it supports health status persistence
        self.deps.vm_manager
            .backend()
            .update_vm_health_status(vm_name, status.clone())
            .await
    }

    /// Start cleanup task
    async fn start_cleanup_task(&mut self) -> BlixardResult<()> {
        let cleanup_interval = self.config.cleanup_interval;
        let deps = self.deps.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let manager = HealthStateManager::new(config, deps);
            let mut interval = interval(cleanup_interval);

            loop {
                interval.tick().await;

                if let Err(e) = manager.cleanup_old_entries().await {
                    error!("Health state cleanup failed: {}", e);
                }
            }
        });

        self.cleanup_handle = Some(handle);
        Ok(())
    }
}

#[async_trait]
impl LifecycleManager for HealthStateManager {
    async fn start(&mut self) -> BlixardResult<()> {
        if self.is_running {
            warn!("HealthStateManager is already running");
            return Ok(());
        }

        // Start cleanup task if cleanup is enabled
        if self.config.cleanup_interval.as_secs() > 0 {
            self.start_cleanup_task().await?;
        }

        self.is_running = true;
        info!("HealthStateManager started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }

        self.is_running = false;
        info!("HealthStateManager stopped");
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
        "HealthStateManager"
    }
}

impl Drop for HealthStateManager {
    fn drop(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::time::MockClock;
    use crate::vm_health_types::{HealthCheckType, HealthState};
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_health_state_manager_lifecycle() {
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

        let state_config = HealthStateManagerConfig {
            max_state_age: Duration::from_hours(1),
            cleanup_interval: Duration::from_millis(100),
            max_results_per_vm: 10,
            enable_persistence: false,
        };

        let mut manager = HealthStateManager::new(state_config, deps);

        // Test lifecycle
        assert!(!manager.is_running());
        assert_eq!(manager.name(), "HealthStateManager");

        // Start should succeed
        manager.start().await.unwrap();
        assert!(manager.is_running());

        // Test health check management
        let health_check = HealthCheck {
            name: "tcp-check".to_string(),
            check_type: HealthCheckType::TcpConnect {
                host: "localhost".to_string(),
                port: 8080,
                timeout_ms: 5000,
            },
            interval_secs: 30,
            timeout_secs: 10,
            retries: 3,
        };

        manager.add_health_check("test-vm", health_check.clone()).await.unwrap();

        let checks = manager.list_health_checks("test-vm").await.unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "tcp-check");

        // Test monitoring toggle
        assert!(manager.is_monitoring_enabled("test-vm").await);
        manager.toggle_monitoring("test-vm", false).await.unwrap();
        assert!(!manager.is_monitoring_enabled("test-vm").await);

        // Test health status
        let mut status = VmHealthStatus::default();
        status.state = HealthState::Healthy;
        manager.update_health_status("test-vm", status).await.unwrap();

        let retrieved_status = manager.get_health_status("test-vm").await;
        assert!(retrieved_status.is_some());
        assert_eq!(retrieved_status.unwrap().state, HealthState::Healthy);

        // Stop should succeed
        manager.stop().await.unwrap();
        assert!(!manager.is_running());
    }
}