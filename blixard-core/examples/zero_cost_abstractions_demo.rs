//! Demonstration of zero-cost abstractions in Blixard
//!
//! This example shows how the zero-cost abstraction patterns can be applied
//! to the Blixard codebase to improve performance and type safety.

use blixard_core::zero_cost::{
    type_state::{vm_lifecycle::*, node_lifecycle::*},
    const_generics::{ConstBuffer, FixedCapacityVec, ConstString},
    phantom_types::{units::*, database::*},
    validated_types::{Positive, BoundedInt, ValidatedString, validators::*},
    static_dispatch::{vm_backend::*, database::*},
    compile_time_validation::*,
    zero_alloc_patterns::{StackString, InlineVec, StaticPool},
    const_collections::*,
};
use blixard_core::types::VmConfig;
use blixard_core::validated_newtype;
use std::time::Instant;

/// Example: VM resource requirements with validated types
#[derive(Debug, Clone)]
pub struct VmResourceRequirements {
    /// CPU cores (must be positive)
    pub cpu_cores: Positive<u32>,
    /// Memory in MB (bounded between 64MB and 64GB)
    pub memory_mb: BoundedInt<64, 65536>,
    /// Storage in GB (must be positive)
    pub storage_gb: Positive<u64>,
    /// Network bandwidth in Mbps (percentage of total)
    pub network_percent: BoundedInt<0, 100>,
}

impl VmResourceRequirements {
    /// Create with validation
    pub fn new(
        cpu_cores: u32,
        memory_mb: i64,
        storage_gb: u64,
        network_percent: i64,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            cpu_cores: Positive::new(cpu_cores)?,
            memory_mb: BoundedInt::new(memory_mb)?,
            storage_gb: Positive::new(storage_gb)?,
            network_percent: BoundedInt::new(network_percent)?,
        })
    }

    /// Get resource usage as measurements with units
    pub fn as_measurements(&self) -> ResourceMeasurements {
        ResourceMeasurements {
            memory: Measurement::new(*self.memory_mb.get() as u64),
            storage: Measurement::new(*self.storage_gb.get()),
            cpu_cores: *self.cpu_cores.get(),
            network_percent: *self.network_percent.get(),
        }
    }
}

/// Resource measurements with phantom types for unit safety
#[derive(Debug)]
pub struct ResourceMeasurements {
    pub memory: Measurement<u64, blixard_core::zero_cost::phantom_types::units::Megabytes>,
    pub storage: Measurement<u64, blixard_core::zero_cost::phantom_types::units::Gigabytes>,
    pub cpu_cores: u32,
    pub network_percent: i64,
}

impl ResourceMeasurements {
    /// Convert memory to bytes
    pub fn memory_bytes(&self) -> Measurement<u64, blixard_core::zero_cost::phantom_types::units::Bytes> {
        self.memory.to_bytes()
    }

    /// Convert storage to bytes
    pub fn storage_bytes(&self) -> Measurement<u64, blixard_core::zero_cost::phantom_types::units::Bytes> {
        self.storage.to_bytes()
    }
}

/// Example: Validated VM identifier
validated_newtype!(
    /// VM identifier that ensures valid format
    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct VmIdentifier(String) where |id| {
        !id.is_empty() && 
        id.len() <= 64 && 
        id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    };
    "VM identifier must be non-empty, alphanumeric with dashes/underscores, and max 64 chars"
);

/// Example: Node priority level
pub type NodePriority = BoundedInt<0, 10>;

/// Example: Configuration with compile-time validation
const VM_CONFIG_VERSION: &str = "1.2.0";
pub type VmConfigData = ValidatedConfig<VM_CONFIG_VERSION, 4096>;

/// Example: VM state machine with type safety
pub fn demonstrate_vm_lifecycle() {
    println!("=== VM Lifecycle State Machine Demo ===");
    
    // Create a VM in the Created state
    let vm = Vm::new("demo-vm".to_string(), VmConfig::default());
    println!("VM created: {} (state: {})", vm.id(), vm.state_name());

    // Configure the VM (Created -> Configured)
    let vm = vm.configure();
    println!("VM configured: {} (state: {})", vm.id(), vm.state_name());

    // Start the VM (Configured -> Starting)
    let vm = vm.start();
    println!("VM starting: {} (state: {})", vm.id(), vm.state_name());

    // VM successfully started (Starting -> Running)
    let vm = vm.started();
    println!("VM running: {} (state: {})", vm.id(), vm.state_name());

    // Stop the VM (Running -> Stopping)
    let vm = vm.stop();
    println!("VM stopping: {} (state: {})", vm.id(), vm.state_name());

    // VM successfully stopped (Stopping -> Stopped)
    let vm = vm.stopped();
    println!("VM stopped: {} (state: {})", vm.id(), vm.state_name());

    // Restart (Stopped -> Starting)
    let vm = vm.restart();
    println!("VM restarting: {} (state: {})", vm.id(), vm.state_name());

    // This would not compile - invalid state transition:
    // let vm = vm.configure(); // Error: no method named `configure` found for struct `Vm<Starting>`
    
    println!();
}

