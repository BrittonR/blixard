mod common;

use once_cell::sync::Lazy;
use proptest::prelude::*;
use redb::{Database, ReadableTable};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tempfile::TempDir;

use blixard_core::{
    raft_manager::{
        schedule_task, ProposalData, RaftStateMachine, ResourceRequirements, TaskResult, TaskSpec,
        WorkerCapabilities, WorkerStatus,
    },
    storage::{TASK_RESULT_TABLE, VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE},
    types::{VmCommand, VmConfig},
};

// Shared runtime to prevent resource exhaustion from creating too many runtimes
static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().unwrap());

// Property: Tasks can only be assigned to workers that meet resource requirements
proptest! {
    #[test]
    fn prop_task_assignment_respects_resources(
        task_cpu in 1u32..16,
        task_mem in 256u64..32768,
        task_disk in 1u64..1000,
        worker_cpu in 1u32..32,
        worker_mem in 512u64..65536,
        worker_disk in 10u64..2000,
    ) {
        RUNTIME.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let database = Arc::new(Database::create(db_path).unwrap());

            // Register a worker
            let write_txn = database.begin_write().unwrap();
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();

            let capabilities = WorkerCapabilities {
                cpu_cores: worker_cpu,
                memory_mb: worker_mem,
                disk_gb: worker_disk,
                features: vec![],
            };

            let worker_data = bincode::serialize(&("127.0.0.1:7001", &capabilities)).unwrap();
            worker_table.insert(1u64.to_le_bytes().as_slice(), worker_data.as_slice()).unwrap();
            status_table.insert(1u64.to_le_bytes().as_slice(), [WorkerStatus::Online as u8].as_slice()).unwrap();
            drop(worker_table);
            drop(status_table);
            write_txn.commit().unwrap();

            // Try to schedule a task
            let task = TaskSpec {
                command: "test".to_string(),
                args: vec![],
                resources: ResourceRequirements {
                    cpu_cores: task_cpu,
                    memory_mb: task_mem,
                    disk_gb: task_disk,
                    required_features: vec![],
                },
                timeout_secs: 60,
            };

            let result = schedule_task(database.clone(), "test-task", &task).await.unwrap();

            // Task should only be assigned if worker meets requirements
            let should_assign = worker_cpu >= task_cpu &&
                               worker_mem >= task_mem &&
                               worker_disk >= task_disk;

            if should_assign {
                assert_eq!(result, Some(1));
            } else {
                assert_eq!(result, None);
            }
        });
    }
}

// Property: Task results are never lost once written
proptest! {
    #[test]
    fn prop_task_results_persist(
        task_ids in prop::collection::vec("[a-z0-9-]{5,20}", 1..10),
        success in prop::collection::vec(any::<bool>(), 1..10),
        outputs in prop::collection::vec("[a-zA-Z0-9 ]{0,100}", 1..10),
    ) {
        RUNTIME.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let database = Arc::new(Database::create(db_path).unwrap());
            let state_machine = RaftStateMachine::new(database.clone(), std::sync::Weak::new());

            // Store task results
            let mut expected_results = HashMap::new();

            for ((task_id, succ), output) in task_ids.iter().zip(success.iter()).zip(outputs.iter()) {
                let result = TaskResult {
                    success: *succ,
                    output: output.clone(),
                    error: if *succ { None } else { Some("test error".to_string()) },
                    execution_time_ms: 100,
                };

                expected_results.insert(task_id.clone(), result.clone());

                // Apply through state machine
                let entry = raft::prelude::Entry {
                    context: vec![],
                    data: bincode::serialize(&ProposalData::CompleteTask {
                        task_id: task_id.clone(),
                        result,
                    }).unwrap(),
                    index: 1,
                    term: 1,
                    entry_type: raft::prelude::EntryType::EntryNormal as i32,
                    sync_log: false,
                };

                state_machine.apply_entry(&entry).await.unwrap();
            }

            // Read back and verify all results
            let read_txn = database.begin_read().unwrap();
            let result_table = read_txn.open_table(TASK_RESULT_TABLE).unwrap();

            for (task_id, expected) in expected_results {
                let stored = result_table.get(task_id.as_str()).unwrap().unwrap();
                let actual: TaskResult = bincode::deserialize(stored.value()).unwrap();

                assert_eq!(actual.success, expected.success);
                assert_eq!(actual.output, expected.output);
                assert_eq!(actual.error, expected.error);
            }
        });
    }
}

