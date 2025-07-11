//! Recovery coordination component
//!
//! This module provides the RecoveryCoordinator component that manages
//! VM recovery operations using the LifecycleManager pattern.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tracing::{error, info, warn};
use async_trait::async_trait;

use crate::{
    error::{BlixardError, BlixardResult},
    patterns::LifecycleManager,
    types::VmConfig,
    vm_auto_recovery::VmAutoRecovery,
    vm_health_config::{RecoveryCoordinatorConfig, VmHealthMonitorDependencies},
};

/// Recovery operation status
#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    pub vm_name: String,
    pub started_at: std::time::SystemTime,
    pub status: RecoveryStatus,
    pub attempts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    TimedOut,
}

/// Component responsible for coordinating VM recovery operations
#[derive(Debug)]
pub struct RecoveryCoordinator {
    config: RecoveryCoordinatorConfig,
    deps: VmHealthMonitorDependencies,
    auto_recovery: Arc<VmAutoRecovery>,
    /// Semaphore to limit concurrent recovery operations
    recovery_semaphore: Arc<Semaphore>,
    /// Track active recovery operations
    active_recoveries: Arc<RwLock<HashMap<String, RecoveryOperation>>>,
    is_running: bool,
}

impl RecoveryCoordinator {
    /// Create new recovery coordinator
    pub fn new(
        config: RecoveryCoordinatorConfig,
        deps: VmHealthMonitorDependencies,
        auto_recovery: Arc<VmAutoRecovery>,
    ) -> Self {
        let recovery_semaphore = Arc::new(Semaphore::new(config.max_concurrent_recoveries));

        Self {
            config,
            deps,
            auto_recovery,
            recovery_semaphore,
            active_recoveries: Arc::new(RwLock::new(HashMap::new())),
            is_running: false,
        }
    }

    /// Trigger recovery for a failed VM
    pub async fn trigger_recovery(&self, vm_name: &str, vm_config: &VmConfig) -> BlixardResult<()> {
        if !self.config.enable_auto_recovery {
            info!("Auto-recovery is disabled, skipping recovery for VM '{}'", vm_name);
            return Ok(());
        }

        // Check if recovery is already in progress
        {
            let active = self.active_recoveries.read().await;
            if let Some(recovery) = active.get(vm_name) {
                if matches!(recovery.status, RecoveryStatus::InProgress | RecoveryStatus::Pending) {
                    info!("Recovery already in progress for VM '{}'", vm_name);
                    return Ok(());
                }
            }
        }

        // Create recovery operation
        let recovery_op = RecoveryOperation {
            vm_name: vm_name.to_string(),
            started_at: std::time::SystemTime::now(),
            status: RecoveryStatus::Pending,
            attempts: 0,
            last_error: None,
        };

        // Add to active recoveries
        {
            let mut active = self.active_recoveries.write().await;
            active.insert(vm_name.to_string(), recovery_op);
        }

        // Execute recovery in background
        let coordinator = self.clone_for_recovery();
        let vm_name = vm_name.to_string();
        let vm_config = vm_config.clone();

        tokio::spawn(async move {
            coordinator.execute_recovery(&vm_name, &vm_config).await;
        });

        Ok(())
    }

