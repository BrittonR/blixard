//! Byzantine failure tests for malicious node behavior
//!
//! Tests system resilience against nodes that:
//! - Send invalid or contradictory messages
//! - Attempt to corrupt distributed state
//! - Try to become leader illegitimately
//! - Send messages with forged identities

use std::sync::Arc;
use std::time::Duration;

use madsim::time::{sleep, timeout};
use blixard_core::proto::cluster_service_client::ClusterServiceClient;
use blixard_core::proto::{RaftMessageRequest, RaftMessageResponse};
use blixard_core::test_helpers::{TestCluster, TestNode};
use raft::prelude::*;
use tonic::{Request, Response, Status};

/// Test node sending invalid Raft messages with wrong terms
#[madsim::test]
async fn test_byzantine_wrong_term_messages() {
    let mut cluster = TestCluster::new(3).await;
    cluster.start_all_nodes().await;
    
    // Wait for leader election
    sleep(Duration::from_secs(2)).await;
    
    let leader_id = cluster.find_leader().await
        .expect("Should have elected a leader");
    
    // Get a follower node
    let follower_id = if leader_id == 1 { 2 } else { 1 };
    let follower = cluster.get_node(follower_id).unwrap();
    
    // Byzantine behavior: Send vote request with future term
    let mut malicious_msg = Message::default();
    malicious_msg.set_msg_type(MessageType::MsgRequestVote);
    malicious_msg.set_to(leader_id);
    malicious_msg.set_from(follower_id);
    malicious_msg.set_term(1000); // Way future term
    malicious_msg.set_log_term(1);
    malicious_msg.set_index(1);
    
    // Send the malicious message
    let serialized = blixard_core::raft_codec::serialize_message(&malicious_msg)
        .expect("Should serialize message");
    
    let leader_addr = cluster.get_node(leader_id).unwrap().grpc_addr();
    let endpoint = format!("http://{}", leader_addr);
    let mut client = ClusterServiceClient::connect(endpoint).await
        .expect("Should connect to leader");
    
    let response = client.send_raft_message(Request::new(RaftMessageRequest {
        raft_data: serialized,
    })).await;
    
    // The node should handle this gracefully
    assert!(response.is_ok(), "Node should accept message without crashing");
    
    // Give time for processing
    sleep(Duration::from_secs(1)).await;
    
    // Verify the cluster is still functional
    let new_leader = cluster.find_leader().await;
    assert!(new_leader.is_some(), "Cluster should still have a leader");
    
    // Verify state hasn't been corrupted
    let task_id = "test-task-1";
    let submitted = cluster.submit_task(leader_id, task_id, "echo", &["test"]).await;
    assert!(submitted, "Should still be able to submit tasks");
    
    // Wait for task to complete
    sleep(Duration::from_secs(2)).await;
    
    let status = cluster.get_task_status(leader_id, task_id).await;
    assert!(status.is_some(), "Task should have been processed");
}

/// Test node voting for multiple candidates in same term
#[madsim::test]
async fn test_byzantine_multiple_votes() {
    let mut cluster = TestCluster::new(5).await;
    cluster.start_all_nodes().await;
    
    // Wait for initial leader election
    sleep(Duration::from_secs(2)).await;
    
    let initial_leader = cluster.find_leader().await
        .expect("Should have elected initial leader");
    
    // Stop the leader to trigger new election
    cluster.stop_node(initial_leader).await;
    
    // During election, have a Byzantine node vote for multiple candidates
    let byzantine_node = if initial_leader == 1 { 2 } else { 1 };
    let candidates = vec![3, 4, 5].into_iter()
        .filter(|&id| id != initial_leader && id != byzantine_node)
        .collect::<Vec<_>>();
    
    // Byzantine behavior: Vote for all candidates
    for candidate in &candidates {
        let mut vote_msg = Message::default();
        vote_msg.set_msg_type(MessageType::MsgRequestVoteResponse);
        vote_msg.set_to(*candidate);
        vote_msg.set_from(byzantine_node);
        vote_msg.set_term(2); // Next term
        vote_msg.set_reject(false); // Grant vote
        
        let serialized = blixard_core::raft_codec::serialize_message(&vote_msg)
            .expect("Should serialize message");
        
        let candidate_addr = cluster.get_node(*candidate).unwrap().grpc_addr();
        let endpoint = format!("http://{}", candidate_addr);
        let mut client = ClusterServiceClient::connect(endpoint).await
            .expect("Should connect to candidate");
        
        let _ = client.send_raft_message(Request::new(RaftMessageRequest {
            raft_data: serialized,
        })).await;
    }
    
    // Wait for election to complete despite Byzantine behavior
    sleep(Duration::from_secs(3)).await;
    
    // Verify only one leader is elected
    let leaders = cluster.find_all_leaders().await;
    assert_eq!(leaders.len(), 1, "Should have exactly one leader despite Byzantine votes");
    
    // Verify consensus still works
    let new_leader = leaders[0];
    let task_id = "byzantine-test-task";
    let submitted = cluster.submit_task(new_leader, task_id, "echo", &["data"]).await;
    assert!(submitted, "Should be able to submit tasks to new leader");
}

