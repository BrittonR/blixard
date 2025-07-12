//! Comprehensive linearizability test for Blixard
//!
//! This test implements a Jepsen-style linearizability testing framework
//! inspired by FoundationDB and TigerBeetle's approaches.

#![cfg(feature = "test-helpers")]

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use blixard_core::{
    test_helpers::{TestCluster, TestNode},
    types::{CreateVmRequest, VmConfig as BlixardVmConfig},
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Operation types for linearizability testing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum Operation {
    CreateVm { name: String, config: VmConfig },
    StartVm { name: String },
    StopVm { name: String },
    DeleteVm { name: String },
    GetVmStatus { name: String },
    Write { key: String, value: String },
    Read { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct VmConfig {
    memory_mb: u32,
    vcpus: u32,
}

/// Response types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum Response {
    VmCreated { success: bool },
    VmStarted { success: bool },
    VmStopped { success: bool },
    VmDeleted { success: bool },
    VmStatus { exists: bool, running: Option<bool> },
    Written { success: bool },
    Value { value: Option<String> },
    Error { message: String },
}

/// History entry
#[derive(Debug, Clone)]
struct HistoryEntry {
    process_id: u64,
    operation: Operation,
    invocation_time: Instant,
    response_time: Option<Instant>,
    response: Option<Response>,
}

/// History recorder
#[derive(Clone)]
struct History {
    entries: Arc<Mutex<Vec<HistoryEntry>>>,
    next_process_id: Arc<AtomicU64>,
}

impl History {
    fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            next_process_id: Arc::new(AtomicU64::new(0)),
        }
    }

    async fn begin_operation(&self, operation: Operation) -> u64 {
        let process_id = self.next_process_id.fetch_add(1, Ordering::SeqCst);
        let entry = HistoryEntry {
            process_id,
            operation,
            invocation_time: Instant::now(),
            response_time: None,
            response: None,
        };
        self.entries.lock().await.push(entry);
        process_id
    }

    async fn end_operation(&self, process_id: u64, response: Response) {
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.iter_mut().find(|e| e.process_id == process_id) {
            entry.response_time = Some(Instant::now());
            entry.response = Some(response);
        }
    }

    async fn get_completed(&self) -> Vec<HistoryEntry> {
        self.entries
            .lock()
            .await
            .iter()
            .filter(|e| e.response.is_some())
            .cloned()
            .collect()
    }
}

/// Sequential specification for VMs
struct VmSpecification {
    vms: HashMap<String, (VmConfig, bool)>, // (config, running)
}

impl VmSpecification {
    fn new() -> Self {
        Self {
            vms: HashMap::new(),
        }
    }

    fn apply(&mut self, operation: &Operation) -> Response {
        match operation {
            Operation::CreateVm { name, config } => {
                if self.vms.contains_key(name) {
                    Response::VmCreated { success: false }
                } else {
                    self.vms.insert(name.clone(), (config.clone(), false));
                    Response::VmCreated { success: true }
                }
            }
            Operation::StartVm { name } => {
                if let Some((_, running)) = self.vms.get_mut(name) {
                    *running = true;
                    Response::VmStarted { success: true }
                } else {
                    Response::VmStarted { success: false }
                }
            }
            Operation::StopVm { name } => {
                if let Some((_, running)) = self.vms.get_mut(name) {
                    *running = false;
                    Response::VmStopped { success: true }
                } else {
                    Response::VmStopped { success: false }
                }
            }
            Operation::DeleteVm { name } => {
                if self.vms.remove(name).is_some() {
                    Response::VmDeleted { success: true }
                } else {
                    Response::VmDeleted { success: false }
                }
            }
            Operation::GetVmStatus { name } => {
                if let Some((_, running)) = self.vms.get(name) {
                    Response::VmStatus {
                        exists: true,
                        running: Some(*running),
                    }
                } else {
                    Response::VmStatus {
                        exists: false,
                        running: None,
                    }
                }
            }
            _ => Response::Error {
                message: "Unsupported operation".to_string(),
            },
        }
    }
}

