//! Integration tests for resource monitoring and pressure detection

#![cfg(feature = "test-helpers")]

use blixard_core::{
    node_shared::SharedNodeState,
    raft_manager::WorkerCapabilities,
    resource_collection::SystemResourceCollector,
    resource_monitor::{ResourceMonitor, ResourcePressure},
    types::{Hypervisor, NodeConfig, VmConfig, VmStatus},
    vm_backend::{MockVmBackend, VmBackend, VmManager},
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Helper to create a test VM manager
async fn create_test_vm_manager() -> Arc<VmManager> {
    let backend = MockVmBackend::new();
    Arc::new(VmManager::new(Box::new(backend)))
}

/// Helper to create test node state
async fn create_test_node_state(node_id: u64, cpu: u32, memory: u64) -> Arc<SharedNodeState> {
    let temp_dir = TempDir::new().unwrap();
    let node_config = NodeConfig {
        id: node_id,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:7000".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
        vm_backend: "microvm".to_string(),
        transport_config: None,
        topology: Default::default(),
    };

    let shared_state = Arc::new(SharedNodeState::new(node_config));

    // Set worker capabilities
    let capabilities = WorkerCapabilities {
        cpu_cores: cpu,
        memory_mb: memory,
        disk_gb: 100,
        features: vec!["microvm".to_string()],
    };

    shared_state.set_worker_capabilities(capabilities).await;
    shared_state
}

#[tokio::test]
async fn test_resource_monitor_basic_operation() {
    let node_state = create_test_node_state(1, 8, 16384).await;
    let vm_manager = create_test_vm_manager().await;

    // Create resource monitor with short interval for testing
    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    // Start monitoring
    monitor.start().await.unwrap();

    // Wait for initial collection
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get node utilization
    let utilization = monitor.get_node_utilization().await;
    assert!(utilization.is_some());

    let util = utilization.unwrap();
    assert_eq!(util.node_id, 1);
    assert_eq!(util.physical_vcpus, 8);
    assert_eq!(util.physical_memory_mb, 16384);
    assert_eq!(util.vm_count, 0);

    // Stop monitoring
    monitor.stop().await;
}

#[tokio::test]
async fn test_resource_pressure_detection() {
    let node_state = create_test_node_state(1, 4, 8192).await;
    let vm_manager = create_test_vm_manager().await;

    // Add some VMs to simulate resource usage
    for i in 0..3 {
        let vm_config = VmConfig {
            name: format!("pressure-test-vm-{}", i),
            vcpus: 2,
            memory: 2048,
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

        vm_manager.create_vm(vm_config).await.unwrap();
    }

    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    // Register pressure callback
    let pressure_detected = Arc::new(RwLock::new(false));
    let pressure_clone = pressure_detected.clone();

    monitor
        .on_pressure_change(move |pressure: ResourcePressure| {
            if pressure.is_high_pressure {
                let pressure_clone = pressure_clone.clone();
                tokio::spawn(async move {
                    *pressure_clone.write().await = true;
                });
            }
        })
        .await;

    // Start monitoring
    monitor.start().await.unwrap();

    // Wait for monitoring to detect pressure
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Check pressure state
    let pressure = monitor.get_resource_pressure().await;

    // With 6 vCPUs allocated on 4 physical cores, we should have overcommit
    assert!(pressure.overcommit_cpu > 1.0);

    // Stop monitoring
    monitor.stop().await;
}

#[tokio::test]
async fn test_resource_efficiency_calculation() {
    let node_state = create_test_node_state(1, 8, 16384).await;
    let vm_manager = create_test_vm_manager().await;

    // Create VMs with known resource allocation
    let vm1 = VmConfig {
        name: "efficient-vm-1".to_string(),
        vcpus: 4,
        memory: 8192,
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

    let vm2 = VmConfig {
        name: "efficient-vm-2".to_string(),
        vcpus: 2,
        memory: 4096,
        preemptible: false,
        ..vm1.clone()
    };

    vm_manager.create_vm(vm1).await.unwrap();
    vm_manager.create_vm(vm2).await.unwrap();

    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    monitor.start().await.unwrap();

    // Wait for monitoring
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get efficiency metrics
    let efficiency = monitor.get_resource_efficiency().await;

    // CPU efficiency should be less than 1.0 (actual < allocated)
    assert!(efficiency.cpu_efficiency > 0.0);
    assert!(efficiency.cpu_efficiency <= 1.0);

    // Memory efficiency should also be reasonable
    assert!(efficiency.memory_efficiency > 0.0);
    assert!(efficiency.memory_efficiency <= 1.0);

    assert_eq!(efficiency.vm_count, 2);

    monitor.stop().await;
}

#[tokio::test]
async fn test_vm_resource_usage_tracking() {
    let node_state = create_test_node_state(1, 8, 16384).await;
    let vm_manager = create_test_vm_manager().await;

    // Create a VM
    let vm_config = VmConfig {
        name: "tracked-vm".to_string(),
        vcpus: 2,
        memory: 4096,
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

    vm_manager.create_vm(vm_config).await.unwrap();

    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    monitor.start().await.unwrap();

    // Wait for monitoring
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get VM usage
    let vm_usage = monitor.get_vm_usage("tracked-vm").await;
    assert!(vm_usage.is_some());

    let usage = vm_usage.unwrap();
    assert_eq!(usage.vm_name, "tracked-vm");
    assert_eq!(usage.allocated_vcpus, 2);
    assert_eq!(usage.allocated_memory_mb, 4096);

    // Actual usage should be non-zero (even if simulated)
    assert!(usage.actual_cpu_percent > 0.0);
    assert!(usage.actual_memory_mb > 0);

    // Get all VM usage
    let all_usage = monitor.get_all_vm_usage().await;
    assert_eq!(all_usage.len(), 1);
    assert!(all_usage.contains_key("tracked-vm"));

    monitor.stop().await;
}

#[tokio::test]
async fn test_system_resource_collection() {
    // Test system resource collector directly

    // CPU usage should be between 0 and 100
    let cpu_usage = SystemResourceCollector::get_system_cpu_usage().unwrap();
    assert!(cpu_usage >= 0.0);
    assert!(cpu_usage <= 100.0);

    // Memory usage should be reasonable
    let memory_usage = SystemResourceCollector::get_system_memory_usage().unwrap();
    assert!(memory_usage > 0);

    // Disk usage for /tmp should be non-zero
    let disk_usage = SystemResourceCollector::get_disk_usage("/tmp").unwrap();
    assert!(disk_usage >= 0);
}

#[tokio::test]
async fn test_overcommit_ratio_calculation() {
    let node_state = create_test_node_state(1, 4, 8192).await;
    let vm_manager = create_test_vm_manager().await;

    // Create VMs that overcommit resources
    for i in 0..3 {
        let vm_config = VmConfig {
            name: format!("overcommit-vm-{}", i),
            vcpus: 2,
            memory: 4096,
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

        vm_manager.create_vm(vm_config).await.unwrap();
    }

    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    monitor.start().await.unwrap();

    // Wait for monitoring
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get node utilization
    let utilization = monitor.get_node_utilization().await.unwrap();

    // Check overcommit ratios
    assert_eq!(utilization.total_allocated_vcpus, 6); // 3 VMs * 2 vCPUs
    assert_eq!(utilization.physical_vcpus, 4);
    assert_eq!(utilization.overcommit_ratio_cpu, 1.5); // 6/4 = 1.5

    assert_eq!(utilization.total_allocated_memory_mb, 12288); // 3 * 4096
    assert_eq!(utilization.physical_memory_mb, 8192);
    assert_eq!(utilization.overcommit_ratio_memory, 1.5); // 12288/8192 = 1.5

    monitor.stop().await;
}

#[tokio::test]
async fn test_pressure_callback_execution() {
    let node_state = create_test_node_state(1, 2, 4096).await;
    let vm_manager = create_test_vm_manager().await;

    let mut monitor = ResourceMonitor::new(
        node_state.clone(),
        vm_manager.clone(),
        Duration::from_millis(100),
    );

    // Track callback executions
    let callback_count = Arc::new(RwLock::new(0));
    let count_clone = callback_count.clone();

    monitor
        .on_pressure_change(move |pressure: ResourcePressure| {
            let count_clone = count_clone.clone();
            tokio::spawn(async move {
                let mut count = count_clone.write().await;
                *count += 1;

                println!(
                    "Pressure callback: CPU={:.2}, Memory={:.2}, High={}",
                    pressure.cpu_pressure, pressure.memory_pressure, pressure.is_high_pressure
                );
            });
        })
        .await;

    // Start monitoring
    monitor.start().await.unwrap();

    // Create VMs to trigger pressure changes
    for i in 0..3 {
        let vm_config = VmConfig {
            name: format!("pressure-trigger-vm-{}", i),
            vcpus: 1,
            memory: 2048,
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

        vm_manager.create_vm(vm_config).await.unwrap();

        // Wait between VM creations to allow monitoring to detect changes
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Wait for final monitoring cycle
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Callback should have been executed at least once
    let final_count = *callback_count.read().await;
    assert!(final_count >= 1, "Pressure callback was not executed");

    monitor.stop().await;
}