    /// Execute recovery operation for a VM
    async fn execute_recovery(&self, vm_name: &str, vm_config: &VmConfig) {
        // Acquire semaphore to limit concurrent recoveries
        let _permit = match self.recovery_semaphore.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                warn!("Too many concurrent recovery operations, queueing recovery for VM '{}'", vm_name);
                
                // Wait for permit with timeout
                match tokio::time::timeout(
                    self.config.recovery_timeout,
                    self.recovery_semaphore.acquire()
                ).await {
                    Ok(Ok(permit)) => permit,
                    Ok(Err(_)) => {
                        error!("Recovery semaphore closed for VM '{}'", vm_name);
                        self.mark_recovery_failed(vm_name, "Recovery semaphore closed").await;
                        return;
                    }
                    Err(_) => {
                        error!("Recovery timeout waiting for semaphore for VM '{}'", vm_name);
                        self.mark_recovery_failed(vm_name, "Timeout waiting for recovery slot").await;
                        return;
                    }
                }
            }
        };

        // Update status to in progress
        self.update_recovery_status(vm_name, RecoveryStatus::InProgress).await;

        info!("Starting recovery for VM '{}'", vm_name);

        // Execute recovery with timeout
        let recovery_result = tokio::time::timeout(
            self.config.recovery_timeout,
            self.auto_recovery.trigger_recovery(vm_name, vm_config)
        ).await;

        match recovery_result {
            Ok(Ok(())) => {
                info!("Recovery completed successfully for VM '{}'", vm_name);
                self.mark_recovery_completed(vm_name).await;
            }
            Ok(Err(e)) => {
                error!("Recovery failed for VM '{}': {}", vm_name, e);
                self.mark_recovery_failed(vm_name, &e.to_string()).await;

                // If migration is enabled and recovery failed, attempt migration
                if self.config.enable_migration {
                    info!("Attempting VM migration for failed recovery of VM '{}'", vm_name);
                    if let Err(migration_error) = self.attempt_migration(vm_name, vm_config).await {
                        error!("Migration also failed for VM '{}': {}", vm_name, migration_error);
                    }
                }
            }
            Err(_timeout) => {
                error!("Recovery timed out for VM '{}'", vm_name);
                self.mark_recovery_timeout(vm_name).await;
            }
        }
    }

    /// Attempt to migrate a VM to another node
    async fn attempt_migration(&self, vm_name: &str, vm_config: &VmConfig) -> BlixardResult<()> {
        // For now, this is a placeholder. In a real implementation, this would:
        // 1. Find an available node with sufficient resources
        // 2. Schedule the VM on the new node
        // 3. Update the VM's node assignment in Raft
        // 4. Clean up the old VM instance
        
        info!("Migration not yet implemented for VM '{}'", vm_name);
        Err(BlixardError::NotImplemented {
            feature: "VM migration".to_string(),
        })
    }

    /// Update recovery operation status
    async fn update_recovery_status(&self, vm_name: &str, status: RecoveryStatus) {
        let mut active = self.active_recoveries.write().await;
        if let Some(recovery) = active.get_mut(vm_name) {
            recovery.status = status;
            recovery.attempts += 1;
        }
    }

    /// Mark recovery as completed
    async fn mark_recovery_completed(&self, vm_name: &str) {
        let mut active = self.active_recoveries.write().await;
        if let Some(recovery) = active.get_mut(vm_name) {
            recovery.status = RecoveryStatus::Completed;
        }
    }

    /// Mark recovery as failed
    async fn mark_recovery_failed(&self, vm_name: &str, error: &str) {
        let mut active = self.active_recoveries.write().await;
        if let Some(recovery) = active.get_mut(vm_name) {
            recovery.status = RecoveryStatus::Failed;
            recovery.last_error = Some(error.to_string());
        }
    }

    /// Mark recovery as timed out
    async fn mark_recovery_timeout(&self, vm_name: &str) {
        let mut active = self.active_recoveries.write().await;
        if let Some(recovery) = active.get_mut(vm_name) {
            recovery.status = RecoveryStatus::TimedOut;
            recovery.last_error = Some("Recovery operation timed out".to_string());
        }
    }

    /// Get recovery status for a VM
    pub async fn get_recovery_status(&self, vm_name: &str) -> Option<RecoveryOperation> {
        self.active_recoveries.read().await.get(vm_name).cloned()
    }

    /// List all active recovery operations
    pub async fn list_active_recoveries(&self) -> Vec<RecoveryOperation> {
        self.active_recoveries.read().await.values().cloned().collect()
    }

    /// Clean up completed recovery operations
    pub async fn cleanup_completed_recoveries(&self) {
        let mut active = self.active_recoveries.write().await;
        active.retain(|_, recovery| {
            !matches!(recovery.status, RecoveryStatus::Completed | RecoveryStatus::Failed | RecoveryStatus::TimedOut)
        });
    }

    /// Create a clone for recovery execution (to avoid self-reference issues)
    fn clone_for_recovery(&self) -> Self {
        Self {
            config: self.config.clone(),
            deps: self.deps.clone(),
            auto_recovery: Arc::clone(&self.auto_recovery),
            recovery_semaphore: Arc::clone(&self.recovery_semaphore),
            active_recoveries: Arc::clone(&self.active_recoveries),
            is_running: self.is_running,
        }
    }
}

impl Clone for RecoveryCoordinator {
    fn clone(&self) -> Self {
        self.clone_for_recovery()
    }
}

#[async_trait]
impl LifecycleManager for RecoveryCoordinator {
    async fn start(&mut self) -> BlixardResult<()> {
        if self.is_running {
            warn!("RecoveryCoordinator is already running");
            return Ok(());
        }

        self.is_running = true;
        info!("RecoveryCoordinator started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> BlixardResult<()> {
        if !self.is_running {
            return Ok(());
        }

        // Wait for active recoveries to complete or timeout
        let active_count = self.active_recoveries.read().await.len();
        if active_count > 0 {
            info!("Waiting for {} active recovery operations to complete", active_count);
            
            // Give active recoveries some time to complete
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        self.is_running = false;
        info!("RecoveryCoordinator stopped");
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
        "RecoveryCoordinator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::time::MockClock;
    use crate::vm_auto_recovery::RecoveryPolicy;
    use crate::types::Hypervisor;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recovery_coordinator_lifecycle() {
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

        let recovery_config = RecoveryCoordinatorConfig {
            recovery_policy: RecoveryPolicy::default(),
            max_concurrent_recoveries: 2,
            enable_auto_recovery: true,
            enable_migration: false,
            recovery_timeout: Duration::from_secs(30),
        };

        let mut coordinator = RecoveryCoordinator::new(recovery_config, deps, auto_recovery);

        // Test lifecycle
        assert!(!coordinator.is_running());
        assert_eq!(coordinator.name(), "RecoveryCoordinator");

        // Start should succeed
        coordinator.start().await.unwrap();
        assert!(coordinator.is_running());

        // Test recovery triggering
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            vcpus: 2,
            memory: 1024,
            preemptible: false,
            priority: 100,
            anti_affinity: None,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            kernel: None,
            init_command: None,
            flake_modules: vec![],
            hypervisor: Hypervisor::CloudHypervisor,
            tenant_id: None,
        };

        coordinator.trigger_recovery("test-vm", &vm_config).await.unwrap();

        // Give recovery some time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        let status = coordinator.get_recovery_status("test-vm").await;
        assert!(status.is_some());

        let recoveries = coordinator.list_active_recoveries().await;
        assert!(!recoveries.is_empty());

        // Stop should succeed
        coordinator.stop().await.unwrap();
        assert!(!coordinator.is_running());
    }
}