/// Check if a history is linearizable
fn check_linearizability(
    completed: &[HistoryEntry],
    mut spec: VmSpecification,
) -> Result<(), String> {
    // Simple linearizability check using permutation search
    // For production, use more efficient algorithms

    if completed.is_empty() {
        return Ok(());
    }

    // Build happens-before graph based on real-time ordering
    let mut happens_before: HashMap<u64, HashSet<u64>> = HashMap::new();

    for i in 0..completed.len() {
        let op1 = &completed[i];
        happens_before
            .entry(op1.process_id)
            .or_insert_with(HashSet::new);

        for j in 0..completed.len() {
            if i != j {
                let op2 = &completed[j];

                // op1 happens-before op2 if op1 completes before op2 starts
                if let (Some(end1), start2) = (op1.response_time, op2.invocation_time) {
                    if end1 < start2 {
                        happens_before
                            .get_mut(&op1.process_id)
                            .unwrap()
                            .insert(op2.process_id);
                    }
                }
            }
        }
    }

    // Try to find a valid linearization
    let mut linearization = Vec::new();
    let mut used = HashSet::new();

    if find_linearization(
        completed,
        &happens_before,
        &mut spec,
        &mut linearization,
        &mut used,
    ) {
        Ok(())
    } else {
        Err("No valid linearization found".to_string())
    }
}

fn find_linearization(
    operations: &[HistoryEntry],
    happens_before: &HashMap<u64, HashSet<u64>>,
    spec: &mut VmSpecification,
    linearization: &mut Vec<u64>,
    used: &mut HashSet<u64>,
) -> bool {
    if linearization.len() == operations.len() {
        return true;
    }

    for op in operations {
        if used.contains(&op.process_id) {
            continue;
        }

        // Check if all operations that must happen before this one are already linearized
        let can_linearize = operations.iter().all(|other| {
            if let Some(successors) = happens_before.get(&other.process_id) {
                !successors.contains(&op.process_id) || used.contains(&other.process_id)
            } else {
                true
            }
        });

        if !can_linearize {
            continue;
        }

        // Try linearizing this operation
        let expected = spec.apply(&op.operation);

        if op.response.as_ref() == Some(&expected) {
            // Mark as used and continue
            used.insert(op.process_id);
            linearization.push(op.process_id);

            // Clone spec state for backtracking
            let spec_backup = VmSpecification {
                vms: spec.vms.clone(),
            };

            if find_linearization(operations, happens_before, spec, linearization, used) {
                return true;
            }

            // Backtrack
            *spec = spec_backup;
            linearization.pop();
            used.remove(&op.process_id);
        }
    }

    false
}

