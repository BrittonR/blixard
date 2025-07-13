//! Simple demo of zero-cost abstractions without complex error handling
//!
//! This module provides a simplified demonstration that doesn't depend
//! on the full Blixard error system.

use super::{
    type_state::vm_lifecycle::*,
    validated_types::{Positive, BoundedInt},
    phantom_types::{units::*, environment::*},
    zero_alloc_patterns::{StackString, InlineVec, StaticPool},
    const_collections::{ConstMap, ConstSet},
};
use crate::types::VmConfig;

/// Simple VM configuration with validated types
#[derive(Debug, Clone)]
pub struct SimpleVmConfig {
    /// VM name (stack allocated)
    pub name: StackString<64>,
    /// CPU cores (must be positive)
    pub vcpus: Positive<u32>,
    /// Memory in MB (bounded between 64MB and 64GB)
    pub memory_mb: BoundedInt<64, 65536>,
    /// Disk size in GB (must be positive)  
    pub disk_gb: Positive<u64>,
    /// Priority level (0-255)
    pub priority: BoundedInt<0, 255>,
}

impl SimpleVmConfig {
    /// Create a new validated VM configuration
    pub fn new(
        name: &str,
        vcpus: u32,
        memory_mb: i64,
        disk_gb: u64,
        priority: i64,
    ) -> Result<Self, &'static str> {
        let name = StackString::from_str(name)?;
        let vcpus = Positive::<u32>::new(vcpus)?;
        let memory_mb = BoundedInt::new(memory_mb)?;
        let disk_gb = Positive::<u64>::new(disk_gb)?;
        let priority = BoundedInt::new(priority)?;

        Ok(Self {
            name,
            vcpus,
            memory_mb,
            disk_gb,
            priority,
        })
    }

    /// Get resource measurements with unit safety
    pub fn resource_measurements(&self) -> SimpleResourceMeasurements {
        SimpleResourceMeasurements {
            memory: Measurement::new(self.memory_mb.get() as u64),
            storage: Measurement::new(self.disk_gb.get()),
            cpu_cores: self.vcpus.get(),
        }
    }
}

/// Resource measurements with phantom types for unit safety
#[derive(Debug, Clone)]
pub struct SimpleResourceMeasurements {
    pub memory: Measurement<u64, Megabytes>,
    pub storage: Measurement<u64, super::phantom_types::units::Gigabytes>,
    pub cpu_cores: u32,
}

impl SimpleResourceMeasurements {
    /// Check if resources fit within limits
    pub fn fits_within(&self, limits: &SimpleResourceLimits) -> bool {
        self.memory.value() <= limits.max_memory.value() &&
        self.storage.value() <= limits.max_storage.value() &&
        self.cpu_cores <= limits.max_cpu_cores
    }
}

/// Resource limits with typed measurements
#[derive(Debug, Clone)]
pub struct SimpleResourceLimits {
    pub max_memory: Measurement<u64, Megabytes>,
    pub max_storage: Measurement<u64, super::phantom_types::units::Gigabytes>,
    pub max_cpu_cores: u32,
}

// Use the branded VM identifiers from phantom_types module
// These are properly implemented with the sealed trait pattern

/// VM placement strategies (compile-time lookup) - pre-sorted by key
pub const PLACEMENT_STRATEGIES: ConstMap<&str, u8, 5> = ConstMap::new([
    ("least_available", 1),
    ("locality_aware", 4),
    ("most_available", 0),
    ("priority_based", 3),
    ("round_robin", 2),
]);

/// VM resource types (compile-time set) - pre-sorted
pub const VM_RESOURCE_TYPES: ConstSet<&str, 4> = ConstSet::new([
    "cpu",
    "memory", 
    "network",
    "storage",
]);

/// Zero-allocation VM pool
pub struct SimpleVmPool {
    pool: StaticPool<SimpleVm, 64>,
}

#[derive(Debug)]
pub struct SimpleVm {
    pub id: u64,
    pub name: StackString<64>,
    pub vcpus: u32,
    pub memory_mb: u64,
}

impl SimpleVmPool {
    /// Create a new VM pool
    pub const fn new() -> Self {
        Self {
            pool: StaticPool::new(),
        }
    }

    /// Try to acquire a VM from the pool
    pub fn acquire_vm(&mut self, id: u64, name: &str, vcpus: u32, memory_mb: u64) -> Result<super::zero_alloc_patterns::PooledObject<SimpleVm, 64>, &'static str> {
        let vm_name = StackString::from_str(name)?;
        
        self.pool.try_acquire(|| SimpleVm {
            id,
            name: vm_name,
            vcpus,
            memory_mb,
        }).ok_or("Pool exhausted")
    }

    /// Get pool statistics
    pub fn stats(&self) -> super::zero_alloc_patterns::PoolStats {
        self.pool.stats()
    }
}

/// Demonstration functions
pub fn demonstrate_type_state() -> Result<(), &'static str> {
    println!("=== Type-State Pattern Demo ===");
    
    // VM lifecycle with compile-time state verification
    let vm = Vm::new("demo-vm".to_string(), VmConfig::default());
    println!("VM created: {} (state: {})", vm.id(), vm.state_name());

    let vm = vm.configure();
    println!("VM configured: {} (state: {})", vm.id(), vm.state_name());

    let vm = vm.start();
    println!("VM starting: {} (state: {})", vm.id(), vm.state_name());

    let vm = vm.started();
    println!("VM running: {} (state: {})", vm.id(), vm.state_name());

    // This would not compile - invalid transition:
    // let vm = vm.configure(); // Error!
    
    Ok(())
}

