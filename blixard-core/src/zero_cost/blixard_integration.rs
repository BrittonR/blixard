//! Integration of zero-cost abstractions with Blixard core types
//!
//! This module shows how to apply zero-cost patterns to improve
//! the existing Blixard codebase while maintaining compatibility.

use crate::types::{VmConfig, VmId, VmStatus};
use crate::error::{BlixardError, BlixardResult};
use super::{
    type_state::{vm_lifecycle::*, node_lifecycle::*},
    validated_types::{Positive, BoundedInt, NonEmpty},
    phantom_types::{units::*, Branded, Brand},
    static_dispatch::StaticDispatch,
    zero_alloc_patterns::{StackString, InlineVec, StaticPool},
    const_collections::ConstMap,
};
use std::marker::PhantomData;
use std::sync::Arc;

/// Enhanced VM configuration with validated types
#[derive(Debug, Clone)]
pub struct ValidatedVmConfig {
    /// VM name (validated identifier)
    pub name: StackString<64>,
    /// CPU cores (must be positive)
    pub vcpus: Positive<u32>,
    /// Memory in MB (bounded between 64MB and 64GB)
    pub memory_mb: BoundedInt<64, 65536>,
    /// Disk size in GB (must be positive)
    pub disk_gb: Positive<u64>,
    /// Priority level (0-255)
    pub priority: BoundedInt<0, 255>,
    /// Additional features (zero-alloc list)
    pub features: InlineVec<StackString<32>, 8>,
}

impl ValidatedVmConfig {
    /// Create a new validated VM configuration
    pub fn new(
        name: &str,
        vcpus: u32,
        memory_mb: i64,
        disk_gb: u64,
        priority: i64,
    ) -> BlixardResult<Self> {
        let name = StackString::from_str(name)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid VM name: {}", e) 
            })?;

        let vcpus = Positive::new(vcpus)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid vCPU count: {}", e) 
            })?;

        let memory_mb = BoundedInt::new(memory_mb)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid memory size: {}", e) 
            })?;

        let disk_gb = Positive::new(disk_gb)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid disk size: {}", e) 
            })?;

        let priority = BoundedInt::new(priority)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid priority: {}", e) 
            })?;

        Ok(Self {
            name,
            vcpus,
            memory_mb,
            disk_gb,
            priority,
            features: InlineVec::new(),
        })
    }

    /// Add a feature (zero allocation)
    pub fn add_feature(&mut self, feature: &str) -> BlixardResult<()> {
        let feature_str = StackString::from_str(feature)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: format!("Invalid feature name: {}", e) 
            })?;

        self.features.push(feature_str)
            .map_err(|_| BlixardError::ConfigurationError { 
                component: "ValidatedVmConfig".to_string(),
                message: "Too many features (max 8)".to_string() 
            })?;

        Ok(())
    }

    /// Convert to legacy VmConfig
    pub fn to_vm_config(&self) -> VmConfig {
        VmConfig {
            name: self.name.as_str().to_string(),
            vcpus: *self.vcpus.get(),
            memory_mb: *self.memory_mb.get() as u64,
            disk_size_gb: Some(*self.disk_gb.get()),
            // Convert other fields as needed
            ..Default::default()
        }
    }

    /// Get resource measurements with unit safety
    pub fn resource_measurements(&self) -> ResourceMeasurements {
        ResourceMeasurements {
            memory: Measurement::new(*self.memory_mb.get() as u64),
            storage: Measurement::new(*self.disk_gb.get()),
            cpu_cores: *self.vcpus.get(),
        }
    }
}

/// Resource measurements with phantom types for unit safety
#[derive(Debug, Clone)]
pub struct ResourceMeasurements {
    pub memory: Measurement<u64, super::phantom_types::units::Megabytes>,
    pub storage: Measurement<u64, super::phantom_types::units::Gigabytes>,
    pub cpu_cores: u32,
}

impl ResourceMeasurements {
    /// Check if resources fit within limits
    pub fn fits_within(&self, limits: &ResourceLimits) -> bool {
        self.memory.value() <= limits.max_memory.value() &&
        self.storage.value() <= limits.max_storage.value() &&
        self.cpu_cores <= limits.max_cpu_cores
    }

    /// Calculate resource utilization percentage
    pub fn utilization_percent(&self, total: &ResourceMeasurements) -> ResourceUtilization {
        let memory_percent = (*self.memory.value() * 100) / *total.memory.value();
        let storage_percent = (*self.storage.value() * 100) / *total.storage.value();
        let cpu_percent = (self.cpu_cores * 100) / total.cpu_cores;

        ResourceUtilization {
            memory_percent: BoundedInt::clamp(memory_percent as i64),
            storage_percent: BoundedInt::clamp(storage_percent as i64),
            cpu_percent: BoundedInt::clamp(cpu_percent as i64),
        }
    }
}

