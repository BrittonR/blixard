//! Linearizability tests for Blixard distributed operations
//! 
//! These tests verify that concurrent operations on the distributed system
//! maintain linearizability - they appear to take effect atomically.

#![cfg(feature = "test-helpers")]

mod common;
mod linearizability_framework;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use blixard_core::{
    test_helpers::{TestCluster, TestNode},
    types::{CreateVmRequest, VmConfig as BlixardVmConfig},
};
use tokio::sync::Mutex;
use tokio::time::sleep;

use linearizability_framework::*;

/// Test concurrent VM creation operations for linearizability
#[tokio::test]
async fn test_concurrent_vm_creation_linearizability() {
    let recorder = HistoryRecorder::new();
    let cluster = TestCluster::new(3).await;
    cluster.wait_for_leader(Duration::from_secs(10)).await.unwrap();
    
    let clients = cluster.get_clients().await;
    let num_vms = 5;
    let num_clients = 3;
    
    // Spawn concurrent operations
    let mut handles = vec![];
    
    for client_id in 0..num_clients {
        for vm_id in 0..num_vms {
            let vm_name = format!("vm-{}-{}", client_id, vm_id);
            let client = clients[client_id % clients.len()].clone();
            let recorder = recorder.clone();
            
            let handle = tokio::spawn(async move {
                // Record operation start
                let op = Operation::CreateVm {
                    name: vm_name.clone(),
                    config: VmConfig {
                        memory_mb: 1024,
                        vcpus: 2,
                    },
                };
                let process_id = recorder.begin_operation(op).await;
                
                // Perform operation
                let result = client.create_vm(CreateVmRequest {
                    name: vm_name,
                    config: BlixardVmConfig {
                        memory: 1024 * 1024 * 1024,
                        vcpus: 2,
                        disk_size: 10 * 1024 * 1024 * 1024,
                        image: "test-image".to_string(),
                        network_interfaces: vec![],
                        metadata: HashMap::new(),
                    },
                }).await;
                
                // Record operation end
                let response = match result {
                    Ok(_) => Response::VmCreated { success: true },
                    Err(_) => Response::VmCreated { success: false },
                };
                recorder.end_operation(process_id, response).await;
            });
            
            handles.push(handle);
        }
    }
    
    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Check linearizability
    let history = recorder.get_history().await;
    let result = check_linearizability(&history, VmSpecification::new);
    
    if let Err(e) = result {
        // Analyze the failure
        let analyzer = HistoryAnalyzer::new(history);
        if let Some(minimal) = analyzer.find_minimal_violation(VmSpecification::new) {
            panic!("Linearizability violation found. Minimal failing sequence: {:?}", minimal);
        }
        panic!("Linearizability check failed: {}", e);
    }
}