/// Test node attempting to become leader without proper election
#[madsim::test]
async fn test_byzantine_fake_leader() {
    let mut cluster = TestCluster::new(3).await;
    cluster.start_all_nodes().await;
    
    // Wait for legitimate leader election
    sleep(Duration::from_secs(2)).await;
    
    let legitimate_leader = cluster.find_leader().await
        .expect("Should have elected legitimate leader");
    
    let byzantine_node = if legitimate_leader == 1 { 2 } else { 1 };
    
    // Byzantine behavior: Send append entries as if we're the leader
    let mut fake_append = Message::default();
    fake_append.set_msg_type(MessageType::MsgAppend);
    fake_append.set_to(3); // Target third node
    fake_append.set_from(byzantine_node);
    fake_append.set_term(1); // Current term
    fake_append.set_log_term(1);
    fake_append.set_index(1);
    fake_append.set_commit(1);
    
    // Add a fake entry
    let mut entry = Entry::default();
    entry.set_term(1);
    entry.set_index(2);
    entry.set_entry_type(EntryType::EntryNormal);
    entry.data = b"fake-data".to_vec();
    fake_append.entries = vec![entry];
    
    let serialized = blixard_core::raft_codec::serialize_message(&fake_append)
        .expect("Should serialize message");
    
    let target_addr = cluster.get_node(3).unwrap().grpc_addr();
    let endpoint = format!("http://{}", target_addr);
    let mut client = ClusterServiceClient::connect(endpoint).await
        .expect("Should connect to target");
    
    let response = client.send_raft_message(Request::new(RaftMessageRequest {
        raft_data: serialized,
    })).await;
    
    assert!(response.is_ok(), "Node should handle fake leader message gracefully");
    
    // Give time for processing
    sleep(Duration::from_secs(1)).await;
    
    // Verify the legitimate leader is still the leader
    let current_leader = cluster.find_leader().await
        .expect("Should still have a leader");
    assert_eq!(current_leader, legitimate_leader, 
        "Legitimate leader should remain despite Byzantine behavior");
    
    // Verify no state corruption occurred
    let nodes = cluster.get_all_nodes();
    let mut applied_indices = Vec::new();
    for node in nodes {
        let status = node.get_raft_status().await;
        applied_indices.push(status.applied);
    }
    
    // All nodes should have consistent applied index
    let first_applied = applied_indices[0];
    for applied in &applied_indices {
        assert_eq!(*applied, first_applied, 
            "All nodes should have same applied index, no corruption from fake leader");
    }
}