/// Resource limits with typed measurements
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory: Measurement<u64, super::phantom_types::units::Megabytes>,
    pub max_storage: Measurement<u64, super::phantom_types::units::Gigabytes>,
    pub max_cpu_cores: u32,
}

/// Resource utilization as percentages
#[derive(Debug, Clone)]
pub struct ResourceUtilization {
    pub memory_percent: BoundedInt<0, 100>,
    pub storage_percent: BoundedInt<0, 100>,
    pub cpu_percent: BoundedInt<0, 100>,
}

/// Enhanced VM manager with type-state pattern
pub struct TypedVmManager<S: super::type_state::TypeState> {
    state: PhantomData<S>,
    vm_pool: StaticPool<TypedVm, 256>,
    config_pool: StaticPool<ValidatedVmConfig, 256>,
}

/// VM instance with type-state tracking
#[derive(Debug)]
pub struct TypedVm {
    pub id: VmId,
    pub config: Arc<ValidatedVmConfig>,
    pub created_at: std::time::SystemTime,
}

// Define manager states
pub struct Uninitialized;
pub struct Initialized;
pub struct Running;

impl super::type_state::private::Sealed for Uninitialized {}
impl super::type_state::TypeState for Uninitialized {
    const STATE_NAME: &'static str = "Uninitialized";
}

impl super::type_state::private::Sealed for Initialized {}
impl super::type_state::TypeState for Initialized {
    const STATE_NAME: &'static str = "Initialized";
}

impl super::type_state::private::Sealed for Running {}
impl super::type_state::TypeState for Running {
    const STATE_NAME: &'static str = "Running";
}

impl TypedVmManager<Uninitialized> {
    /// Create a new VM manager
    pub fn new() -> Self {
        Self {
            state: PhantomData,
            vm_pool: StaticPool::new(),
            config_pool: StaticPool::new(),
        }
    }

    /// Initialize the manager
    pub fn initialize(self) -> BlixardResult<TypedVmManager<Initialized>> {
        // Perform initialization
        Ok(TypedVmManager {
            state: PhantomData,
            vm_pool: self.vm_pool,
            config_pool: self.config_pool,
        })
    }
}

impl TypedVmManager<Initialized> {
    /// Start the manager
    pub fn start(self) -> BlixardResult<TypedVmManager<Running>> {
        // Start background tasks
        Ok(TypedVmManager {
            state: PhantomData,
            vm_pool: self.vm_pool,
            config_pool: self.config_pool,
        })
    }
}

impl TypedVmManager<Running> {
    /// Create a new VM with validation
    pub fn create_vm(&mut self, config: ValidatedVmConfig) -> BlixardResult<VmId> {
        let vm_id = VmId::new();

        // Try to get a VM from the pool
        let mut vm = self.vm_pool.try_acquire(|| TypedVm {
            id: vm_id,
            config: Arc::new(config.clone()),
            created_at: std::time::SystemTime::now(),
        }).ok_or_else(|| BlixardError::ResourceExhausted { 
            component: "VmManager".to_string(),
            resource: "VM pool".to_string(),
            limit: 256 
        })?;

        // Update the VM with new data
        vm.id = vm_id;
        vm.config = Arc::new(config);
        vm.created_at = std::time::SystemTime::now();

        Ok(vm_id)
    }

    /// Get manager statistics
    pub fn stats(&self) -> ManagerStats {
        let vm_stats = self.vm_pool.stats();
        let config_stats = self.config_pool.stats();

        ManagerStats {
            vms_active: vm_stats.in_use,
            vms_capacity: vm_stats.total_capacity,
            configs_cached: config_stats.initialized,
            memory_efficiency: ((vm_stats.in_use as f64 / vm_stats.total_capacity as f64) * 100.0) as u8,
        }
    }
}

/// Manager statistics
#[derive(Debug)]
pub struct ManagerStats {
    pub vms_active: usize,
    pub vms_capacity: usize,
    pub configs_cached: usize,
    pub memory_efficiency: u8,
}

/// Branded VM identifiers for different contexts
pub struct Production;
pub struct Development;
pub struct Testing;

impl super::phantom_types::private::Sealed for Production {}
impl Brand for Production {
    const BRAND: &'static str = "Production";
}

impl super::phantom_types::private::Sealed for Development {}
impl Brand for Development {
    const BRAND: &'static str = "Development";
}

impl super::phantom_types::private::Sealed for Testing {}
impl Brand for Testing {
    const BRAND: &'static str = "Testing";
}

