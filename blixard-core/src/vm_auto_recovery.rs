use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, metrics};
use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::SharedNodeState,
    types::VmConfig,
    vm_scheduler::{PlacementStrategy, VmScheduler},
};

/// Recovery policy for a failed VM
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryPolicy {
    /// Maximum number of restart attempts
    pub max_restart_attempts: u32,
    /// Delay between restart attempts
    pub restart_delay: Duration,
    /// Whether to attempt migration to another node after local failures
    pub enable_migration: bool,
    /// Backoff multiplier for restart delays (e.g., 2.0 for exponential backoff)
    pub backoff_multiplier: f64,
    /// Maximum backoff delay
    pub max_backoff_delay: Duration,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_restart_attempts: 3,
            restart_delay: Duration::from_secs(5),
            enable_migration: true,
            backoff_multiplier: 2.0,
            max_backoff_delay: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Recovery state for a specific VM
#[derive(Debug)]
struct VmRecoveryState {
    /// Number of restart attempts so far
    restart_attempts: u32,
    /// Last restart attempt time
    last_attempt: Instant,
    /// Current backoff delay
    current_delay: Duration,
    /// Whether recovery has been exhausted
    exhausted: bool,
}

/// Auto-recovery manager for failed VMs
///
/// This component manages the recovery of failed VMs with configurable
/// policies including restart attempts, backoff strategies, and migration.
#[derive(Debug)]
pub struct VmAutoRecovery {
    node_state: Arc<SharedNodeState>,
    policy: RecoveryPolicy,
    recovery_states: Arc<RwLock<HashMap<String, VmRecoveryState>>>,
}

impl VmAutoRecovery {
    /// Create a new auto-recovery manager
    pub fn new(node_state: Arc<SharedNodeState>, policy: RecoveryPolicy) -> Self {
        Self {
            node_state,
            policy,
            recovery_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Trigger recovery for a failed VM
    pub async fn trigger_recovery(&self, vm_name: &str, vm_config: &VmConfig) -> BlixardResult<()> {
        #[cfg(feature = "observability")]
        let _metrics = metrics();

        // Check recovery state
        let (_should_attempt, restart_attempts, max_attempts) = {
            let mut states = self.recovery_states.write().await;
            let state = self.get_or_create_recovery_state(&mut states, vm_name);

            // Check if recovery is exhausted
            if let Err(e) = self.check_recovery_exhausted(state, vm_name) {
                return Err(e);
            }

            // Check backoff period
            if self.is_in_backoff_period(state, vm_name)? {
                return Ok(());
            }

            // Update recovery attempt
            state.restart_attempts += 1;
            state.last_attempt = Instant::now();
            
            (true, state.restart_attempts, self.policy.max_restart_attempts)
        };

        info!(
            "Attempting recovery for VM '{}' (attempt {}/{})",
            vm_name, restart_attempts, max_attempts
        );

        // Try local restart first
        let restart_result = self.attempt_local_restart(vm_name).await;

        match restart_result {
            Ok(()) => {
                let mut states = self.recovery_states.write().await;
                self.handle_restart_success(vm_name, &mut states).await
            }
            Err(e) => {
                self.handle_restart_failure(vm_name, vm_config, e).await
            }
        }
    }

    /// Get or create recovery state for a VM
    fn get_or_create_recovery_state<'a>(
        &self,
        states: &'a mut HashMap<String, VmRecoveryState>,
        vm_name: &str,
    ) -> &'a mut VmRecoveryState {
        states
            .entry(vm_name.to_string())
            .or_insert_with(|| VmRecoveryState {
                restart_attempts: 0,
                last_attempt: Instant::now() - self.policy.restart_delay, // Allow immediate first attempt
                current_delay: self.policy.restart_delay,
                exhausted: false,
            })
    }

    /// Check if recovery is exhausted for a VM
    fn check_recovery_exhausted(
        &self,
        state: &VmRecoveryState,
        vm_name: &str,
    ) -> BlixardResult<()> {
        if state.exhausted {
            warn!(
                "Recovery exhausted for VM '{}' - manual intervention required",
                vm_name
            );
            Err(BlixardError::VmOperationFailed {
                operation: "auto_recovery".to_string(),
                details: format!(
                    "Recovery exhausted after {} attempts",
                    self.policy.max_restart_attempts
                ),
            })
        } else {
            Ok(())
        }
    }

    /// Check if VM is in backoff period
    fn is_in_backoff_period(
        &self,
        state: &VmRecoveryState,
        vm_name: &str,
    ) -> BlixardResult<bool> {
        let elapsed = state.last_attempt.elapsed();
        if elapsed < state.current_delay {
            let remaining = state.current_delay - elapsed;
            info!(
                "VM '{}' recovery in backoff period - {} seconds remaining",
                vm_name,
                remaining.as_secs()
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Handle successful restart
    async fn handle_restart_success(
        &self,
        vm_name: &str,
        states: &mut HashMap<String, VmRecoveryState>,
    ) -> BlixardResult<()> {
        info!("Successfully restarted VM '{}'", vm_name);
        
        #[cfg(feature = "observability")]
        {
            let metrics = metrics();
            metrics.vm_recovery_success.add(
                1,
                &[
                    attributes::vm_name(vm_name),
                    attributes::recovery_type("restart"),
                ],
            );
        }
        
        // Reset recovery state on success
        states.remove(vm_name);
        Ok(())
    }

    /// Handle restart failure
    async fn handle_restart_failure(
        &self,
        vm_name: &str,
        vm_config: &VmConfig,
        error: BlixardError,
    ) -> BlixardResult<()> {
        // Re-acquire lock to access state
        let mut states = self.recovery_states.write().await;
        let state = states.get_mut(vm_name).unwrap(); // Safe: we just created/accessed it
        warn!("Failed to restart VM '{}': {}", vm_name, error);
        
        #[cfg(feature = "observability")]
        {
            let metrics = metrics();
            metrics.vm_recovery_failed.add(
                1,
                &[
                    attributes::vm_name(vm_name),
                    attributes::recovery_type("restart"),
                    attributes::error(true),
                ],
            );
        }

        // Check if we've exhausted restart attempts
        if state.restart_attempts >= self.policy.max_restart_attempts {
            drop(states); // Release lock before potentially long operation
            return self.handle_restart_exhausted(vm_name, vm_config).await;
        }

        // Update backoff delay
        self.update_backoff_delay(state);

        info!(
            "VM '{}' recovery failed - next attempt in {} seconds",
            vm_name,
            state.current_delay.as_secs()
        );

        Err(error)
    }

    /// Handle exhausted restart attempts
    async fn handle_restart_exhausted(
        &self,
        vm_name: &str,
        vm_config: &VmConfig,
    ) -> BlixardResult<()> {
        if self.policy.enable_migration {
            info!(
                "Local restart attempts exhausted for VM '{}', attempting migration",
                vm_name
            );
            self.attempt_migration(vm_name, vm_config).await
        } else {
            // Mark as exhausted
            let mut states = self.recovery_states.write().await;
            if let Some(state) = states.get_mut(vm_name) {
                state.exhausted = true;
            }
            error!("Recovery exhausted for VM '{}' - no more attempts", vm_name);
            Err(BlixardError::VmOperationFailed {
                operation: "auto_recovery".to_string(),
                details: format!(
                    "All {} restart attempts failed",
                    self.policy.max_restart_attempts
                ),
            })
        }
    }

    /// Update backoff delay with exponential backoff
    fn update_backoff_delay(&self, state: &mut VmRecoveryState) {
        state.current_delay = Duration::from_secs_f64(
            (state.current_delay.as_secs_f64() * self.policy.backoff_multiplier)
                .min(self.policy.max_backoff_delay.as_secs_f64()),
        );
    }

    /// Attempt to restart the VM on the local node
    async fn attempt_local_restart(&self, vm_name: &str) -> BlixardResult<()> {
        info!("Attempting local restart for VM '{}'", vm_name);

        self.node_state.start_vm(vm_name).await
    }

    /// Attempt to migrate the VM to another node
    async fn attempt_migration(&self, vm_name: &str, vm_config: &VmConfig) -> BlixardResult<()> {
        info!("Attempting migration for VM '{}'", vm_name);
        #[cfg(feature = "observability")]
        let metrics = metrics();

        // Get the database to access scheduler
        let database =
            self.node_state
                .get_database()
                .await
                .ok_or_else(|| BlixardError::Internal {
                    message: "Database not initialized".to_string(),
                })?;

        // Use scheduler to find a suitable target node
        let scheduler = VmScheduler::new(database);
        let placement = scheduler
            .schedule_vm_placement(vm_config, PlacementStrategy::MostAvailable)
            .await?;

        let current_node_id = self.node_state.get_id();

        // Check if the selected node is different from current
        if placement.target_node_id == current_node_id {
            warn!(
                "No alternative node available for VM '{}' migration",
                vm_name
            );
            #[cfg(feature = "observability")]
            metrics.vm_recovery_failed.add(
                1,
                &[
                    attributes::vm_name(vm_name),
                    attributes::recovery_type("migration"),
                    attributes::error(true),
                ],
            );
            return Err(BlixardError::VmOperationFailed {
                operation: "auto_recovery_migration".to_string(),
                details: "No alternative node available for migration".to_string(),
            });
        }

        info!(
            "Migrating VM '{}' from node {} to node {}",
            vm_name, current_node_id, placement.target_node_id
        );

        // Create migration task
        match self
            .node_state
            .migrate_vm(vm_name, placement.target_node_id)
            .await
        {
            Ok(()) => {
                info!("Successfully initiated migration for VM '{}'", vm_name);
                #[cfg(feature = "observability")]
                metrics.vm_recovery_success.add(
                    1,
                    &[
                        attributes::vm_name(vm_name),
                        attributes::recovery_type("migration"),
                    ],
                );

                // Clear recovery state after successful migration
                let mut states = self.recovery_states.write().await;
                states.remove(vm_name);

                Ok(())
            }
            Err(e) => {
                error!("Failed to migrate VM '{}': {}", vm_name, e);
                #[cfg(feature = "observability")]
                metrics.vm_recovery_failed.add(
                    1,
                    &[
                        attributes::vm_name(vm_name),
                        attributes::recovery_type("migration"),
                        attributes::error(true),
                    ],
                );

                // Mark recovery as exhausted
                let mut states = self.recovery_states.write().await;
                if let Some(state) = states.get_mut(vm_name) {
                    state.exhausted = true;
                }

                Err(e)
            }
        }
    }

    /// Reset recovery state for a VM (e.g., after manual intervention)
    pub async fn reset_recovery_state(&self, vm_name: &str) {
        let mut states = self.recovery_states.write().await;
        if states.remove(vm_name).is_some() {
            info!("Reset recovery state for VM '{}'", vm_name);
        }
    }

    /// Get recovery statistics
    pub async fn get_recovery_stats(&self) -> HashMap<String, (u32, bool)> {
        let states = self.recovery_states.read().await;
        states
            .iter()
            .map(|(name, state)| (name.clone(), (state.restart_attempts, state.exhausted)))
            .collect()
    }

    /// Configure recovery policy for a specific VM
    pub async fn configure_recovery_policy(
        &self,
        _vm_name: &str,
        _policy: RecoveryPolicy,
    ) -> BlixardResult<()> {
        // TODO: Implement per-VM recovery policies
        // For now, the VmAutoRecovery uses a global policy
        // In the future, we could store per-VM policies in a HashMap
        warn!("Per-VM recovery policies not implemented yet - using global policy");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recovery_policy_backoff() {
        let policy = RecoveryPolicy {
            restart_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_backoff_delay: Duration::from_secs(10),
            ..Default::default()
        };

        let mut delay = policy.restart_delay;

        // Test exponential backoff
        delay = Duration::from_secs_f64(delay.as_secs_f64() * policy.backoff_multiplier);
        assert_eq!(delay.as_secs(), 2);

        delay = Duration::from_secs_f64(delay.as_secs_f64() * policy.backoff_multiplier);
        assert_eq!(delay.as_secs(), 4);

        delay = Duration::from_secs_f64(delay.as_secs_f64() * policy.backoff_multiplier);
        assert_eq!(delay.as_secs(), 8);

        // Test max backoff
        delay = Duration::from_secs_f64(delay.as_secs_f64() * policy.backoff_multiplier);
        delay = Duration::from_secs_f64(
            delay
                .as_secs_f64()
                .min(policy.max_backoff_delay.as_secs_f64()),
        );
        assert_eq!(delay.as_secs(), 10); // Capped at max
    }
}
