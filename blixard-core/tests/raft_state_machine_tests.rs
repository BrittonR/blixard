#![cfg(feature = "test-helpers")]

use std::sync::Arc;
use tempfile::TempDir;
use redb::Database;
use blixard_core::raft_manager::{RaftStateMachine, ProposalData, TaskSpec, ResourceRequirements, TaskResult, WorkerCapabilities, WorkerStatus};
use blixard_core::types::{VmCommand, VmConfig, VmStatus};
use blixard_core::storage::{TASK_TABLE, TASK_ASSIGNMENT_TABLE, TASK_RESULT_TABLE, WORKER_TABLE, WORKER_STATUS_TABLE};
use raft::prelude::Entry;

/// Create a test database with required tables
fn create_test_db() -> (TempDir, Arc<Database>) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(db_path).unwrap());
    
    // Initialize tables
    let write_txn = db.begin_write().unwrap();
    let _ = write_txn.open_table(TASK_TABLE).unwrap();
    let _ = write_txn.open_table(TASK_ASSIGNMENT_TABLE).unwrap();
    let _ = write_txn.open_table(TASK_RESULT_TABLE).unwrap();
    let _ = write_txn.open_table(WORKER_TABLE).unwrap();
    let _ = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    write_txn.commit().unwrap();
    
    (temp_dir, db)
}