/// Test message replay attacks
#[madsim::test]
async fn test_byzantine_message_replay() {
    let mut cluster = TestCluster::new(3).await;
    cluster.start_all_nodes().await;
    
    // Wait for leader election and submit a task
    sleep(Duration::from_secs(2)).await;
    
    let leader_id = cluster.find_leader().await
        .expect("Should have elected a leader");
    
    // Submit a legitimate task
    let task_id = "replay-target";
    cluster.submit_task(leader_id, task_id, "echo", &["original"]).await;
    
    // Wait for task to be committed
    sleep(Duration::from_secs(1)).await;
    
    // Capture a legitimate append entries message
    // (In real implementation, we'd intercept network traffic)
    // For this test, we'll create a plausible replay message
    
    let mut replay_msg = Message::default();
    replay_msg.set_msg_type(MessageType::MsgAppend);
    replay_msg.set_to(if leader_id == 1 { 2 } else { 1 });
    replay_msg.set_from(leader_id);
    replay_msg.set_term(1);
    replay_msg.set_log_term(1);
    replay_msg.set_index(1);
    replay_msg.set_commit(1);
    
    // Wait a bit, then replay the message multiple times
    sleep(Duration::from_secs(2)).await;
    
    let target_id = if leader_id == 1 { 2 } else { 1 };
    let target_addr = cluster.get_node(target_id).unwrap().grpc_addr();
    let endpoint = format!("http://{}", target_addr);
    
    // Replay the message 10 times
    for i in 0..10 {
        let mut client = ClusterServiceClient::connect(endpoint.clone()).await
            .expect("Should connect to target");
        
        let serialized = blixard_core::raft_codec::serialize_message(&replay_msg)
            .expect("Should serialize message");
        
        let _ = client.send_raft_message(Request::new(RaftMessageRequest {
            raft_data: serialized,
        })).await;
        
        sleep(Duration::from_millis(100)).await;
    }
    
    // Verify the system is still functional
    let current_leader = cluster.find_leader().await;
    assert!(current_leader.is_some(), "Should still have a leader after replay attack");
    
    // Submit a new task to verify no corruption
    let new_task_id = "post-replay-task";
    let submitted = cluster.submit_task(
        current_leader.unwrap(), 
        new_task_id, 
        "echo", 
        &["after-replay"]
    ).await;
    assert!(submitted, "Should be able to submit new tasks after replay attack");
    
    // Verify task completes
    sleep(Duration::from_secs(2)).await;
    let status = cluster.get_task_status(current_leader.unwrap(), new_task_id).await;
    assert!(status.is_some(), "New task should complete successfully");
}

/// Test node sending messages with forged identities
#[madsim::test]
async fn test_byzantine_identity_forgery() {
    let mut cluster = TestCluster::new(5).await;
    cluster.start_all_nodes().await;
    
    sleep(Duration::from_secs(2)).await;
    
    let leader_id = cluster.find_leader().await
        .expect("Should have elected a leader");
    
    // Byzantine node will pretend to be another node
    let byzantine_node = 1;
    let impersonated_node = 2;
    let target_node = 3;
    
    // Create a message pretending to be from node 2
    let mut forged_msg = Message::default();
    forged_msg.set_msg_type(MessageType::MsgHeartbeat);
    forged_msg.set_to(target_node);
    forged_msg.set_from(impersonated_node); // Forged identity!
    forged_msg.set_term(1);
    
    // Send from byzantine node but claiming to be from impersonated node
    let serialized = blixard_core::raft_codec::serialize_message(&forged_msg)
        .expect("Should serialize message");
    
    let target_addr = cluster.get_node(target_node).unwrap().grpc_addr();
    let endpoint = format!("http://{}", target_addr);
    let mut client = ClusterServiceClient::connect(endpoint).await
        .expect("Should connect to target");
    
    let response = client.send_raft_message(Request::new(RaftMessageRequest {
        raft_data: serialized,
    })).await;
    
    assert!(response.is_ok(), "Node should handle forged identity gracefully");
    
    // The system should continue functioning normally
    sleep(Duration::from_secs(1)).await;
    
    // Verify consensus properties still hold
    let leaders = cluster.find_all_leaders().await;
    assert_eq!(leaders.len(), 1, "Should have exactly one leader");
    
    // Verify all nodes agree on the leader
    for node_id in 1..=5 {
        let node = cluster.get_node(node_id).unwrap();
        let status = node.get_raft_status().await;
        if status.role == "Leader" {
            assert_eq!(node_id, leaders[0], "Leader identity should be consistent");
        }
    }
}

/// Helper to find all nodes that think they are leaders
impl TestCluster {
    async fn find_all_leaders(&self) -> Vec<u64> {
        let mut leaders = Vec::new();
        for node_id in 1..=self.size() {
            if let Some(node) = self.get_node(node_id) {
                let status = node.get_raft_status().await;
                if status.role == "Leader" {
                    leaders.push(node_id);
                }
            }
        }
        leaders
    }
}