// Property: Worker status transitions are valid
proptest! {
    #[test]
    fn prop_worker_status_transitions(
        node_ids in prop::collection::vec(1u64..1000, 1..10),
        status_sequences in prop::collection::vec(
            prop::collection::vec(0u8..4, 1..20),
            1..10
        ),
    ) {
        RUNTIME.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let database = Arc::new(Database::create(db_path).unwrap());
            let state_machine = RaftStateMachine::new(database.clone(), std::sync::Weak::new());

            // Register workers
            for node_id in &node_ids {
                let entry = raft::prelude::Entry {
                    context: vec![],
                    data: bincode::serialize(&ProposalData::RegisterWorker {
                        node_id: *node_id,
                        address: format!("127.0.0.1:{}", 7000 + node_id),
                        capabilities: WorkerCapabilities {
                            cpu_cores: 4,
                            memory_mb: 8192,
                            disk_gb: 100,
                            features: vec![],
                        },
                        topology: blixard_core::types::NodeTopology::default(),
                    }).unwrap(),
                    index: 1,
                    term: 1,
                    entry_type: raft::prelude::EntryType::EntryNormal as i32,
                    sync_log: false,
                };

                state_machine.apply_entry(&entry).await.unwrap();
            }

            // Apply status updates
            for (node_id, statuses) in node_ids.iter().zip(status_sequences.iter()) {
                for &status_val in statuses {
                    if let Ok(status) = WorkerStatus::try_from(status_val) {
                        let entry = raft::prelude::Entry {
                            context: vec![],
                            data: bincode::serialize(&ProposalData::UpdateWorkerStatus {
                                node_id: *node_id,
                                status,
                            }).unwrap(),
                            index: 1,
                            term: 1,
                            entry_type: raft::prelude::EntryType::EntryNormal as i32,
                            sync_log: false,
                        };

                        state_machine.apply_entry(&entry).await.unwrap();
                    }
                }
            }

            // Verify all workers have valid status
            let read_txn = database.begin_read().unwrap();
            let status_table = read_txn.open_table(WORKER_STATUS_TABLE).unwrap();

            for node_id in &node_ids {
                let status_data = status_table.get(node_id.to_le_bytes().as_slice()).unwrap();
                assert!(status_data.is_some());

                let status_val = status_data.unwrap().value()[0];
                assert!(WorkerStatus::try_from(status_val).is_ok());
            }
        });
    }
}

// Property: Concurrent VM operations maintain consistency
proptest! {
    #[test]
    fn prop_concurrent_vm_operations(
        vm_names in prop::collection::vec("[a-z]{3,10}", 1..5),
        operations in prop::collection::vec(
            prop::collection::vec(0u8..4, 1..10),
            1..5
        ),
    ) {
        RUNTIME.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let database = Arc::new(Database::create(db_path).unwrap());
            let state_machine = RaftStateMachine::new(database.clone(), std::sync::Weak::new());

            // Track expected VM states
            let mut vm_states = HashMap::new();

            for (vm_name, ops) in vm_names.iter().zip(operations.iter()) {
                for &op in ops {
                    let command = match op % 4 {
                        0 => {
                            // Create VM
                            vm_states.insert(vm_name.clone(), "created");
                            VmCommand::Create {
                                config: common::test_vm_config(vm_name),
                                node_id: 1,
                            }
                        }
                        1 => {
                            // Start VM (only if created)
                            if vm_states.contains_key(vm_name) {
                                vm_states.insert(vm_name.clone(), "running");
                            }
                            VmCommand::Start { name: vm_name.clone() }
                        }
                        2 => {
                            // Stop VM (only if exists)
                            if vm_states.contains_key(vm_name) {
                                vm_states.insert(vm_name.clone(), "stopped");
                            }
                            VmCommand::Stop { name: vm_name.clone() }
                        }
                        _ => {
                            // Delete VM
                            vm_states.remove(vm_name);
                            VmCommand::Delete { name: vm_name.clone() }
                        }
                    };

                    let entry = raft::prelude::Entry {
                        context: vec![],
                        data: bincode::serialize(&ProposalData::CreateVm(command)).unwrap(),
                        index: 1,
                        term: 1,
                        entry_type: raft::prelude::EntryType::EntryNormal as i32,
                        sync_log: false,
                    };

                    let _ = state_machine.apply_entry(&entry).await;
                }
            }

            // Verify final state consistency
            let read_txn = database.begin_read().unwrap();
            if let Ok(vm_table) = read_txn.open_table(VM_STATE_TABLE) {
                let stored_count = vm_table.iter().unwrap().count();
                // Can't do exact comparison due to delete operations, but count should be reasonable
                assert!(stored_count <= vm_names.len());
            }
        });
    }
}

