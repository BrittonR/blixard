//! Demonstration of Arc refactoring to reduce clone() overhead
//!
//! This example shows how to refactor code to use Arc<T> for large structures
//! that are frequently cloned, reducing memory allocation and improving performance.

use blixard_core::{
    error::BlixardResult,
    types::{VmConfig, VmState, VmStatus, VmCommand},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Before: Expensive cloning of VmConfig
mod before {
    use super::*;

    pub struct VmManager {
        vms: HashMap<String, VmState>,
    }

    impl VmManager {
        pub async fn handle_command(&mut self, command: VmCommand) -> BlixardResult<()> {
            match command {
                VmCommand::Create { config, node_id } => {
                    // Expensive clone #1: Cloning for async task
                    let config_clone = config.clone();
                    tokio::spawn(async move {
                        validate_vm_config(&config_clone).await;
                    });

                    // Expensive clone #2: Storing in state
                    let vm_state = VmState {
                        name: config.name.clone(),
                        config: config.clone(), // Full config clone
                        status: VmStatus::Creating,
                        node_id,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };

                    // Expensive clone #3: Logging/metrics
                    let config_for_metrics = config.clone();
                    record_vm_creation_metrics(&config_for_metrics);

                    self.vms.insert(config.name.clone(), vm_state);
                }
                _ => {}
            }
            Ok(())
        }

        pub fn get_vm_configs(&self) -> Vec<VmConfig> {
            // Expensive: Cloning all configs
            self.vms
                .values()
                .map(|state| state.config.clone())
                .collect()
        }
    }

    async fn validate_vm_config(config: &VmConfig) {
        println!("Validating VM: {}", config.name);
    }

    fn record_vm_creation_metrics(config: &VmConfig) {
        println!("Recording metrics for VM: {}", config.name);
    }
}

/// After: Efficient Arc-based sharing
mod after {
    use super::*;

    /// VmState with Arc-wrapped config for cheap cloning
    pub struct VmStateArc {
        pub name: String,
        pub config: Arc<VmConfig>, // Arc wrapper for cheap clones
        pub status: VmStatus,
        pub node_id: u64,
        pub created_at: chrono::DateTime<chrono::Utc>,
        pub updated_at: chrono::DateTime<chrono::Utc>,
    }

    pub struct VmManager {
        vms: Arc<RwLock<HashMap<String, VmStateArc>>>,
    }

    impl VmManager {
        pub fn new() -> Self {
            Self {
                vms: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        pub async fn handle_command(&self, command: VmCommand) -> BlixardResult<()> {
            match command {
                VmCommand::Create { config, node_id } => {
                    // Wrap config in Arc once
                    let config = Arc::new(config);

                    // Cheap clone #1: Arc clone for async task
                    let config_clone = config.clone();
                    tokio::spawn(async move {
                        validate_vm_config(&config_clone).await;
                    });

                    // No clone needed: Create state with Arc
                    let vm_state = VmStateArc {
                        name: config.name.clone(),
                        config: config.clone(), // Cheap Arc clone
                        status: VmStatus::Creating,
                        node_id,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };

                    // Cheap clone #2: Arc clone for metrics
                    let config_for_metrics = config.clone();
                    tokio::spawn(async move {
                        record_vm_creation_metrics(&config_for_metrics).await;
                    });

                    let mut vms = self.vms.write().await;
                    vms.insert(config.name.clone(), vm_state);
                }
                _ => {}
            }
            Ok(())
        }

        pub async fn get_vm_configs(&self) -> Vec<Arc<VmConfig>> {
            // Efficient: Only cloning Arc pointers
            let vms = self.vms.read().await;
            vms.values()
                .map(|state| state.config.clone()) // Cheap Arc clone
                .collect()
        }

        /// Example of batch operations with Arc
        pub async fn update_multiple_vms<F>(&self, filter: F) -> BlixardResult<()>
        where
            F: Fn(&VmConfig) -> bool,
        {
            let vms = self.vms.read().await;
            let matching_configs: Vec<Arc<VmConfig>> = vms
                .values()
                .filter(|state| filter(&state.config))
                .map(|state| state.config.clone()) // Cheap Arc clones
                .collect();

            // Process matching VMs without holding the lock
            drop(vms);

            for config in matching_configs {
                // Each task gets a cheap Arc clone
                let config_clone = config.clone();
                tokio::spawn(async move {
                    process_vm_update(&config_clone).await;
                });
            }

            Ok(())
        }
    }

    async fn validate_vm_config(config: &VmConfig) {
        println!("Validating VM: {}", config.name);
    }

    async fn record_vm_creation_metrics(config: &VmConfig) {
        println!("Recording metrics for VM: {}", config.name);
    }

    async fn process_vm_update(config: &VmConfig) {
        println!("Processing update for VM: {}", config.name);
    }
}

/// Pattern: Arc wrapper type for convenience
pub struct ArcVmConfig(Arc<VmConfig>);

impl ArcVmConfig {
    pub fn new(config: VmConfig) -> Self {
        Self(Arc::new(config))
    }

    pub fn clone_ref(&self) -> Arc<VmConfig> {
        self.0.clone()
    }
}

impl std::ops::Deref for ArcVmConfig {
    type Target = VmConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Performance comparison
pub fn show_performance_impact() {
    println!("=== Performance Impact of Arc Refactoring ===\n");
    
    let vm_config_size = std::mem::size_of::<VmConfig>();
    let arc_size = std::mem::size_of::<Arc<VmConfig>>();
    
    println!("VmConfig size: {} bytes", vm_config_size);
    println!("Arc<VmConfig> size: {} bytes (just a pointer)", arc_size);
    println!();
    
    println!("Clone costs:");
    println!("- VmConfig clone: {} bytes allocated + deep copy of all fields", vm_config_size);
    println!("- Arc<VmConfig> clone: {} bytes + atomic increment (negligible)", arc_size);
    println!();
    
    println!("For 100 clones:");
    println!("- Before: {} bytes allocated", vm_config_size * 100);
    println!("- After: {} bytes (original) + {} bytes (Arc pointers)", vm_config_size, arc_size * 100);
    println!();
    
    println!("Memory savings: ~{}%", ((vm_config_size * 99) * 100) / (vm_config_size * 100));
    println!();
    
    println!("Additional benefits:");
    println!("- No deep copying of strings and HashMaps");
    println!("- Atomic reference counting (thread-safe)");
    println!("- Cache-friendly (less memory allocation/deallocation)");
    println!("- Reduced GC pressure");
}

/// Common Arc patterns in the codebase
pub mod arc_patterns {
    use super::*;

    /// Pattern 1: Service with Arc dependencies
    pub struct VmService {
        node_state: Arc<RwLock<HashMap<String, String>>>,
        config: Arc<VmConfig>,
    }

    impl VmService {
        pub fn new(config: VmConfig) -> Self {
            Self {
                node_state: Arc::new(RwLock::new(HashMap::new())),
                config: Arc::new(config),
            }
        }

        /// Spawn multiple tasks with cheap clones
        pub async fn process_in_parallel(&self) {
            for i in 0..10 {
                let state = self.node_state.clone(); // Cheap
                let config = self.config.clone(); // Cheap
                
                tokio::spawn(async move {
                    println!("Task {} processing VM: {}", i, config.name);
                    let _state = state.read().await;
                    // Process...
                });
            }
        }
    }

    /// Pattern 2: Builder with Arc result
    pub struct VmConfigBuilder {
        config: VmConfig,
    }

    impl VmConfigBuilder {
        pub fn new(name: String) -> Self {
            Self {
                config: VmConfig {
                    name,
                    ..Default::default()
                },
            }
        }

        pub fn vcpus(mut self, vcpus: u32) -> Self {
            self.config.vcpus = vcpus;
            self
        }

        pub fn memory(mut self, memory: u32) -> Self {
            self.config.memory = memory;
            self
        }

        /// Build into Arc for efficient sharing
        pub fn build_arc(self) -> Arc<VmConfig> {
            Arc::new(self.config)
        }
    }

    /// Pattern 3: Cache with Arc values
    pub struct VmConfigCache {
        cache: Arc<RwLock<HashMap<String, Arc<VmConfig>>>>,
    }

    impl VmConfigCache {
        pub async fn get_or_load(&self, name: &str) -> Arc<VmConfig> {
            {
                let cache = self.cache.read().await;
                if let Some(config) = cache.get(name) {
                    return config.clone(); // Cheap Arc clone
                }
            }

            // Load from storage (expensive)
            let config = load_vm_config(name).await;
            let config = Arc::new(config);

            let mut cache = self.cache.write().await;
            cache.insert(name.to_string(), config.clone());

            config
        }
    }

    async fn load_vm_config(name: &str) -> VmConfig {
        // Simulate loading from storage
        VmConfig {
            name: name.to_string(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("Arc Refactoring Demo\n");
    
    // Show performance impact
    show_performance_impact();
    
    println!("\n=== Refactoring Guidelines ===\n");
    
    println!("1. Identify large, frequently cloned types:");
    println!("   - VmConfig (~200+ bytes)");
    println!("   - NodeConfig");
    println!("   - PeerInfo with NodeAddr");
    println!("   - HashMap<String, T> collections");
    println!();
    
    println!("2. Wrap in Arc when:");
    println!("   - Sharing across async tasks");
    println!("   - Storing in multiple collections");
    println!("   - Passing through service layers");
    println!("   - Caching frequently accessed data");
    println!();
    
    println!("3. Keep direct ownership when:");
    println!("   - Single owner with mutable access");
    println!("   - Short-lived stack variables");
    println!("   - Performance-critical tight loops");
    println!();
    
    println!("4. Use Arc<RwLock<T>> for:");
    println!("   - Shared mutable state");
    println!("   - Collections that need concurrent access");
    println!("   - Configuration that can be updated");
    
    Ok(())
}