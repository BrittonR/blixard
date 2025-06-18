#![cfg(feature = "test-helpers")]

use blixard::raft_manager::{ProposalData, WorkerStatus, ResourceRequirements, TaskSpec};

mod common;
use common::raft_test_utils::{RaftTestHarness, ProposalGenerator};

#[tokio::test]
async fn test_apply_task_proposal() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Submit a task
    let _task_id = harness.submit_task("test-task").await.unwrap();
    
    // Wait for task to be applied
    harness.wait_for_commits(2).await.unwrap(); // bootstrap + task
    
    // Verify task was created
    let _tasks = harness.get_tasks().await;
    // TODO: When task retrieval is implemented, verify the task was created
    // For now, just verify the proposal was accepted without error
}

#[tokio::test]
async fn test_apply_worker_registration() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Register a worker
    harness.register_worker("worker-1", 100).await.unwrap();
    
    // Wait for worker registration to be applied
    harness.wait_for_commits(2).await.unwrap(); // bootstrap + worker
    
    // Verify worker was registered
    let _workers = harness.get_workers().await;
    // TODO: When worker retrieval is implemented, verify the worker was registered
    // For now, just verify the proposal was accepted without error
}

#[tokio::test]
async fn test_apply_vm_operations() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Submit VM create command
    harness.submit_vm_command("vm-1", "create").await.unwrap();
    
    // Wait for VM command to be applied
    harness.wait_for_commits(2).await.unwrap(); // bootstrap + vm command
    
    // Verify VM was created (note: actual VM creation is stubbed)
    let _vms = harness.get_vms().await;
    // VM manager is currently a stub, so this might be empty
    // This test verifies the proposal was processed without errors
    
    // Verify no pending proposals remain
    assert_eq!(harness.get_pending_proposals_count().await, 0);
}

#[tokio::test]
async fn test_apply_invalid_proposal() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Try to complete a non-existent task
    let proposal = ProposalGenerator::complete_task_proposal("non-existent", true);
    let result = harness.raft_manager.propose(proposal).await;
    
    // Should succeed at proposal level (will be rejected during apply)
    assert!(result.is_ok());
    
    // Wait for proposal to be processed
    harness.wait_for_commits(2).await.unwrap(); // bootstrap + invalid proposal
    
    // Verify no tasks were affected
    // TODO: When task retrieval is implemented, verify no tasks exist
}

#[tokio::test]
async fn test_state_machine_persistence() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Create various state
    let _task_id = harness.submit_task("persistent-task").await.unwrap();
    harness.register_worker("persistent-worker", 50).await.unwrap();
    
    // Wait for all operations to be applied
    harness.wait_for_commits(3).await.unwrap(); // bootstrap + task + worker
    
    // Verify state was persisted
    // TODO: When state retrieval is implemented, verify persistence
    // For now, just verify the proposals were accepted
}

#[tokio::test]
async fn test_task_assignment_with_requirements() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Register a worker with specific capacity
    harness.register_worker("worker-1", 200).await.unwrap();
    
    // Submit a task with requirements
    let proposal = ProposalData::AssignTask {
        task_id: "task-with-reqs".to_string(),
        node_id: 1,
        task: TaskSpec {
            command: "Resource Heavy Task".to_string(),
            args: vec![],
            resources: ResourceRequirements {
                cpu_cores: 2,
                memory_mb: 150,
                disk_gb: 0,
                required_features: vec![],
            },
            timeout_secs: 300,
        },
    };
    
    harness.raft_manager.propose(proposal).await.unwrap();
    
    // Wait for operations to be applied
    harness.wait_for_commits(3).await.unwrap(); // bootstrap + worker + task
    
    // Verify task was created with requirements
    // TODO: When task retrieval is implemented, verify requirements
}

#[tokio::test]
async fn test_worker_status_updates() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Register a worker
    harness.register_worker("worker-1", 100).await.unwrap();
    
    // Update worker status to unhealthy
    harness.update_worker_status("worker-1", WorkerStatus::Failed).await.unwrap();
    
    // Wait for updates to be applied
    harness.wait_for_commits(3).await.unwrap(); // bootstrap + register + update
    
    // Verify worker status was updated
    // TODO: When worker retrieval is implemented, verify status update
}

