use redb::{Database, ReadableTable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::{
    error::BlixardResult,
    storage::VM_STATE_TABLE,
    types::{VmConfig, VmState, VmStatus},
    vm_backend::VmBackend,
};

/// VM state persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmPersistenceConfig {
    /// Enable automatic VM recovery on node restart
    pub auto_recovery_enabled: bool,
    /// Maximum number of VMs to recover in parallel
    pub max_parallel_recovery: usize,
    /// Delay between recovery attempts (in seconds)
    pub recovery_delay_secs: u64,
    /// Skip VMs that were in failed state before restart
    pub skip_failed_vms: bool,
}

impl Default for VmPersistenceConfig {
    fn default() -> Self {
        Self {
            auto_recovery_enabled: true,
            max_parallel_recovery: 5,
            recovery_delay_secs: 2,
            skip_failed_vms: true,
        }
    }
}

/// VM state persistence manager
///
/// This component handles persisting VM state to the database and
/// recovering VMs after node restarts. It ensures that VM configurations
/// and states survive node failures.
pub struct VmStatePersistence {
    database: Arc<Database>,
    config: VmPersistenceConfig,
    /// Track VMs that have been persisted this session
    persisted_vms: Arc<RwLock<HashMap<String, VmState>>>,
}

impl VmStatePersistence {
    /// Create a new VM state persistence manager
    pub fn new(database: Arc<Database>, config: VmPersistenceConfig) -> Self {
        Self {
            database,
            config,
            persisted_vms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Persist VM state to the database
    pub async fn persist_vm_state(&self, vm_state: &VmState) -> BlixardResult<()> {
        debug!("Persisting state for VM: {}", vm_state.name);

        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE)?;
            let serialized = bincode::serialize(vm_state)?;
            table.insert(vm_state.name.as_str(), serialized.as_slice())?;
        }
        write_txn.commit()?;

        // Track persisted VM
        self.persisted_vms
            .write()
            .await
            .insert(vm_state.name.clone(), vm_state.clone());

        info!(
            "Persisted state for VM '{}' with status {:?}",
            vm_state.name, vm_state.status
        );

        Ok(())
    }

