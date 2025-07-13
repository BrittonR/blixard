//! Real-world Arc refactoring example: VM Recovery optimization
//!
//! This demonstrates how to refactor the VM recovery code from vm_state_persistence.rs
//! to use Arc<VmConfig> instead of cloning VmConfig for each VM recovery task.

use blixard_core::{
    error::BlixardResult,
    types::{VmConfig, VmState, VmStatus},
    vm_backend::VmBackend,
};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Original implementation with expensive clones
mod before_refactoring {
    use super::*;

    pub async fn recover_vms_original(
        vms_to_recover: Vec<VmState>,
        vm_backend: Arc<dyn VmBackend>,
        max_parallel_recovery: usize,
    ) -> Vec<(String, Result<bool, String>)> {
        let mut all_results = Vec::new();

        // Recover VMs in batches to avoid overwhelming the system
        for chunk in vms_to_recover.chunks(max_parallel_recovery) {
            let recovery_futures: Vec<_> = chunk
                .iter()
                .map(|vm_state| {
                    let backend = vm_backend.clone();
                    let vm_name = vm_state.name.clone();
                    let vm_config = vm_state.config.clone(); // EXPENSIVE: Full VmConfig clone

                    async move {
                        info!("Attempting to recover VM: {}", vm_name);

                        // First recreate the VM configuration
                        match backend.create_vm(&vm_config, vm_state.node_id).await {
                            Ok(_) => {
                                debug!("Recreated VM configuration for: {}", vm_name);
                            }
                            Err(e) => {
                                debug!("VM configuration might already exist: {}", e);
                            }
                        }

                        // Now start the VM
                        match backend.start_vm(&vm_name).await {
                            Ok(_) => {
                                info!("Successfully recovered VM: {}", vm_name);
                                (vm_name, Ok(true))
                            }
                            Err(e) => {
                                error!("Failed to recover VM '{}': {}", vm_name, e);
                                (vm_name, Err(e.to_string()))
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(recovery_futures).await;
            all_results.extend(results);
        }

        all_results
    }
}

/// Optimized implementation with Arc
mod after_refactoring {
    use super::*;

    /// VmState with Arc-wrapped config
    #[derive(Clone)]
    pub struct VmStateArc {
        pub name: String,
        pub config: Arc<VmConfig>, // Arc for cheap cloning
        pub status: VmStatus,
        pub node_id: u64,
        pub created_at: chrono::DateTime<chrono::Utc>,
        pub updated_at: chrono::DateTime<chrono::Utc>,
    }

    impl From<VmState> for VmStateArc {
        fn from(state: VmState) -> Self {
            Self {
                name: state.name,
                config: Arc::new(state.config), // Wrap once
                status: state.status,
                node_id: state.node_id,
                created_at: state.created_at,
                updated_at: state.updated_at,
            }
        }
    }

    pub async fn recover_vms_optimized(
        vms_to_recover: Vec<VmState>,
        vm_backend: Arc<dyn VmBackend>,
        max_parallel_recovery: usize,
    ) -> Vec<(String, Result<bool, String>)> {
        // Convert to Arc version once
        let vms_to_recover: Vec<VmStateArc> = vms_to_recover
            .into_iter()
            .map(VmStateArc::from)
            .collect();

        let mut all_results = Vec::new();

        // Recover VMs in batches
        for chunk in vms_to_recover.chunks(max_parallel_recovery) {
            let recovery_futures: Vec<_> = chunk
                .iter()
                .map(|vm_state| {
                    let backend = vm_backend.clone();
                    let vm_name = vm_state.name.clone();
                    let vm_config = vm_state.config.clone(); // CHEAP: Just Arc pointer clone
                    let node_id = vm_state.node_id;

                    async move {
                        info!("Attempting to recover VM: {}", vm_name);

                        // Use the Arc<VmConfig> directly
                        match backend.create_vm(&vm_config, node_id).await {
                            Ok(_) => {
                                debug!("Recreated VM configuration for: {}", vm_name);
                            }
                            Err(e) => {
                                debug!("VM configuration might already exist: {}", e);
                            }
                        }

                        match backend.start_vm(&vm_name).await {
                            Ok(_) => {
                                info!("Successfully recovered VM: {}", vm_name);
                                (vm_name, Ok(true))
                            }
                            Err(e) => {
                                error!("Failed to recover VM '{}': {}", vm_name, e);
                                (vm_name, Err(e.to_string()))
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(recovery_futures).await;
            all_results.extend(results);
        }

        all_results
    }

    /// Further optimization: Pre-cache configs for duplicate VMs
    pub async fn recover_vms_with_cache(
        vms_to_recover: Vec<VmState>,
        vm_backend: Arc<dyn VmBackend>,
        max_parallel_recovery: usize,
    ) -> Vec<(String, Result<bool, String>)> {
        use std::collections::HashMap;

        // Build a cache of unique configs (many VMs might share the same config)
        let mut config_cache: HashMap<String, Arc<VmConfig>> = HashMap::new();
        let mut vms_with_cached_configs = Vec::new();

        for vm_state in vms_to_recover {
            // Create a cache key from config properties
            let cache_key = format!(
                "{}-{}-{}-{}",
                vm_state.config.config_path,
                vm_state.config.vcpus,
                vm_state.config.memory,
                vm_state.config.tenant_id
            );

            let cached_config = config_cache
                .entry(cache_key)
                .or_insert_with(|| Arc::new(vm_state.config.clone()))
                .clone();

            vms_with_cached_configs.push((
                vm_state.name,
                cached_config,
                vm_state.node_id,
            ));
        }

        info!(
            "Config cache hit rate: {} unique configs for {} VMs",
            config_cache.len(),
            vms_with_cached_configs.len()
        );

        let mut all_results = Vec::new();

        // Process with cached configs
        for chunk in vms_with_cached_configs.chunks(max_parallel_recovery) {
            let recovery_futures: Vec<_> = chunk
                .iter()
                .map(|(vm_name, vm_config, node_id)| {
                    let backend = vm_backend.clone();
                    let vm_name = vm_name.clone();
                    let vm_config = vm_config.clone(); // Super cheap Arc clone
                    let node_id = *node_id;

                    async move {
                        info!("Attempting to recover VM: {}", vm_name);

                        match backend.create_vm(&vm_config, node_id).await {
                            Ok(_) => {
                                debug!("Recreated VM configuration for: {}", vm_name);
                            }
                            Err(e) => {
                                debug!("VM configuration might already exist: {}", e);
                            }
                        }

                        match backend.start_vm(&vm_name).await {
                            Ok(_) => {
                                info!("Successfully recovered VM: {}", vm_name);
                                (vm_name, Ok(true))
                            }
                            Err(e) => {
                                error!("Failed to recover VM '{}': {}", vm_name, e);
                                (vm_name, Err(e.to_string()))
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(recovery_futures).await;
            all_results.extend(results);
        }

        all_results
    }
}

/// Performance analysis
pub fn analyze_performance_impact() {
    println!("=== VM Recovery Performance Analysis ===\n");

    let vm_config_size = std::mem::size_of::<VmConfig>();
    let arc_size = std::mem::size_of::<Arc<VmConfig>>();

    println!("Memory usage for recovering 100 VMs:\n");

    println!("Before (cloning VmConfig):");
    println!("- VmConfig size: {} bytes", vm_config_size);
    println!("- Total cloned: {} bytes", vm_config_size * 100);
    println!("- Peak memory: {} KB", (vm_config_size * 100) / 1024);

    println!("\nAfter (Arc<VmConfig>):");
    println!("- Arc pointer size: {} bytes", arc_size);
    println!("- Total Arc clones: {} bytes", arc_size * 100);
    println!("- Actual configs: {} bytes (shared)", vm_config_size * 100);
    println!("- Peak memory: {} KB", ((arc_size * 100) + (vm_config_size * 100)) / 1024);

    println!("\nWith config caching (assuming 10 unique configs for 100 VMs):");
    println!("- Unique configs: {} bytes", vm_config_size * 10);
    println!("- Arc pointers: {} bytes", arc_size * 100);
    println!("- Peak memory: {} KB", ((arc_size * 100) + (vm_config_size * 10)) / 1024);

    println!("\nBenefits:");
    println!("- 90% reduction in config memory allocation");
    println!("- Faster recovery due to less allocation/deallocation");
    println!("- Better CPU cache utilization");
    println!("- Reduced GC pressure");
}

/// Mock VmBackend for testing
struct MockVmBackend;

#[async_trait::async_trait]
impl VmBackend for MockVmBackend {
    async fn create_vm(&self, config: &VmConfig, _node_id: u64) -> BlixardResult<()> {
        debug!("Mock creating VM: {}", config.name);
        Ok(())
    }

    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        debug!("Mock starting VM: {}", name);
        Ok(())
    }

    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        debug!("Mock stopping VM: {}", name);
        Ok(())
    }

    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        debug!("Mock deleting VM: {}", name);
        Ok(())
    }

    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        Ok(vec![])
    }

    async fn get_vm_status(&self, _name: &str) -> BlixardResult<Option<VmStatus>> {
        Ok(Some(VmStatus::Running))
    }

    async fn update_vm_status(&self, _name: &str, _status: VmStatus) -> BlixardResult<()> {
        Ok(())
    }

    async fn get_vm_ip(&self, _name: &str) -> BlixardResult<Option<String>> {
        Ok(Some("10.0.0.1".to_string()))
    }

    async fn migrate_vm(
        &self,
        _vm_name: &str,
        _target_node_id: u64,
        _live: bool,
    ) -> BlixardResult<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("VM Recovery Arc Refactoring Demo\n");

    // Create test data
    let test_vms: Vec<VmState> = (0..10)
        .map(|i| VmState {
            name: format!("vm-{}", i),
            config: VmConfig {
                name: format!("vm-{}", i),
                config_path: "/etc/vms/standard.nix".to_string(),
                vcpus: 2,
                memory: 2048,
                tenant_id: "tenant-1".to_string(),
                ..Default::default()
            },
            status: VmStatus::Stopped,
            node_id: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .collect();

    let backend: Arc<dyn VmBackend> = Arc::new(MockVmBackend);

    // Run performance analysis
    analyze_performance_impact();

    println!("\n=== Running recovery simulations ===\n");

    // Test original implementation
    println!("Running original implementation...");
    let start = std::time::Instant::now();
    let results1 = before_refactoring::recover_vms_original(
        test_vms.clone(),
        backend.clone(),
        5,
    ).await;
    let duration1 = start.elapsed();
    println!("Original: {} VMs recovered in {:?}", results1.len(), duration1);

    // Test optimized implementation
    println!("\nRunning Arc-optimized implementation...");
    let start = std::time::Instant::now();
    let results2 = after_refactoring::recover_vms_optimized(
        test_vms.clone(),
        backend.clone(),
        5,
    ).await;
    let duration2 = start.elapsed();
    println!("Optimized: {} VMs recovered in {:?}", results2.len(), duration2);

    // Test with config caching
    println!("\nRunning with config caching...");
    let start = std::time::Instant::now();
    let results3 = after_refactoring::recover_vms_with_cache(
        test_vms,
        backend,
        5,
    ).await;
    let duration3 = start.elapsed();
    println!("With cache: {} VMs recovered in {:?}", results3.len(), duration3);

    println!("\n=== Refactoring Guidelines ===\n");
    println!("1. Identify clone hotspots in async loops");
    println!("2. Wrap frequently cloned configs in Arc");
    println!("3. Consider caching for duplicate configurations");
    println!("4. Use Arc<dyn Trait> for shared service dependencies");
    println!("5. Measure impact with realistic workloads");

    Ok(())
}