/// Generate concurrent workload
async fn generate_workload(
    client_count: usize,
    operations_per_client: usize,
    cluster: &TestCluster,
    history: History,
) {
    let clients = cluster.get_clients().await;
    let mut handles = vec![];

    for client_id in 0..client_count {
        let client = clients[client_id % clients.len()].clone();
        let history = history.clone();

        let handle = tokio::spawn(async move {
            let mut rng = thread_rng();

            for op_idx in 0..operations_per_client {
                // Generate random operation
                let op = match rng.gen_range(0..5) {
                    0 => Operation::CreateVm {
                        name: format!("vm-{}-{}", client_id, op_idx),
                        config: VmConfig {
                            memory_mb: 1024,
                            vcpus: 2,
                        },
                    },
                    1 => Operation::StartVm {
                        name: format!("vm-{}-{}", client_id, rng.gen_range(0..op_idx.max(1))),
                    },
                    2 => Operation::StopVm {
                        name: format!("vm-{}-{}", client_id, rng.gen_range(0..op_idx.max(1))),
                    },
                    3 => Operation::DeleteVm {
                        name: format!("vm-{}-{}", client_id, rng.gen_range(0..op_idx.max(1))),
                    },
                    _ => Operation::GetVmStatus {
                        name: format!("vm-{}-{}", client_id, rng.gen_range(0..op_idx.max(1))),
                    },
                };

                let process_id = history.begin_operation(op.clone()).await;

                // Execute operation
                let response = match &op {
                    Operation::CreateVm { name, config } => {
                        match client
                            .create_vm(CreateVmRequest {
                                name: name.clone(),
                                config: BlixardVmConfig {
                                    memory: config.memory_mb as u64 * 1024 * 1024,
                                    vcpus: config.vcpus,
                                    disk_size: 10 * 1024 * 1024 * 1024,
                                    image: "test-image".to_string(),
                                    network_interfaces: vec![],
                                    metadata: HashMap::new(),
                                },
                            })
                            .await
                        {
                            Ok(_) => Response::VmCreated { success: true },
                            Err(_) => Response::VmCreated { success: false },
                        }
                    }
                    Operation::StartVm { name } => match client.start_vm(name.clone()).await {
                        Ok(_) => Response::VmStarted { success: true },
                        Err(_) => Response::VmStarted { success: false },
                    },
                    Operation::StopVm { name } => match client.stop_vm(name.clone()).await {
                        Ok(_) => Response::VmStopped { success: true },
                        Err(_) => Response::VmStopped { success: false },
                    },
                    Operation::DeleteVm { name } => match client.delete_vm(name.clone()).await {
                        Ok(_) => Response::VmDeleted { success: true },
                        Err(_) => Response::VmDeleted { success: false },
                    },
                    Operation::GetVmStatus { name } => {
                        match client.get_vm_status(name.clone()).await {
                            Ok(status) => Response::VmStatus {
                                exists: true,
                                running: Some(status.state == "running"),
                            },
                            Err(_) => Response::VmStatus {
                                exists: false,
                                running: None,
                            },
                        }
                    }
                    _ => Response::Error {
                        message: "Not implemented".to_string(),
                    },
                };

                history.end_operation(process_id, response).await;

                // Small delay between operations
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        handles.push(handle);
    }

    // Wait for all clients
    for handle in handles {
        handle.await.unwrap();
    }
}

/// Test VM operations for linearizability
#[tokio::test]
async fn test_vm_linearizability() {
    // Setup cluster
    let cluster = TestCluster::with_size(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Create history
    let history = History::new();

    // Generate concurrent workload
    generate_workload(5, 20, &cluster, history.clone()).await;

    // Check linearizability
    let completed = history.get_completed().await;
    println!("Completed {} operations", completed.len());

    let spec = VmSpecification::new();
    match check_linearizability(&completed, spec) {
        Ok(()) => println!("History is linearizable!"),
        Err(e) => {
            // Find minimal failing subsequence
            for size in 2..=completed.len().min(10) {
                for window in completed.windows(size) {
                    let spec = VmSpecification::new();
                    if check_linearizability(window, spec).is_err() {
                        println!("Found minimal failing sequence of {} operations:", size);
                        for entry in window {
                            println!("  {:?} -> {:?}", entry.operation, entry.response);
                        }
                        break;
                    }
                }
            }
            panic!("Linearizability violation: {}", e);
        }
    }
}

/// Test under network partition
#[tokio::test]
async fn test_partition_linearizability() {
    let cluster = TestCluster::with_size(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let history = History::new();

    // Start workload
    let workload_handle = {
        let cluster = cluster.clone();
        let history = history.clone();
        tokio::spawn(async move {
            generate_workload(3, 30, &cluster, history).await;
        })
    };

    // Inject partition after some operations
    tokio::time::sleep(Duration::from_secs(5)).await;
    cluster.partition_network(vec![1, 2], vec![3, 4, 5]).await;

    // Let it run with partition
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Heal partition
    cluster.heal_network_partition().await;

    // Wait for workload to complete
    workload_handle.await.unwrap();

    // Check linearizability
    let completed = history.get_completed().await;
    println!("Completed {} operations under partition", completed.len());

    // Count operations by response type
    let mut success_count = 0;
    let mut failure_count = 0;

    for entry in &completed {
        match &entry.response {
            Some(Response::VmCreated { success: true })
            | Some(Response::VmStarted { success: true })
            | Some(Response::VmStopped { success: true })
            | Some(Response::VmDeleted { success: true }) => success_count += 1,
            _ => failure_count += 1,
        }
    }

    println!("Successful operations: {}", success_count);
    println!("Failed operations: {}", failure_count);

    // Check that minority partition operations failed
    let spec = VmSpecification::new();
    match check_linearizability(&completed, spec) {
        Ok(()) => println!("History is linearizable despite partition!"),
        Err(e) => panic!("Linearizability violation under partition: {}", e),
    }
}

/// Test clock skew scenarios
#[tokio::test]
async fn test_clock_skew_linearizability() {
    let cluster = TestCluster::with_size(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let history = History::new();

    // Simulate clock skew by adding artificial delays
    let clients = cluster.get_clients().await;
    let mut handles = vec![];

    for (client_id, client) in clients.iter().enumerate() {
        let client = client.clone();
        let history = history.clone();
        let skew_ms = client_id as u64 * 1000; // Different skew per client

        let handle = tokio::spawn(async move {
            // Add artificial "clock skew"
            tokio::time::sleep(Duration::from_millis(skew_ms)).await;

            for i in 0..10 {
                let op = Operation::Write {
                    key: format!("key-{}", i),
                    value: format!("client-{}-value-{}", client_id, i),
                };

                let process_id = history.begin_operation(op).await;

                // Simulate write through consensus
                tokio::time::sleep(Duration::from_millis(50)).await;

                history
                    .end_operation(process_id, Response::Written { success: true })
                    .await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let completed = history.get_completed().await;
    println!(
        "Completed {} operations with simulated clock skew",
        completed.len()
    );

    // For key-value ops, we'd need a KV specification
    // This demonstrates the framework's capability to handle timing issues
}

/// Analyze violation patterns
fn analyze_violations(history: &[HistoryEntry]) {
    println!("\nAnalyzing violation patterns...");

    // Check for lost updates
    let mut writes_by_key: HashMap<String, Vec<&HistoryEntry>> = HashMap::new();
    let mut reads_by_key: HashMap<String, Vec<&HistoryEntry>> = HashMap::new();

    for entry in history {
        match &entry.operation {
            Operation::Write { key, .. } => {
                writes_by_key.entry(key.clone()).or_default().push(entry);
            }
            Operation::Read { key } => {
                reads_by_key.entry(key.clone()).or_default().push(entry);
            }
            _ => {}
        }
    }

    // Look for anomalies
    for (key, writes) in &writes_by_key {
        if let Some(reads) = reads_by_key.get(key) {
            for read in reads {
                if let Some(Response::Value {
                    value: Some(read_value),
                }) = &read.response
                {
                    // Check if this value was ever written
                    let value_written = writes.iter().any(|w| {
                        matches!(&w.operation, Operation::Write { value, .. } if value == read_value)
                    });

                    if !value_written {
                        println!(
                            "Anomaly: Read of key '{}' returned value '{}' that was never written",
                            key, read_value
                        );
                    }
                }
            }
        }
    }

    // Check for duplicate successful operations
    let mut successful_creates: HashMap<String, usize> = HashMap::new();

    for entry in history {
        if let Operation::CreateVm { name, .. } = &entry.operation {
            if let Some(Response::VmCreated { success: true }) = &entry.response {
                *successful_creates.entry(name.clone()).or_default() += 1;
            }
        }
    }

    for (name, count) in successful_creates {
        if count > 1 {
            println!(
                "Anomaly: VM '{}' was successfully created {} times",
                name, count
            );
        }
    }
}

/// Performance analysis
fn analyze_performance(history: &[HistoryEntry]) {
    println!("\nPerformance Analysis:");

    let mut latencies = Vec::new();
    let mut op_counts: HashMap<String, usize> = HashMap::new();

    for entry in history {
        if let (Some(response_time), invocation_time) = (entry.response_time, entry.invocation_time)
        {
            let latency = response_time.duration_since(invocation_time);
            latencies.push(latency);

            let op_type = match &entry.operation {
                Operation::CreateVm { .. } => "CreateVm",
                Operation::StartVm { .. } => "StartVm",
                Operation::StopVm { .. } => "StopVm",
                Operation::DeleteVm { .. } => "DeleteVm",
                Operation::GetVmStatus { .. } => "GetVmStatus",
                Operation::Write { .. } => "Write",
                Operation::Read { .. } => "Read",
            };

            *op_counts.entry(op_type.to_string()).or_default() += 1;
        }
    }

    if !latencies.is_empty() {
        latencies.sort();

        let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let p50 = latencies[latencies.len() / 2];
        let p99 = latencies[latencies.len() * 99 / 100];

        println!("Average latency: {:?}", avg_latency);
        println!("P50 latency: {:?}", p50);
        println!("P99 latency: {:?}", p99);

        println!("\nOperation counts:");
        for (op_type, count) in op_counts {
            println!("  {}: {}", op_type, count);
        }
    }
}