/// Create a Raft entry with the given proposal
fn create_entry(proposal: ProposalData) -> Entry {
    let mut entry = Entry::default();
    entry.data = bincode::serialize(&proposal).unwrap();
    entry
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_assign_task() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Create a task assignment proposal
    let proposal = ProposalData::AssignTask {
        task_id: "task-1".to_string(),
        node_id: 1,
        task: TaskSpec {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 100,
                disk_gb: 0,
                required_features: vec![],
            },
            timeout_secs: 60,
        },
    };
    
    let entry = create_entry(proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify task was stored
    let read_txn = db.begin_read().unwrap();
    let task_table = read_txn.open_table(TASK_TABLE).unwrap();
    assert!(task_table.get("task-1").unwrap().is_some());
    
    // Verify assignment was stored
    let assignment_table = read_txn.open_table(TASK_ASSIGNMENT_TABLE).unwrap();
    let assignment = assignment_table.get("task-1").unwrap().unwrap();
    let node_id = u64::from_le_bytes(assignment.value().try_into().unwrap());
    assert_eq!(node_id, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_complete_task() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // First assign a task
    let assign_proposal = ProposalData::AssignTask {
        task_id: "task-1".to_string(),
        node_id: 1,
        task: TaskSpec {
            command: "test".to_string(),
            args: vec![],
            resources: ResourceRequirements {
                cpu_cores: 1,
                memory_mb: 100,
                disk_gb: 0,
                required_features: vec![],
            },
            timeout_secs: 60,
        },
    };
    
    let entry = create_entry(assign_proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Now complete the task
    let complete_proposal = ProposalData::CompleteTask {
        task_id: "task-1".to_string(),
        result: TaskResult {
            success: true,
            output: "Success".to_string(),
            error: None,
            execution_time_ms: 100,
        },
    };
    
    let entry = create_entry(complete_proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify result was stored
    let read_txn = db.begin_read().unwrap();
    let result_table = read_txn.open_table(TASK_RESULT_TABLE).unwrap();
    assert!(result_table.get("task-1").unwrap().is_some());
    
    // Verify assignment was removed
    let assignment_table = read_txn.open_table(TASK_ASSIGNMENT_TABLE).unwrap();
    assert!(assignment_table.get("task-1").unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_register_worker() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    let proposal = ProposalData::RegisterWorker {
        node_id: 1,
        address: "worker-1:8080".to_string(),
        capabilities: WorkerCapabilities {
            cpu_cores: 4,
            memory_mb: 8192,
            disk_gb: 100,
            features: vec!["gpu".to_string()],
        },
    };
    
    let entry = create_entry(proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify worker was stored
    let read_txn = db.begin_read().unwrap();
    let worker_table = read_txn.open_table(WORKER_TABLE).unwrap();
    assert!(worker_table.get(1u64.to_le_bytes().as_slice()).unwrap().is_some());
    
    // Verify initial status is Online
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    let status_bytes = status_table.get(1u64.to_le_bytes().as_slice()).unwrap().unwrap();
    assert_eq!(status_bytes.value()[0], WorkerStatus::Online as u8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_update_worker_status() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // First register a worker
    let register_proposal = ProposalData::RegisterWorker {
        node_id: 1,
        address: "worker-1:8080".to_string(),
        capabilities: WorkerCapabilities {
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 50,
            features: vec![],
        },
    };
    
    let entry = create_entry(register_proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Update worker status
    let update_proposal = ProposalData::UpdateWorkerStatus {
        node_id: 1,
        status: WorkerStatus::Busy,
    };
    
    let entry = create_entry(update_proposal);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify status was updated
    let read_txn = db.begin_read().unwrap();
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    let status_bytes = status_table.get(1u64.to_le_bytes().as_slice()).unwrap().unwrap();
    assert_eq!(status_bytes.value()[0], WorkerStatus::Busy as u8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_vm_command_create() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    let proposal = ProposalData::CreateVm(VmCommand::Create {
        config: VmConfig {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory: 1024,
        },
        node_id: 1,
    });
    
    let entry = create_entry(proposal);
    
    // VM operations are stubs, just verify no errors
    state_machine.apply_entry(&entry).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apply_empty_entry() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Empty entries should be no-ops
    let entry = Entry::default();
    state_machine.apply_entry(&entry).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_complete_nonexistent_task() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Try to complete a task that was never assigned
    let complete_proposal = ProposalData::CompleteTask {
        task_id: "nonexistent".to_string(),
        result: TaskResult {
            success: true,
            output: "Success".to_string(),
            error: None,
            execution_time_ms: 100,
        },
    };
    
    let entry = create_entry(complete_proposal);
    
    // Should not panic, just store the result
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify result was stored even though task wasn't assigned
    let read_txn = db.begin_read().unwrap();
    let result_table = read_txn.open_table(TASK_RESULT_TABLE).unwrap();
    assert!(result_table.get("nonexistent").unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_update_nonexistent_worker_status() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Try to update status of non-existent worker
    let update_proposal = ProposalData::UpdateWorkerStatus {
        node_id: 999,
        status: WorkerStatus::Failed,
    };
    
    let entry = create_entry(update_proposal);
    
    // Should create the status entry even if worker doesn't exist
    state_machine.apply_entry(&entry).await.unwrap();
    
    let read_txn = db.begin_read().unwrap();
    let status_table = read_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    let status_bytes = status_table.get(999u64.to_le_bytes().as_slice()).unwrap().unwrap();
    assert_eq!(status_bytes.value()[0], WorkerStatus::Failed as u8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_idempotent_task_assignment() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    let task = TaskSpec {
        command: "test".to_string(),
        args: vec![],
        resources: ResourceRequirements {
            cpu_cores: 1,
            memory_mb: 100,
            disk_gb: 0,
            required_features: vec![],
        },
        timeout_secs: 60,
    };
    
    // Apply the same task assignment twice
    for _ in 0..2 {
        let proposal = ProposalData::AssignTask {
            task_id: "task-1".to_string(),
            node_id: 1,
            task: task.clone(),
        };
        
        let entry = create_entry(proposal);
        state_machine.apply_entry(&entry).await.unwrap();
    }
    
    // Verify only one entry exists
    let read_txn = db.begin_read().unwrap();
    let task_table = read_txn.open_table(TASK_TABLE).unwrap();
    let assignment_table = read_txn.open_table(TASK_ASSIGNMENT_TABLE).unwrap();
    
    assert!(task_table.get("task-1").unwrap().is_some());
    assert!(assignment_table.get("task-1").unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_worker_registration_overwrites() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Register worker with initial capabilities
    let proposal1 = ProposalData::RegisterWorker {
        node_id: 1,
        address: "worker-1:8080".to_string(),
        capabilities: WorkerCapabilities {
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 50,
            features: vec![],
        },
    };
    
    let entry = create_entry(proposal1);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Re-register with different capabilities
    let proposal2 = ProposalData::RegisterWorker {
        node_id: 1,
        address: "worker-1:9090".to_string(),  // Different port
        capabilities: WorkerCapabilities {
            cpu_cores: 4,  // More cores
            memory_mb: 8192,  // More memory
            disk_gb: 100,
            features: vec!["gpu".to_string()],
        },
    };
    
    let entry = create_entry(proposal2);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify updated capabilities
    let read_txn = db.begin_read().unwrap();
    let worker_table = read_txn.open_table(WORKER_TABLE).unwrap();
    let worker_data = worker_table.get(1u64.to_le_bytes().as_slice()).unwrap().unwrap();
    
    let (address, capabilities): (String, WorkerCapabilities) = 
        bincode::deserialize(worker_data.value()).unwrap();
    
    assert_eq!(address, "worker-1:9090");
    assert_eq!(capabilities.cpu_cores, 4);
    assert_eq!(capabilities.memory_mb, 8192);
    assert_eq!(capabilities.features, vec!["gpu"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_task_result_overwrites() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // First result - failure
    let result1 = ProposalData::CompleteTask {
        task_id: "task-1".to_string(),
        result: TaskResult {
            success: false,
            output: "".to_string(),
            error: Some("Connection timeout".to_string()),
            execution_time_ms: 5000,
        },
    };
    
    let entry = create_entry(result1);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Second result - success (maybe a retry)
    let result2 = ProposalData::CompleteTask {
        task_id: "task-1".to_string(),
        result: TaskResult {
            success: true,
            output: "Success on retry".to_string(),
            error: None,
            execution_time_ms: 200,
        },
    };
    
    let entry = create_entry(result2);
    state_machine.apply_entry(&entry).await.unwrap();
    
    // Verify latest result is stored
    let read_txn = db.begin_read().unwrap();
    let result_table = read_txn.open_table(TASK_RESULT_TABLE).unwrap();
    let result_data = result_table.get("task-1").unwrap().unwrap();
    
    let result: TaskResult = bincode::deserialize(result_data.value()).unwrap();
    assert!(result.success);
    assert_eq!(result.output, "Success on retry");
    assert_eq!(result.error, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_vm_operations() {
    let (_temp_dir, db) = create_test_db();
    let state_machine = RaftStateMachine::new(db.clone(), std::sync::Weak::new());
    
    // Create VM
    let create = ProposalData::CreateVm(VmCommand::Create {
        config: VmConfig {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory: 1024,
        },
        node_id: 1,
    });
    
    state_machine.apply_entry(&create_entry(create)).await.unwrap();
    
    // Update VM status
    let update = ProposalData::UpdateVmStatus {
        vm_name: "test-vm".to_string(),
        status: VmStatus::Running,
        node_id: 1,
    };
    
    state_machine.apply_entry(&create_entry(update)).await.unwrap();
    
    // Stop VM
    let stop = ProposalData::CreateVm(VmCommand::Stop {
        name: "test-vm".to_string(),
    });
    
    state_machine.apply_entry(&create_entry(stop)).await.unwrap();
    
    // Delete VM
    let delete = ProposalData::CreateVm(VmCommand::Delete {
        name: "test-vm".to_string(),
    });
    
    state_machine.apply_entry(&create_entry(delete)).await.unwrap();
    
    // All operations should complete without error
}