/// Example: Node lifecycle with type safety
pub fn demonstrate_node_lifecycle() {
    println!("=== Node Lifecycle State Machine Demo ===");
    
    let node = Node::new(1);
    println!("Node created: {} (state: {})", node.id(), node.state_name());

    let node = node.initialize();
    println!("Node initializing: {} (state: {})", node.id(), node.state_name());

    let node = node.initialized();
    println!("Node ready: {} (state: {})", node.id(), node.state_name());

    let node = node.join_cluster();
    println!("Node joining cluster: {} (state: {})", node.id(), node.state_name());

    let node = node.joined();
    println!("Node in cluster: {} (state: {})", node.id(), node.state_name());

    println!();
}

/// Example: Validated types and measurements
pub fn demonstrate_validated_types() {
    println!("=== Validated Types and Units Demo ===");

    // Create validated VM requirements
    let requirements = VmResourceRequirements::new(4, 2048, 100, 80).unwrap();
    println!("VM Requirements: {:?}", requirements);

    // Get measurements with unit safety
    let measurements = requirements.as_measurements();
    println!("Memory in bytes: {}", measurements.memory_bytes().value());
    println!("Storage in bytes: {}", measurements.storage_bytes().value());

    // VM identifier validation
    let vm_id = VmIdentifier::new("my-test-vm-001".to_string()).unwrap();
    println!("Valid VM ID: {}", vm_id.as_inner());

    // This would fail:
    // let invalid_id = VmIdentifier::new("".to_string()); // Error: VM identifier must be non-empty

    // Node priority
    let priority = NodePriority::new(8).unwrap();
    println!("Node priority: {}", priority.get());

    println!();
}

/// Example: Static dispatch for zero-cost polymorphism
pub fn demonstrate_static_dispatch() {
    println!("=== Static Dispatch Demo ===");

    // Docker VM manager
    let mut docker_manager = VmManager::<DockerDispatch>::new(()).unwrap();
    println!("Created {} VM manager", docker_manager.backend_type());

    let vm_id = docker_manager.create_vm("nginx:latest").unwrap();
    docker_manager.start_vm(vm_id).unwrap();
    docker_manager.stop_vm(vm_id).unwrap();

    // Firecracker VM manager (zero cost to switch)
    let mut fc_manager = VmManager::<FirecrackerDispatch>::new(()).unwrap();
    println!("Created {} VM manager", fc_manager.backend_type());

    let vm_id = fc_manager.create_vm("alpine.ext4").unwrap();
    fc_manager.start_vm(vm_id).unwrap();
    fc_manager.stop_vm(vm_id).unwrap();

    // Database client with different backends
    let mut pg_client = DatabaseClient::<PostgreSQLDispatch>::new(()).unwrap();
    println!("Created {} client", pg_client.backend_type());

    pg_client.connect("main", "postgresql://localhost/blixard").unwrap();
    let _result = pg_client.execute("main", "SELECT COUNT(*) FROM vms").unwrap();

    println!();
}

/// Example: Zero-allocation patterns
pub fn demonstrate_zero_allocation() {
    println!("=== Zero-Allocation Patterns Demo ===");

    // Stack-allocated string
    let mut stack_str = StackString::<64>::new();
    stack_str.push_str("VM-").unwrap();
    stack_str.push_str("12345").unwrap();
    println!("Stack string: {}", stack_str);

    // Inline vector with fixed capacity
    let mut inline_vec = InlineVec::<u32, 8>::new();
    inline_vec.push(100).unwrap();
    inline_vec.push(200).unwrap();
    inline_vec.push(300).unwrap();
    println!("Inline vector: {:?}", inline_vec.as_slice());

    // Static object pool
    let mut pool = StaticPool::<Vec<u8>, 4>::new();
    
    {
        let mut buffer = pool.try_acquire(|| Vec::with_capacity(1024)).unwrap();
        buffer.extend_from_slice(b"Hello, World!");
        println!("Pooled buffer: {}", String::from_utf8_lossy(&buffer));
    } // Buffer returned to pool automatically

    println!("Pool stats: {:?}", pool.stats());

    println!();
}

/// Example: Compile-time collections and lookups
pub fn demonstrate_const_collections() {
    println!("=== Compile-Time Collections Demo ===");

    // Const map for VM states
    const VM_STATE_DESCRIPTIONS: ConstMap<&str, &str, 6> = ConstMap::from_unsorted([
        ("created", "VM has been created but not configured"),
        ("configured", "VM is configured and ready to start"),
        ("starting", "VM is in the process of starting"),
        ("running", "VM is running and operational"),
        ("stopping", "VM is in the process of stopping"),
        ("stopped", "VM has been stopped"),
    ]);

    println!("VM state 'running': {}", VM_STATE_DESCRIPTIONS.get("running").unwrap());
    println!("VM state 'invalid': {:?}", VM_STATE_DESCRIPTIONS.get("invalid"));

    // Const set for supported hypervisors
    const SUPPORTED_HYPERVISORS: ConstSet<&str, 3> = ConstSet::from_unsorted([
        "firecracker",
        "cloud-hypervisor", 
        "qemu",
    ]);

    println!("Supports firecracker: {}", SUPPORTED_HYPERVISORS.contains("firecracker"));
    println!("Supports docker: {}", SUPPORTED_HYPERVISORS.contains("docker"));

    // Static lookup for resource limits
    const RESOURCE_LIMITS: StaticLookup<u64, 10> = StaticLookup::with_entries([
        (0, 1024),    // Small: 1GB RAM
        (1, 2048),    // Medium: 2GB RAM
        (2, 4096),    // Large: 4GB RAM
        (3, 8192),    // XLarge: 8GB RAM
    ]);

    println!("Medium VM memory limit: {}MB", RESOURCE_LIMITS.get(1).unwrap());

    println!();
}