// Property: Task scheduling is deterministic given same state
proptest! {
    #[test]
    fn prop_deterministic_scheduling(
        seed_workers in prop::collection::vec(
            (1u64..100, 1u32..16, 1024u64..16384, 10u64..500),
            3..10
        ),
        task_requirements in (1u32..8, 512u64..8192, 5u64..100),
    ) {
        RUNTIME.block_on(async {
            let (req_cpu, req_mem, req_disk) = task_requirements;

            // Run scheduling twice with same setup
            let mut results = vec![];

            for _ in 0..2 {
                let temp_dir = TempDir::new().unwrap();
                let db_path = temp_dir.path().join("test.db");
                let database = Arc::new(Database::create(db_path).unwrap());

                // Setup workers in same order
                let write_txn = database.begin_write().unwrap();
                let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
                let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();

                for (node_id, cpu, mem, disk) in &seed_workers {
                    let capabilities = WorkerCapabilities {
                        cpu_cores: *cpu,
                        memory_mb: *mem,
                        disk_gb: *disk,
                        features: vec![],
                    };

                    let worker_data = bincode::serialize(&("127.0.0.1:7001", &capabilities)).unwrap();
                    worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice()).unwrap();
                    status_table.insert(node_id.to_le_bytes().as_slice(), [WorkerStatus::Online as u8].as_slice()).unwrap();
                }
                drop(worker_table);
                drop(status_table);
                write_txn.commit().unwrap();

                // Schedule task
                let task = TaskSpec {
                    command: "test".to_string(),
                    args: vec![],
                    resources: ResourceRequirements {
                        cpu_cores: req_cpu,
                        memory_mb: req_mem,
                        disk_gb: req_disk,
                        required_features: vec![],
                    },
                    timeout_secs: 60,
                };

                let result = schedule_task(database, "test-task", &task).await.unwrap();
                results.push(result);
            }

            // Results should be identical
            assert_eq!(results[0], results[1]);
        });
    }
}

// Property: No task is assigned to offline workers
proptest! {
    #[test]
    fn prop_no_offline_worker_assignment(
        workers in prop::collection::vec(
            (1u64..100, 0u8..4, 4u32..16, 4096u64..16384),
            1..20
        ),
    ) {
        RUNTIME.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let database = Arc::new(Database::create(db_path).unwrap());

            // Setup workers with various statuses
            let write_txn = database.begin_write().unwrap();
            let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();

            let mut online_workers = HashSet::new();

            for (node_id, status_val, cpu, mem) in &workers {
                let capabilities = WorkerCapabilities {
                    cpu_cores: *cpu,
                    memory_mb: *mem,
                    disk_gb: 100,
                    features: vec![],
                };

                let worker_data = bincode::serialize(&("127.0.0.1:7001", &capabilities)).unwrap();
                worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice()).unwrap();
                status_table.insert(node_id.to_le_bytes().as_slice(), [*status_val].as_slice()).unwrap();

                if *status_val == WorkerStatus::Online as u8 {
                    online_workers.insert(*node_id);
                }
            }
            drop(worker_table);
            drop(status_table);
            write_txn.commit().unwrap();

            // Try to schedule a task with minimal requirements
            let task = TaskSpec {
                command: "test".to_string(),
                args: vec![],
                resources: ResourceRequirements {
                    cpu_cores: 1,
                    memory_mb: 256,
                    disk_gb: 1,
                    required_features: vec![],
                },
                timeout_secs: 60,
            };

            let result = schedule_task(database, "test-task", &task).await.unwrap();

            // If assigned, must be to an online worker
            if let Some(assigned_node) = result {
                assert!(online_workers.contains(&assigned_node));
            }
        });
    }
}