pub fn demonstrate_validated_types() -> Result<(), &'static str> {
    println!("=== Validated Types Demo ===");

    // Create validated VM configuration
    let config = SimpleVmConfig::new("web-server", 4, 2048, 100, 5)?;
    println!("Created VM config: {:?}", config);

    // Get measurements with unit safety
    let measurements = config.resource_measurements();
    println!("Memory in bytes: {}", measurements.memory.to_bytes().value());
    println!("Storage in bytes: {}", measurements.storage.to_bytes().value());

    // Check resource limits
    let limits = SimpleResourceLimits {
        max_memory: Measurement::new(8192),
        max_storage: Measurement::new(1000),
        max_cpu_cores: 16,
    };

    // Create fresh measurements for comparison (since to_bytes() consumes the value)
    let fresh_measurements = config.resource_measurements();
    println!("Fits within limits: {}", fresh_measurements.fits_within(&limits));

    Ok(())
}

pub fn demonstrate_phantom_types() -> Result<(), &'static str> {
    println!("=== Phantom Types Demo ===");

    // Unit-safe measurements
    let memory_mb = Measurement::<u64, Megabytes>::new(2048);
    let memory_bytes_measurement = memory_mb.to_bytes();
    let memory_bytes = memory_bytes_measurement.value();
    let memory_gb = memory_bytes / (1024 * 1024 * 1024);
    
    println!("Memory: {}MB = {} bytes = {}GB", memory_mb.value(), memory_bytes, memory_gb);

    // Branded identifiers
    let vm_id = 12345;
    let prod_id = ProductionVmId::new(vm_id);
    let dev_id = DevelopmentVmId::new(vm_id);

    println!("Production VM: {} (brand: {})", prod_id.as_inner(), prod_id.brand_name());
    println!("Development VM: {} (brand: {})", dev_id.as_inner(), dev_id.brand_name());

    Ok(())
}

pub fn demonstrate_zero_allocation() -> Result<(), &'static str> {
    println!("=== Zero-Allocation Patterns Demo ===");

    // Stack-allocated string
    let mut vm_name = StackString::<32>::new();
    vm_name.push_str("vm-")?;
    vm_name.push_str("001")?;
    println!("VM name: {}", vm_name);

    // Inline vector with fixed capacity  
    let mut vm_ids = InlineVec::<u64, 8>::new();
    vm_ids.push(1001).map_err(|_| "Vector full")?;
    vm_ids.push(1002).map_err(|_| "Vector full")?;
    vm_ids.push(1003).map_err(|_| "Vector full")?;
    println!("VM IDs: {:?}", vm_ids.as_slice());

    // Object pool for resource reuse
    let mut pool = SimpleVmPool::new();
    
    {
        let vm = pool.acquire_vm(1, "test-vm", 2, 1024)?;
        println!("Acquired VM: {:?}", *vm);
    } // VM automatically returned to pool

    println!("Pool stats: {:?}", pool.stats());

    Ok(())
}

pub fn demonstrate_const_collections() -> Result<(), &'static str> {
    println!("=== Const Collections Demo ===");

    // Compile-time map lookup
    let strategy_id = PLACEMENT_STRATEGIES.get("round_robin");
    println!("Round robin strategy ID: {:?}", strategy_id);

    // Compile-time set membership
    let has_cpu = VM_RESOURCE_TYPES.contains("cpu");
    let has_gpu = VM_RESOURCE_TYPES.contains("gpu");
    println!("Has CPU resource: {}, Has GPU resource: {}", has_cpu, has_gpu);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_vm_config() {
        let config = SimpleVmConfig::new("test", 2, 1024, 50, 3).unwrap();
        assert_eq!(config.name.as_str(), "test");
        assert_eq!(*config.vcpus.get(), 2);
        assert_eq!(*config.memory_mb.get(), 1024);
    }

    #[test]
    fn test_resource_measurements() {
        let config = SimpleVmConfig::new("test", 4, 2048, 100, 5).unwrap();
        let measurements = config.resource_measurements();
        
        let limits = SimpleResourceLimits {
            max_memory: Measurement::new(4096),
            max_storage: Measurement::new(500),
            max_cpu_cores: 8,
        };

        assert!(measurements.fits_within(&limits));
    }

    #[test]
    fn test_branded_types() {
        let vm_id = 123;
        let prod_id = ProductionVmId::new(vm_id);
        let dev_id = DevelopmentVmId::new(vm_id);

        assert_eq!(*prod_id.as_inner(), vm_id);
        assert_eq!(*dev_id.as_inner(), vm_id);
        assert_eq!(prod_id.brand_name(), "Production");
        assert_eq!(dev_id.brand_name(), "Development");
    }

    #[test]
    fn test_vm_pool() {
        let mut pool = SimpleVmPool::new();
        
        {
            let _vm = pool.acquire_vm(1, "test", 2, 1024).unwrap();
            let stats = pool.stats();
            assert_eq!(stats.in_use, 1);
        }
        
        let stats = pool.stats();
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.available, 64);
    }

    #[test]
    fn test_const_collections() {
        assert_eq!(PLACEMENT_STRATEGIES.get("round_robin"), Some(2));
        assert_eq!(PLACEMENT_STRATEGIES.get("invalid"), None);
        
        assert!(VM_RESOURCE_TYPES.contains("memory"));
        assert!(!VM_RESOURCE_TYPES.contains("gpu"));
    }
}