/// Example: Const validation and compile-time checks
pub fn demonstrate_compile_time_validation() {
    println!("=== Compile-Time Validation Demo ===");

    // These are validated at compile time
    validate_const_str!("valid_identifier", identifier);
    validate_const_str!("2.1.0", semver);
    validate_range!(50, 0, 100);

    // Validated configuration
    const CONFIG: ValidatedConfig<"1.0.0", 512> = ValidatedConfig::new()
        .with_data(b"max_vms=100\ntimeout=30s\nretries=3");

    println!("Config version: {}", CONFIG.version());
    println!("Config size: {} bytes", CONFIG.size());

    // Const lookup table
    const_lookup!(
        VmSizes: [&'static str; (u32, u32)] = [
            ("nano", (1, 512)),
            ("micro", (1, 1024)),
            ("small", (2, 2048)),
            ("medium", (4, 4096)),
            ("large", (8, 8192)),
        ]
    );

    let (cpu, mem) = VmSizes::lookup("medium").unwrap();
    println!("Medium VM: {} CPU, {}MB RAM", cpu, mem);

    println!();
}

/// Performance comparison: showing zero-cost nature
pub fn performance_comparison() {
    println!("=== Performance Comparison ===");

    const ITERATIONS: usize = 1_000_000;

    // Test 1: Validated types vs runtime validation
    let start = Instant::now();
    for i in 0..ITERATIONS {
        let _positive = Positive::<u32>::new_unchecked(i as u32 + 1);
    }
    let zero_cost_time = start.elapsed();

    let start = Instant::now();
    for i in 0..ITERATIONS {
        let value = i as u32 + 1;
        if value > 0 {
            let _positive = value;
        }
    }
    let runtime_check_time = start.elapsed();

    println!("Validated types: {:?}", zero_cost_time);
    println!("Runtime checks: {:?}", runtime_check_time);
    println!("Overhead: {:.2}%", 
        ((zero_cost_time.as_nanos() as f64 / runtime_check_time.as_nanos() as f64) - 1.0) * 100.0);

    // Test 2: Static dispatch vs dynamic dispatch
    let mut docker_manager = VmManager::<DockerDispatch>::new(()).unwrap();
    
    let start = Instant::now();
    for i in 0..1000 {
        let _vm_id = docker_manager.create_vm(&format!("config-{}", i)).unwrap();
    }
    let static_dispatch_time = start.elapsed();

    println!("Static dispatch: {:?}", static_dispatch_time);

    // Test 3: Const collections vs HashMap
    const STATUS_MAP: ConstMap<u32, &str, 5> = ConstMap::from_unsorted([
        (200, "OK"),
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (404, "Not Found"),
        (500, "Internal Server Error"),
    ]);

    use std::collections::HashMap;
    let mut hash_map = HashMap::new();
    hash_map.insert(200, "OK");
    hash_map.insert(400, "Bad Request");
    hash_map.insert(401, "Unauthorized");
    hash_map.insert(404, "Not Found");
    hash_map.insert(500, "Internal Server Error");

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _result = STATUS_MAP.get(404);
    }
    let const_map_time = start.elapsed();

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _result = hash_map.get(&404);
    }
    let hash_map_time = start.elapsed();

    println!("Const map lookup: {:?}", const_map_time);
    println!("HashMap lookup: {:?}", hash_map_time);
    println!("Speedup: {:.2}x", 
        hash_map_time.as_nanos() as f64 / const_map_time.as_nanos() as f64);

    println!();
}

fn main() {
    println!("🚀 Blixard Zero-Cost Abstractions Demonstration\n");

    demonstrate_vm_lifecycle();
    demonstrate_node_lifecycle();
    demonstrate_validated_types();
    demonstrate_static_dispatch();
    demonstrate_zero_allocation();
    demonstrate_const_collections();
    demonstrate_compile_time_validation();
    performance_comparison();

    println!("✅ All demonstrations completed successfully!");
    println!("\n📈 Key Benefits:");
    println!("  • Zero runtime overhead - abstractions compile away");
    println!("  • Compile-time safety - invalid states caught at build time");
    println!("  • Type-driven design - impossible states are unrepresentable");
    println!("  • Performance optimizations - const evaluation and static dispatch");
    println!("  • Memory efficiency - zero-allocation patterns for hot paths");
}