pub type ProductionVmId = Branded<VmId, Production>;
pub type DevelopmentVmId = Branded<VmId, Development>;
pub type TestingVmId = Branded<VmId, Testing>;

/// VM placement strategy with compile-time optimization
pub const PLACEMENT_STRATEGIES: ConstMap<&str, u8, 5> = ConstMap::from_unsorted([
    ("most_available", 0),
    ("least_available", 1),
    ("round_robin", 2),
    ("priority_based", 3),
    ("locality_aware", 4),
]);

/// Node capability flags (compile-time lookup)
pub const NODE_CAPABILITIES: super::const_collections::StaticLookup<&'static str, 64> = 
    super::const_collections::StaticLookup::with_entries([
        (0b0001, "gpu"),
        (0b0010, "high_memory"),
        (0b0100, "nvme_storage"),
        (0b1000, "infiniband"),
        (0b0011, "gpu_high_memory"),
        (0b0101, "gpu_nvme"),
        (0b1001, "gpu_infiniband"),
        (0b1111, "full_featured"),
    ]);

/// Zero-cost VM scheduler with static dispatch
pub trait VmSchedulerBackend {
    type PlacementResult;
    type Error;

    fn schedule_vm(
        &self,
        config: &ValidatedVmConfig,
        constraints: &SchedulingConstraints,
    ) -> Result<Self::PlacementResult, Self::Error>;
}

/// Scheduling constraints with validated types
#[derive(Debug)]
pub struct SchedulingConstraints {
    pub preferred_nodes: InlineVec<u64, 8>,
    pub required_capabilities: u64, // Bitmask
    pub max_latency_ms: BoundedInt<1, 10000>,
    pub anti_affinity_groups: InlineVec<StackString<32>, 4>,
}

impl SchedulingConstraints {
    /// Create new constraints with validation
    pub fn new(max_latency_ms: i64) -> BlixardResult<Self> {
        let max_latency = BoundedInt::new(max_latency_ms)
            .map_err(|e| BlixardError::ConfigurationError { 
                component: "SchedulingConstraints".to_string(),
                message: format!("Invalid latency constraint: {}", e) 
            })?;

        Ok(Self {
            preferred_nodes: InlineVec::new(),
            required_capabilities: 0,
            max_latency_ms: max_latency,
            anti_affinity_groups: InlineVec::new(),
        })
    }

    /// Add a preferred node
    pub fn add_preferred_node(&mut self, node_id: u64) -> BlixardResult<()> {
        self.preferred_nodes.push(node_id)
            .map_err(|_| BlixardError::ConfigurationError { 
                component: "SchedulingConstraints".to_string(),
                message: "Too many preferred nodes (max 8)".to_string() 
            })?;
        Ok(())
    }

    /// Add required capability
    pub fn add_capability(&mut self, capability: &str) -> BlixardResult<()> {
        // Simplified capability mapping for demo
        match capability {
            "gpu" => self.required_capabilities |= 0b0001,
            "high_memory" => self.required_capabilities |= 0b0010,
            "nvme_storage" => self.required_capabilities |= 0b0100,
            "infiniband" => self.required_capabilities |= 0b1000,
            _ => return Err(BlixardError::ConfigurationError { 
                component: "SchedulingConstraints".to_string(),
                message: format!("Unknown capability: {}", capability) 
            }),
        }
        Ok(())
    }

    /// Check if node matches capabilities
    pub fn node_matches_capabilities(&self, node_capabilities: u64) -> bool {
        (node_capabilities & self.required_capabilities) == self.required_capabilities
    }
}

/// Performance monitoring with zero-allocation counters
pub struct PerformanceCounters {
    vm_creation_count: std::sync::atomic::AtomicU64,
    vm_start_time_ns: std::sync::atomic::AtomicU64,
    scheduler_decisions: std::sync::atomic::AtomicU64,
    resource_allocation_failures: std::sync::atomic::AtomicU64,
}

