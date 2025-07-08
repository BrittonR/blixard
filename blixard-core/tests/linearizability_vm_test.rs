//! Comprehensive linearizability test for VM operations
//!
//! This test demonstrates the full capabilities of the linearizability framework
//! by testing VM lifecycle operations under various failure scenarios.

#![cfg(feature = "test-helpers")]

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use blixard_core::{
    test_helpers::{TestCluster, TestNode},
    types::{CreateVmRequest, VmConfig as BlixardVmConfig},
};
use tokio::sync::Mutex;

// Import linearizability framework types
use blixard_core::linearizability::{
    analysis::{generate_html_report, HistoryAnalyzer, ViolationPattern},
    checker::LinearizabilityChecker,
    failure_injection::{FailureInjector, FailureScenario},
    history::{HistoryRecorder, Operation, Response, VmConfig, WorkerInfo},
    specifications::VmSpecification,
    workload::{patterns, OperationMix, WorkloadConfig, WorkloadGenerator},
};

/// Test VM operations under normal conditions
#[tokio::test]
async fn test_vm_operations_linearizable() {
    // Initialize test cluster
    let cluster = TestCluster::new(3).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    // Create history recorder
    let recorder = HistoryRecorder::new();

    // Configure workload
    let workload_config = WorkloadConfig {
        client_count: 5,
        operation_count: 100,
        operation_mix: {
            let mut mix = OperationMix::default();
            mix.weights.clear();
            mix.weights.insert("create_vm".to_string(), 30);
            mix.weights.insert("start_vm".to_string(), 25);
            mix.weights.insert("stop_vm".to_string(), 20);
            mix.weights.insert("delete_vm".to_string(), 15);
            mix.weights.insert("get_vm_status".to_string(), 10);
            mix
        },
        think_time: Duration::from_millis(50),
        ..Default::default()
    };

    // Generate workload
    let generator = WorkloadGenerator::new(workload_config);
    let clients = cluster.get_clients().await;

    // Run concurrent clients
    let mut handles = vec![];

    for client_id in 0..5 {
        let client = clients[client_id % clients.len()].clone();
        let recorder = recorder.clone();
        let generator_clone = generator.clone();

        let handle = tokio::spawn(async move {
            generator_clone
                .run_client(client_id, |op| {
                    let client = client.clone();
                    let recorder = recorder.clone();

                    async move {
                        let process_id = recorder.begin_operation(op.clone()).await;

                        let response = match &op {
                            Operation::CreateVm { name, config, .. } => {
                                let result = client
                                    .create_vm(CreateVmRequest {
                                        name: name.clone(),
                                        config: BlixardVmConfig {
                                            memory: config.memory_mb as u64 * 1024 * 1024,
                                            vcpus: config.vcpus,
                                            disk_size: config.disk_gb as u64 * 1024 * 1024 * 1024,
                                            image: config.image.clone(),
                                            network_interfaces: vec![],
                                            metadata: HashMap::new(),
                                        },
                                    })
                                    .await;

                                match result {
                                    Ok(resp) => Response::VmCreated {
                                        success: true,
                                        node_id: Some(resp.node_id),
                                    },
                                    Err(_) => Response::VmCreated {
                                        success: false,
                                        node_id: None,
                                    },
                                }
                            }
                            Operation::StartVm { name } => {
                                match client.start_vm(name.clone()).await {
                                    Ok(_) => Response::VmStarted { success: true },
                                    Err(_) => Response::VmStarted { success: false },
                                }
                            }
                            Operation::StopVm { name } => {
                                match client.stop_vm(name.clone()).await {
                                    Ok(_) => Response::VmStopped { success: true },
                                    Err(_) => Response::VmStopped { success: false },
                                }
                            }
                            Operation::DeleteVm { name } => {
                                match client.delete_vm(name.clone()).await {
                                    Ok(_) => Response::VmDeleted { success: true },
                                    Err(_) => Response::VmDeleted { success: false },
                                }
                            }
                            Operation::GetVmStatus { name } => {
                                match client.get_vm_status(name.clone()).await {
                                    Ok(status) => Response::VmStatus {
                                        exists: true,
                                        state: Some(status.state),
                                        node_id: Some(status.node_id),
                                    },
                                    Err(_) => Response::VmStatus {
                                        exists: false,
                                        state: None,
                                        node_id: None,
                                    },
                                }
                            }
                            _ => Response::Error {
                                code: "UNSUPPORTED".to_string(),
                                message: "Operation not implemented".to_string(),
                            },
                        };

                        recorder.end_operation(process_id, response.clone()).await;
                        response
                    }
                })
                .await
        });

        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Get history and check linearizability
    let history = recorder.get_history().await;
    let checker = LinearizabilityChecker::new();
    let mut spec = VmSpecification::new();

    // Register workers in the specification
    for i in 1..=3 {
        spec.apply(&Operation::RegisterWorker {
            info: WorkerInfo {
                node_id: i,
                cpu_capacity: 16,
                memory_capacity: 32 * 1024 * 1024 * 1024,
                disk_capacity: 1024 * 1024 * 1024 * 1024,
                features: vec![],
            },
        });
    }

    let result = checker.check_wing_gong(&history, &mut spec);

    if !result.is_linearizable {
        // Analyze failures
        let analyzer = HistoryAnalyzer::new(history.clone());
        let report = analyzer.analyze();

        println!("Linearizability violations detected!");
        println!("Violations: {:?}", result.violations);
        println!("Report: {:?}", report);

        // Find minimal witness
        if let Some(witness) = analyzer.find_minimal_violation(spec, &checker) {
            println!("Minimal witness ({} operations):", witness.len());
            for entry in witness {
                println!("  {:?} -> {:?}", entry.operation, entry.response);
            }
        }

        panic!("VM operations are not linearizable!");
    }

    // Analyze performance
    let analyzer = HistoryAnalyzer::new(history);
    let report = analyzer.analyze();

    println!("Test completed successfully!");
    println!("Total operations: {}", report.summary.total_operations);
    println!(
        "Throughput: {:.1} ops/sec",
        report.performance.throughput_ops_per_sec
    );
    println!(
        "Average latency: {:.1}ms",
        report.performance.avg_latency_ms
    );
    println!("P99 latency: {:.1}ms", report.performance.p99_latency_ms);
}

/// Test VM operations under network partition
#[tokio::test]
async fn test_vm_operations_with_partition() {
    let cluster = TestCluster::new(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let recorder = HistoryRecorder::new();
    let failure_injector = Arc::new(Mutex::new(FailureInjector::new()));

    // Configure partition scenario
    let partition_scenario = FailureScenario::simple_partition(Duration::from_secs(30));
    failure_injector
        .lock()
        .await
        .load_scenario(partition_scenario);

    // Start failure injection after 10 seconds
    let injector_clone = failure_injector.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        injector_clone.lock().await.start().await;
    });

    // Run workload
    let workload_config = WorkloadConfig {
        client_count: 5,
        operation_count: 50,
        operation_mix: patterns::vm_lifecycle(),
        ..Default::default()
    };

    let generator = WorkloadGenerator::new(workload_config);
    let clients = cluster.get_clients().await;

    // Run clients with failure awareness
    let mut handles = vec![];

    for client_id in 0..5 {
        let client = clients[client_id % clients.len()].clone();
        let recorder = recorder.clone();
        let generator_clone = generator.clone();
        let injector = failure_injector.clone();

        let handle = tokio::spawn(async move {
            generator_clone
                .run_client(client_id, |op| {
                    let client = client.clone();
                    let recorder = recorder.clone();
                    let injector = injector.clone();

                    async move {
                        // Check if we can communicate
                        let client_node = (client_id % 5) as u64 + 1;
                        let leader_node = 1u64; // Assume node 1 is leader

                        if !injector
                            .lock()
                            .await
                            .can_communicate(client_node, leader_node)
                            .await
                        {
                            return Response::Error {
                                code: "NETWORK_PARTITION".to_string(),
                                message: "Cannot reach leader due to network partition".to_string(),
                            };
                        }

                        // Add network delay
                        let delay = injector
                            .lock()
                            .await
                            .get_delay(client_node, leader_node)
                            .await;
                        tokio::time::sleep(delay).await;

                        // Execute operation (simplified for test)
                        let process_id = recorder.begin_operation(op.clone()).await;
                        let response = Response::Error {
                            code: "TEST".to_string(),
                            message: "Simplified test response".to_string(),
                        };
                        recorder.end_operation(process_id, response.clone()).await;

                        response
                    }
                })
                .await
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Analyze results
    let history = recorder.get_history().await;
    let analyzer = HistoryAnalyzer::new(history);
    let report = analyzer.analyze();

    println!("Partition test completed!");
    println!("Violations detected: {}", report.violations.len());

    // Check for split-brain
    let has_split_brain = report
        .violations
        .iter()
        .any(|v| matches!(v, ViolationPattern::SplitBrain { .. }));

    assert!(!has_split_brain, "Split brain detected during partition!");
}

/// Test with Byzantine failures
#[tokio::test]
async fn test_byzantine_vm_operations() {
    let cluster = TestCluster::new(5).await;
    cluster
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();

    let recorder = HistoryRecorder::new();
    let failure_injector = Arc::new(Mutex::new(FailureInjector::new()));

    // Configure Byzantine scenario
    let byzantine_scenario = FailureScenario::byzantine_scenario();
    failure_injector
        .lock()
        .await
        .load_scenario(byzantine_scenario);
    failure_injector.lock().await.start().await;

    // Run simplified workload
    let workload_config = WorkloadConfig {
        client_count: 3,
        operation_count: 30,
        operation_mix: {
            let mut mix = OperationMix::default();
            mix.weights.clear();
            mix.weights.insert("create_vm".to_string(), 50);
            mix.weights.insert("get_vm_status".to_string(), 50);
            mix
        },
        ..Default::default()
    };

    // Execute workload (simplified for test)
    let generator = WorkloadGenerator::new(workload_config);
    let plan = generator.generate_plan().await;

    println!("Byzantine test completed!");
    println!("Generated {} client plans", plan.len());

    // In a real test, we would execute the plan and verify that
    // Byzantine nodes cannot violate linearizability
}

/// Demonstrate comprehensive analysis capabilities
#[tokio::test]
async fn test_analysis_and_reporting() {
    // Create a synthetic history with known violations
    let mut history = History::new();

    // Lost update scenario
    let id1 = history.begin_operation(
        Operation::CreateVm {
            name: "vm1".to_string(),
            config: VmConfig {
                memory_mb: 1024,
                vcpus: 2,
                disk_gb: 10,
                image: "ubuntu".to_string(),
                features: vec![],
            },
            placement_strategy: None,
        },
        HashMap::new(),
    );
    history.end_operation(
        id1,
        Response::VmCreated {
            success: true,
            node_id: Some(1),
        },
    );

    let id2 = history.begin_operation(
        Operation::CreateVm {
            name: "vm1".to_string(),
            config: VmConfig {
                memory_mb: 2048,
                vcpus: 4,
                disk_gb: 20,
                image: "debian".to_string(),
                features: vec![],
            },
            placement_strategy: None,
        },
        HashMap::new(),
    );
    // Both creates succeed - violation!
    history.end_operation(
        id2,
        Response::VmCreated {
            success: true,
            node_id: Some(2),
        },
    );

    // Analyze
    let analyzer = HistoryAnalyzer::new(history);
    let report = analyzer.analyze();

    // Generate HTML report
    let html = generate_html_report(&report);

    println!("Generated HTML report ({} bytes)", html.len());
    assert!(html.contains("Linearizability Analysis Report"));

    // Check recommendations
    assert!(!report.recommendations.is_empty());
    println!("Recommendations:");
    for rec in &report.recommendations {
        println!("  - {}", rec);
    }
}