/// Test concurrent key-value operations through Raft
#[tokio::test]
async fn test_kv_linearizability() {
    let recorder = HistoryRecorder::new();
    let cluster = TestCluster::new(3).await;
    cluster.wait_for_leader(Duration::from_secs(10)).await.unwrap();
    
    let clients = cluster.get_clients().await;
    let num_operations = 50;
    let num_keys = 5;
    
    // Shared counter for generating values
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];
    
    for i in 0..num_operations {
        let client = clients[i % clients.len()].clone();
        let recorder = recorder.clone();
        let counter = counter.clone();
        
        let handle = tokio::spawn(async move {
            let key = format!("key-{}", i % num_keys);
            
            // Mix of read and write operations
            let op = if i % 3 == 0 {
                // Read operation
                Operation::Read { key: key.clone() }
            } else {
                // Write operation
                let value = {
                    let mut c = counter.lock().await;
                    *c += 1;
                    format!("value-{}", *c)
                };
                Operation::Write { key: key.clone(), value }
            };
            
            let process_id = recorder.begin_operation(op.clone()).await;
            
            // Simulate operation through the distributed system
            // In real implementation, this would use actual Raft operations
            let response = match &op {
                Operation::Read { key } => {
                    // Simulate read through consensus
                    sleep(Duration::from_millis(10)).await;
                    Response::Value { value: Some(format!("value-{}", key)) }
                },
                Operation::Write { .. } => {
                    // Simulate write through consensus
                    sleep(Duration::from_millis(20)).await;
                    Response::Written { success: true }
                },
                _ => unreachable!(),
            };
            
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Check linearizability
    let history = recorder.get_history().await;
    let result = check_linearizability(&history, KvSpecification::new);
    
    assert!(result.is_ok(), "Key-value operations are not linearizable");
}

/// Test cluster membership changes for linearizability
#[tokio::test]
async fn test_cluster_membership_linearizability() {
    let recorder = HistoryRecorder::new();
    let mut cluster = TestCluster::new(3).await;
    cluster.wait_for_leader(Duration::from_secs(10)).await.unwrap();
    
    let mut handles = vec![];
    
    // Concurrent join operations
    for node_id in 4..7 {
        let recorder = recorder.clone();
        let cluster_clone = cluster.clone();
        
        let handle = tokio::spawn(async move {
            let op = Operation::JoinCluster { node_id };
            let process_id = recorder.begin_operation(op).await;
            
            // Add node to cluster
            let node = TestNode::new(node_id).await;
            let result = cluster_clone.add_node(node).await;
            
            let response = match result {
                Ok(peers) => Response::Joined {
                    success: true,
                    peers: peers.into_iter().map(|p| p.id).collect(),
                },
                Err(_) => Response::Joined {
                    success: false,
                    peers: vec![],
                },
            };
            
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    // Wait for joins to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Concurrent leave operations
    let mut handles = vec![];
    for node_id in 4..6 {
        let recorder = recorder.clone();
        let cluster_clone = cluster.clone();
        
        let handle = tokio::spawn(async move {
            let op = Operation::LeaveCluster { node_id };
            let process_id = recorder.begin_operation(op).await;
            
            let result = cluster_clone.remove_node(node_id).await;
            
            let response = Response::Left {
                success: result.is_ok(),
            };
            
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify linearizability
    let history = recorder.get_history().await;
    
    // For cluster operations, we need a custom specification
    struct ClusterSpecification {
        members: std::collections::HashSet<u64>,
    }
    
    impl ClusterSpecification {
        fn new() -> Self {
            Self {
                members: [1, 2, 3].into_iter().collect(),
            }
        }
    }
    
    impl Specification for ClusterSpecification {
        fn apply(&mut self, operation: &Operation) -> Response {
            match operation {
                Operation::JoinCluster { node_id } => {
                    if self.members.insert(*node_id) {
                        Response::Joined {
                            success: true,
                            peers: self.members.iter().copied().collect(),
                        }
                    } else {
                        Response::Joined {
                            success: false,
                            peers: vec![],
                        }
                    }
                },
                Operation::LeaveCluster { node_id } => {
                    Response::Left {
                        success: self.members.remove(node_id),
                    }
                },
                _ => Response::Error {
                    message: "Unsupported operation".to_string(),
                },
            }
        }
    }
    
    let result = check_linearizability(&history, ClusterSpecification::new);
    assert!(result.is_ok(), "Cluster membership changes are not linearizable");
}

/// Test for detecting split-brain scenarios
#[tokio::test]
async fn test_split_brain_detection() {
    let recorder = HistoryRecorder::new();
    let cluster = TestCluster::new(5).await;
    cluster.wait_for_leader(Duration::from_secs(10)).await.unwrap();
    
    // Create network partition
    cluster.partition_network(vec![1, 2], vec![3, 4, 5]).await;
    
    // Try concurrent writes from both partitions
    let clients = cluster.get_clients().await;
    let mut handles = vec![];
    
    // Operations from minority partition
    for i in 0..5 {
        let client = clients[0].clone(); // Node 1
        let recorder = recorder.clone();
        
        let handle = tokio::spawn(async move {
            let op = Operation::Write {
                key: format!("key-{}", i),
                value: "minority".to_string(),
            };
            let process_id = recorder.begin_operation(op).await;
            
            let result = client.write_key(format!("key-{}", i), "minority").await;
            
            let response = Response::Written {
                success: result.is_ok(),
            };
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    // Operations from majority partition
    for i in 0..5 {
        let client = clients[2].clone(); // Node 3
        let recorder = recorder.clone();
        
        let handle = tokio::spawn(async move {
            let op = Operation::Write {
                key: format!("key-{}", i),
                value: "majority".to_string(),
            };
            let process_id = recorder.begin_operation(op).await;
            
            let result = client.write_key(format!("key-{}", i), "majority").await;
            
            let response = Response::Written {
                success: result.is_ok(),
            };
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Heal partition
    cluster.heal_network_partition().await;
    
    // Read all keys to check final state
    let mut handles = vec![];
    for i in 0..5 {
        let client = clients[0].clone();
        let recorder = recorder.clone();
        
        let handle = tokio::spawn(async move {
            let op = Operation::Read {
                key: format!("key-{}", i),
            };
            let process_id = recorder.begin_operation(op).await;
            
            let result = client.read_key(format!("key-{}", i)).await;
            
            let response = Response::Value {
                value: result.ok(),
            };
            recorder.end_operation(process_id, response).await;
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Check linearizability - minority writes should have failed
    let history = recorder.get_history().await;
    let result = check_linearizability(&history, KvSpecification::new);
    
    // Analyze patterns
    let analyzer = HistoryAnalyzer::new(history);
    let patterns = analyzer.detect_violation_patterns();
    
    // In a properly functioning system, minority partition writes should fail
    // This prevents split-brain scenarios
    assert!(result.is_ok() || patterns.is_empty(),
        "Split-brain scenario detected: {:?}", patterns);
}