#[tokio::test]
async fn test_task_completion_updates_worker() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Register a worker and create a task
    harness.register_worker("worker-1", 100).await.unwrap();
    let task_id = harness.submit_task("test-task").await.unwrap();
    
    // Manually assign task to worker (simulating scheduler)
    // In real system, this would be done by the scheduler
    
    // Complete the task
    let proposal = ProposalGenerator::complete_task_proposal(&task_id, true);
    harness.raft_manager.propose(proposal).await.unwrap();
    
    // Wait for all operations
    harness.wait_for_commits(4).await.unwrap(); // bootstrap + worker + task + complete
    
    // Verify task status
    // TODO: When task retrieval is implemented, verify task completion
}

#[tokio::test]
async fn test_concurrent_state_modifications() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Submit multiple operations concurrently
    let mut handles = vec![];
    
    // Clone Arc<RaftManager> for concurrent access
    let rm1 = harness.raft_manager.clone();
    let rm2 = harness.raft_manager.clone();
    let rm3 = harness.raft_manager.clone();
    
    // Task creation
    handles.push(tokio::spawn(async move {
        rm1.propose(ProposalGenerator::task_proposal("concurrent-1", "Task 1")).await
    }));
    
    // Worker registration
    handles.push(tokio::spawn(async move {
        rm2.propose(ProposalGenerator::register_worker_proposal("concurrent-worker", 100)).await
    }));
    
    // Another task
    handles.push(tokio::spawn(async move {
        rm3.propose(ProposalGenerator::task_proposal("concurrent-2", "Task 2")).await
    }));
    
    // Wait for all proposals to complete
    for handle in handles {
        handle.await.unwrap().unwrap();
    }
    
    // Wait for all to be committed
    harness.wait_for_commits(4).await.unwrap(); // bootstrap + 3 operations
    
    // Verify all operations were applied
    // TODO: When state retrieval is implemented, verify all operations
    // For now, just verify no errors occurred
}

#[tokio::test]
async fn test_vm_status_transitions() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Create VM through command
    harness.submit_vm_command("vm-1", "create").await.unwrap();
    
    // Update VM status through another VM operation
    let proposal = ProposalGenerator::vm_command_proposal("vm-1", "start");
    harness.raft_manager.propose(proposal).await.unwrap();
    
    // Wait for operations
    harness.wait_for_commits(3).await.unwrap(); // bootstrap + create + status
    
    // Note: VM operations are currently stubs, but we verify no errors occurred
    assert_eq!(harness.get_pending_proposals_count().await, 0);
}

#[tokio::test]
async fn test_apply_ordering_guarantees() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Submit operations that depend on order
    harness.register_worker("worker-1", 100).await.unwrap();
    let task_id = harness.submit_task("ordered-task").await.unwrap();
    
    // Update worker status
    harness.update_worker_status("worker-1", WorkerStatus::Busy).await.unwrap();
    
    // Complete task
    let proposal = ProposalGenerator::complete_task_proposal(&task_id, true);
    harness.raft_manager.propose(proposal).await.unwrap();
    
    // Wait for all operations
    harness.wait_for_commits(5).await.unwrap(); // bootstrap + 4 operations
    
    // Verify final state reflects proper ordering
    // TODO: When state retrieval is implemented, verify final state
}

#[tokio::test]
async fn test_state_machine_error_handling() {
    let harness = RaftTestHarness::new(1).await;
    harness.bootstrap_single_node().await.unwrap();
    
    // Try various invalid operations
    let invalid_ops = vec![
        // Complete non-existent task
        ProposalGenerator::complete_task_proposal("non-existent", true),
        // Update non-existent worker
        ProposalGenerator::update_worker_proposal("non-existent", WorkerStatus::Online),
    ];
    
    for op in invalid_ops {
        harness.raft_manager.propose(op).await.unwrap();
    }
    
    // Wait for all invalid operations to be processed
    harness.wait_for_commits(3).await.unwrap(); // bootstrap + 2 invalid ops
    
    // Verify state wasn't corrupted by trying a valid operation
    harness.submit_task("valid-task").await.unwrap();
    harness.wait_for_commits(4).await.unwrap();
    
    // If we got here without errors, the state machine is still functional
}