impl PerformanceCounters {
    pub const fn new() -> Self {
        Self {
            vm_creation_count: std::sync::atomic::AtomicU64::new(0),
            vm_start_time_ns: std::sync::atomic::AtomicU64::new(0),
            scheduler_decisions: std::sync::atomic::AtomicU64::new(0),
            resource_allocation_failures: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Record VM creation (zero allocation)
    pub fn record_vm_creation(&self, start_time_ns: u64) {
        self.vm_creation_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.vm_start_time_ns.fetch_add(start_time_ns, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get average VM start time
    pub fn average_vm_start_time_ns(&self) -> u64 {
        let total_time = self.vm_start_time_ns.load(std::sync::atomic::Ordering::Relaxed);
        let count = self.vm_creation_count.load(std::sync::atomic::Ordering::Relaxed);
        if count == 0 { 0 } else { total_time / count }
    }

    /// Get performance metrics
    pub fn get_metrics(&self) -> PerformanceMetrics {
        PerformanceMetrics {
            vm_creation_count: self.vm_creation_count.load(std::sync::atomic::Ordering::Relaxed),
            average_start_time_ns: self.average_vm_start_time_ns(),
            scheduler_decisions: self.scheduler_decisions.load(std::sync::atomic::Ordering::Relaxed),
            allocation_failures: self.resource_allocation_failures.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PerformanceMetrics {
    pub vm_creation_count: u64,
    pub average_start_time_ns: u64,
    pub scheduler_decisions: u64,
    pub allocation_failures: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validated_vm_config() {
        let mut config = ValidatedVmConfig::new(
            "test-vm",
            4,      // 4 vCPUs
            2048,   // 2GB RAM
            100,    // 100GB disk
            5,      // Priority 5
        ).unwrap();

        assert_eq!(config.name.as_str(), "test-vm");
        assert_eq!(*config.vcpus.get(), 4);
        assert_eq!(*config.memory_mb.get(), 2048);

        config.add_feature("gpu").unwrap();
        config.add_feature("nvme").unwrap();

        assert_eq!(config.features.len(), 2);
    }

    #[test]
    fn test_resource_measurements() {
        let measurements = ResourceMeasurements {
            memory: Measurement::new(2048),
            storage: Measurement::new(100),
            cpu_cores: 4,
        };

        let limits = ResourceLimits {
            max_memory: Measurement::new(4096),
            max_storage: Measurement::new(500),
            max_cpu_cores: 8,
        };

        assert!(measurements.fits_within(&limits));

        let total = ResourceMeasurements {
            memory: Measurement::new(8192),
            storage: Measurement::new(1000),
            cpu_cores: 16,
        };

        let utilization = measurements.utilization_percent(&total);
        assert_eq!(*utilization.memory_percent.get(), 25);
        assert_eq!(*utilization.cpu_percent.get(), 25);
    }

    #[test]
    fn test_typed_vm_manager() {
        let manager = TypedVmManager::new();
        let manager = manager.initialize().unwrap();
        let mut manager = manager.start().unwrap();

        let config = ValidatedVmConfig::new("test", 2, 1024, 50, 3).unwrap();
        let vm_id = manager.create_vm(config).unwrap();

        assert!(!vm_id.to_string().is_empty());

        let stats = manager.stats();
        assert_eq!(stats.vms_active, 1);
        assert_eq!(stats.vms_capacity, 256);
    }

    #[test]
    fn test_scheduling_constraints() {
        let mut constraints = SchedulingConstraints::new(1000).unwrap();
        
        constraints.add_preferred_node(1).unwrap();
        constraints.add_preferred_node(2).unwrap();
        
        constraints.add_capability("gpu").unwrap();
        constraints.add_capability("high_memory").unwrap();

        // Node with gpu (0b0001) and high_memory (0b0010) = 0b0011
        assert!(constraints.node_matches_capabilities(0b0011));
        
        // Node with only gpu (0b0001)
        assert!(!constraints.node_matches_capabilities(0b0001));
    }

    #[test]
    fn test_performance_counters() {
        let counters = PerformanceCounters::new();
        
        counters.record_vm_creation(1_000_000); // 1ms
        counters.record_vm_creation(2_000_000); // 2ms
        
        let metrics = counters.get_metrics();
        assert_eq!(metrics.vm_creation_count, 2);
        assert_eq!(metrics.average_start_time_ns, 1_500_000); // 1.5ms average
    }

    #[test]
    fn test_branded_vm_ids() {
        let vm_id = VmId::new();
        
        let prod_id = ProductionVmId::new(vm_id);
        let dev_id = DevelopmentVmId::new(vm_id);

        assert_eq!(prod_id.brand_name(), "Production");
        assert_eq!(dev_id.brand_name(), "Development");

        // These would not compile if we tried to mix them:
        // let mixed = production_function(dev_id); // Error: expected ProductionVmId
    }

    #[test]
    fn test_const_collections() {
        assert_eq!(PLACEMENT_STRATEGIES.get("round_robin"), Some(2));
        assert_eq!(PLACEMENT_STRATEGIES.get("invalid"), None);

        assert_eq!(NODE_CAPABILITIES.get(0b0001), Some("gpu"));
        assert_eq!(NODE_CAPABILITIES.get(0b1111), Some("full_featured"));
        assert_eq!(NODE_CAPABILITIES.get(0b0000), None);
    }
}