    /// Remove VM state from persistence
    pub async fn remove_vm_state(&self, vm_name: &str) -> BlixardResult<()> {
        debug!("Removing persisted state for VM: {}", vm_name);

        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE)?;
            table.remove(vm_name)?;
        }
        write_txn.commit()?;

        self.persisted_vms.write().await.remove(vm_name);

        info!("Removed persisted state for VM '{}'", vm_name);
        Ok(())
    }

    /// Load all persisted VM states from the database
    pub async fn load_persisted_vms(&self) -> BlixardResult<Vec<VmState>> {
        debug!("Loading persisted VM states from database");

        let read_txn = self.database.begin_read()?;
        let mut vms = Vec::new();

        if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                match bincode::deserialize::<VmState>(value.value()) {
                    Ok(vm_state) => {
                        debug!(
                            "Loaded persisted VM: {} (status: {:?})",
                            key.value(),
                            vm_state.status
                        );
                        vms.push(vm_state);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to deserialize VM state for '{}': {}",
                            key.value(),
                            e
                        );
                    }
                }
            }
        }

        info!("Loaded {} persisted VMs from database", vms.len());
        Ok(vms)
    }

    /// Recover VMs after node restart
    ///
    /// This method loads all persisted VMs and attempts to restart those
    /// that were running before the node restarted.
    pub async fn recover_vms(
        &self,
        vm_backend: Arc<dyn VmBackend>,
    ) -> BlixardResult<RecoveryReport> {
        if !self.config.auto_recovery_enabled {
            info!("VM auto-recovery is disabled");
            return Ok(RecoveryReport::default());
        }

        info!("Starting VM recovery after node restart");
        let persisted_vms = self.load_persisted_vms().await?;

        let mut report = RecoveryReport::default();
        report.total_vms = persisted_vms.len();

        // Filter VMs that need recovery
        let vms_to_recover: Vec<_> = persisted_vms
            .into_iter()
            .filter(|vm| {
                // Skip VMs that were not running
                if vm.status != VmStatus::Running && vm.status != VmStatus::Starting {
                    report
                        .skipped_vms
                        .push((vm.name.clone(), format!("VM was in {:?} state", vm.status)));
                    return false;
                }

                // Skip failed VMs if configured
                if self.config.skip_failed_vms && vm.status == VmStatus::Failed {
                    report.skipped_vms.push((
                        vm.name.clone(),
                        "Skipping failed VM as per configuration".to_string(),
                    ));
                    return false;
                }

                true
            })
            .collect();

        info!(
            "Found {} VMs to recover out of {} total persisted VMs",
            vms_to_recover.len(),
            report.total_vms
        );

        // Recover VMs in batches to avoid overwhelming the system
        for chunk in vms_to_recover.chunks(self.config.max_parallel_recovery) {
            let recovery_futures: Vec<_> = chunk
                .iter()
                .map(|vm_state| {
                    let backend = vm_backend.clone();
                    let vm_name = vm_state.name.clone();
                    let vm_config = vm_state.config.clone();

                    async move {
                        info!("Attempting to recover VM: {}", vm_name);

                        // First recreate the VM configuration
                        match backend.create_vm(&vm_config, vm_state.node_id).await {
                            Ok(_) => {
                                debug!("Recreated VM configuration for: {}", vm_name);
                            }
                            Err(e) => {
                                // VM might already exist, try to start it anyway
                                debug!("VM configuration might already exist: {}", e);
                            }
                        }

                        // Now start the VM
                        match backend.start_vm(&vm_name).await {
                            Ok(_) => {
                                info!("Successfully recovered VM: {}", vm_name);
                                Ok((vm_name, true))
                            }
                            Err(e) => {
                                error!("Failed to recover VM '{}': {}", vm_name, e);
                                Err((vm_name, e.to_string()))
                            }
                        }
                    }
                })
                .collect();

            // Execute recovery attempts in parallel
            let results = futures::future::join_all(recovery_futures).await;

            // Process results
            for result in results {
                match result {
                    Ok((name, _)) => {
                        report.recovered_vms.push(name);
                    }
                    Err((name, error)) => {
                        report.failed_recoveries.push((name, error));
                    }
                }
            }

            // Add delay between batches to avoid overwhelming the system
            if chunk.len() == self.config.max_parallel_recovery {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    self.config.recovery_delay_secs,
                ))
                .await;
            }
        }

        info!(
            "VM recovery completed: {} recovered, {} failed, {} skipped",
            report.recovered_vms.len(),
            report.failed_recoveries.len(),
            report.skipped_vms.len()
        );

        Ok(report)
    }

    /// Get recovery statistics
    pub async fn get_recovery_stats(&self) -> RecoveryStats {
        let persisted = self.persisted_vms.read().await;

        let mut stats = RecoveryStats::default();
        stats.total_persisted = persisted.len();

        for vm_state in persisted.values() {
            match vm_state.status {
                VmStatus::Running => stats.running_vms += 1,
                VmStatus::Stopped => stats.stopped_vms += 1,
                VmStatus::Failed => stats.failed_vms += 1,
                VmStatus::Starting => stats.starting_vms += 1,
                VmStatus::Stopping => stats.stopping_vms += 1,
                VmStatus::Creating => stats.starting_vms += 1,
            }
        }

        stats
    }

    /// Update VM status in persistence
    pub async fn update_vm_status(&self, vm_name: &str, new_status: VmStatus) -> BlixardResult<()> {
        // Load current state
        let read_txn = self.database.begin_read()?;
        let vm_state = if let Ok(table) = read_txn.open_table(VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(vm_name) {
                match bincode::deserialize::<VmState>(data.value()) {
                    Ok(mut state) => {
                        drop(read_txn); // Release read lock before writing
                        state.status = new_status;
                        state.updated_at = chrono::Utc::now();
                        Some(state)
                    }
                    Err(e) => {
                        warn!("Failed to deserialize VM state for status update: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Update if found
        if let Some(updated_state) = vm_state {
            self.persist_vm_state(&updated_state).await?;
        } else {
            warn!(
                "VM '{}' not found in persistence for status update",
                vm_name
            );
        }

        Ok(())
    }
}

/// Report of VM recovery operation
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Total number of VMs found in persistence
    pub total_vms: usize,
    /// VMs that were successfully recovered
    pub recovered_vms: Vec<String>,
    /// VMs that failed to recover with error messages
    pub failed_recoveries: Vec<(String, String)>,
    /// VMs that were skipped (not running or failed)
    pub skipped_vms: Vec<(String, String)>,
}

/// VM recovery statistics
#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub total_persisted: usize,
    pub running_vms: usize,
    pub stopped_vms: usize,
    pub failed_vms: usize,
    pub starting_vms: usize,
    pub stopping_vms: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_vm_state_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(db_path).unwrap());

        let persistence = VmStatePersistence::new(database.clone(), VmPersistenceConfig::default());

        // Create a test VM state
        let vm_state = VmState {
            name: "test-vm".to_string(),
            config: VmConfig {
                name: "test-vm".to_string(),
                config_path: "/tmp/test-vm".to_string(),
                vcpus: 2,
                memory: 1024,
                tenant_id: "test-tenant".to_string(),
                ip_address: None,
                metadata: None,
                anti_affinity: None,
                priority: 100,
                preemptible: false,
                locality_preference: crate::types::LocalityPreference::default(),
                health_check_config: None,
            },
            status: VmStatus::Running,
            node_id: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test persistence
        persistence.persist_vm_state(&vm_state).await.unwrap();

        // Test loading
        let loaded_vms = persistence.load_persisted_vms().await.unwrap();
        assert_eq!(loaded_vms.len(), 1);
        assert_eq!(loaded_vms[0].name, "test-vm");
        assert_eq!(loaded_vms[0].status, VmStatus::Running);

        // Test removal
        persistence.remove_vm_state("test-vm").await.unwrap();
        let loaded_vms = persistence.load_persisted_vms().await.unwrap();
        assert_eq!(loaded_vms.len(), 0);
    }

    #[tokio::test]
    async fn test_recovery_stats() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(db_path).unwrap());

        let persistence = VmStatePersistence::new(database.clone(), VmPersistenceConfig::default());

        // Persist multiple VMs with different states
        let states = vec![
            ("vm1", VmStatus::Running),
            ("vm2", VmStatus::Running),
            ("vm3", VmStatus::Stopped),
            ("vm4", VmStatus::Failed),
        ];

        for (name, status) in states {
            let vm_state = VmState {
                name: name.to_string(),
                config: VmConfig {
                    name: name.to_string(),
                    config_path: format!("/tmp/{}", name),
                    vcpus: 1,
                    memory: 512,
                    tenant_id: "test-tenant".to_string(),
                    ip_address: None,
                    metadata: None,
                    anti_affinity: None,
                    priority: 100,
                    preemptible: false,
                    locality_preference: crate::types::LocalityPreference::default(),
                    health_check_config: None,
                },
                status,
                node_id: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            persistence.persist_vm_state(&vm_state).await.unwrap();
        }

        let stats = persistence.get_recovery_stats().await;
        assert_eq!(stats.total_persisted, 4);
        assert_eq!(stats.running_vms, 2);
        assert_eq!(stats.stopped_vms, 1);
        assert_eq!(stats.failed_vms, 1);
    }
}
