use blixard_core::node_shared::{SharedNodeState, RaftStatus};
use blixard_core::types::{NodeConfig, NodeTopology};
use blixard_core::raft_manager::{TaskSpec, ResourceRequirements};
use proptest::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use std::net::SocketAddr;
use std::collections::HashSet;

// Arbitrary implementations for test types
fn arb_socket_addr() -> impl Strategy<Value = SocketAddr> {
    (any::<[u8; 4]>(), 1000u16..10000).prop_map(|(ip, port)| {
        SocketAddr::from((ip, port))
    })
}

fn arb_node_config() -> impl Strategy<Value = NodeConfig> {
    (1u64..1000, arb_socket_addr(), "\\w+", any::<Option<String>>(), any::<bool>())
        .prop_map(|(id, bind_addr, data_dir, join_addr, use_tailscale)| {
            NodeConfig {
                id,
                bind_addr,
                data_dir,
                join_addr,
                use_tailscale,
                vm_backend: "mock".to_string(),
                transport_config: None,
                topology: NodeTopology::default(),
            }
        })
}

fn arb_raft_status() -> impl Strategy<Value = RaftStatus> {
    (
        any::<bool>(),
        1u64..100,
        any::<Option<u64>>(),
        0u64..1000,
        prop::string::string_regex("[a-z]+").unwrap(),
    ).prop_map(|(is_leader, node_id, leader_id, term, state)| {
        RaftStatus {
            is_leader,
            node_id,
            leader_id,
            term,
            state,
        }
    })
}

#[derive(Debug, Clone)]
enum ConcurrentOperation {
    SetRunning(bool),
    SetInitialized(bool),
    UpdateRaftStatus(RaftStatus),
    AddPeer(u64, String),
    RemovePeer(u64),
    UpdatePeerConnection(u64, bool),
    GetPeers,
    GetRaftStatus,
    IsRunning,
    IsInitialized,
    IsLeader,
    GetClusterStatus,
}

fn arb_concurrent_operation() -> impl Strategy<Value = ConcurrentOperation> {
    prop_oneof![
        any::<bool>().prop_map(ConcurrentOperation::SetRunning),
        any::<bool>().prop_map(ConcurrentOperation::SetInitialized),
        arb_raft_status().prop_map(ConcurrentOperation::UpdateRaftStatus),
        (1u64..100, "\\w+").prop_map(|(id, addr)| ConcurrentOperation::AddPeer(id, addr)),
        (1u64..100).prop_map(ConcurrentOperation::RemovePeer),
        (1u64..100, any::<bool>()).prop_map(|(id, connected)| ConcurrentOperation::UpdatePeerConnection(id, connected)),
        Just(ConcurrentOperation::GetPeers),
        Just(ConcurrentOperation::GetRaftStatus),
        Just(ConcurrentOperation::IsRunning),
        Just(ConcurrentOperation::IsInitialized),
        Just(ConcurrentOperation::IsLeader),
        Just(ConcurrentOperation::GetClusterStatus),
    ]
}

// Test concurrent state updates don't corrupt state
proptest! {
    #[test]
    fn prop_concurrent_state_updates(
        config in arb_node_config(),
        operations in prop::collection::vec(arb_concurrent_operation(), 10..50),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = Arc::new(SharedNodeState::new(config.clone()));
            
            // Execute operations concurrently
            let mut handles = Vec::new();
            for op in operations {
                let state_clone = Arc::clone(&state);
                let handle = tokio::spawn(async move {
                    match op {
                        ConcurrentOperation::SetRunning(running) => {
                            state_clone.set_running(running).await;
                        }
                        ConcurrentOperation::SetInitialized(initialized) => {
                            state_clone.set_initialized(initialized).await;
                        }
                        ConcurrentOperation::UpdateRaftStatus(status) => {
                            state_clone.update_raft_status(status).await;
                        }
                        ConcurrentOperation::AddPeer(id, addr) => {
                            let _ = state_clone.add_peer(id, addr).await;
                        }
                        ConcurrentOperation::RemovePeer(id) => {
                            let _ = state_clone.remove_peer(id).await;
                        }
                        ConcurrentOperation::UpdatePeerConnection(id, connected) => {
                            let _ = state_clone.update_peer_connection(id, connected).await;
                        }
                        ConcurrentOperation::GetPeers => {
                            let _ = state_clone.get_peers().await;
                        }
                        ConcurrentOperation::GetRaftStatus => {
                            let _ = state_clone.get_raft_status().await;
                        }
                        ConcurrentOperation::IsRunning => {
                            let _ = state_clone.is_running().await;
                        }
                        ConcurrentOperation::IsInitialized => {
                            let _ = state_clone.is_initialized().await;
                        }
                        ConcurrentOperation::IsLeader => {
                            let _ = state_clone.is_leader().await;
                        }
                        ConcurrentOperation::GetClusterStatus => {
                            // Need to be initialized for this to work
                            state_clone.set_initialized(true).await;
                            let _ = state_clone.get_cluster_status().await;
                        }
                    }
                });
                handles.push(handle);
            }
            
            // Wait for all operations to complete
            for handle in handles {
                handle.await.unwrap();
            }
            
            // Verify state is consistent
            assert_eq!(state.get_id(), config.id);
            assert_eq!(state.get_bind_addr(), &config.bind_addr);
            
            // Peers should not have duplicates
            let peers = state.get_peers().await;
            let peer_ids: HashSet<u64> = peers.iter().map(|p| p.id).collect();
            assert_eq!(peer_ids.len(), peers.len(), "Peer IDs should be unique");
        });
    }
}

// Test channel sender management
proptest! {
    #[test]
    fn prop_channel_sender_lifecycle(
        config in arb_node_config(),
        set_operations in prop::collection::vec(any::<bool>(), 5..20),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = Arc::new(SharedNodeState::new(config));
            
            // Test shutdown_tx lifecycle
            for should_set in &set_operations {
                if *should_set {
                    let (tx, _rx) = oneshot::channel();
                    state.set_shutdown_tx(tx).await;
                } else {
                    let _ = state.take_shutdown_tx().await;
                }
            }
            
            // Test raft proposal tx
            for should_set in &set_operations {
                if *should_set {
                    let (tx, _rx) = mpsc::unbounded_channel();
                    state.set_raft_proposal_tx(tx).await;
                }
            }
            
            // Test raft message tx
            for should_set in &set_operations {
                if *should_set {
                    let (tx, _rx) = mpsc::unbounded_channel();
                    state.set_raft_message_tx(tx).await;
                }
            }
            
            // Shutdown should clear all senders
            state.shutdown_components().await;
            
            // Verify shutdown clears everything
            assert!(state.take_shutdown_tx().await.is_none());
            assert!(!state.is_running().await);
        });
    }
}

// Test peer management invariants
proptest! {
    #[test]
    fn prop_peer_management_invariants(
        config in arb_node_config(),
        peer_ops in prop::collection::vec(
            prop_oneof![
                (1u64..100, "[a-z]+:[0-9]+").prop_map(|(id, addr)| (id, addr, true)),
                (1u64..100, "[a-z]+:[0-9]+").prop_map(|(id, addr)| (id, addr, false)),
            ],
            0..20
        ),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = SharedNodeState::new(config);
            let mut expected_peers = HashSet::new();
            
            for (id, addr, should_add) in peer_ops {
                if should_add {
                    let result = state.add_peer(id, addr.clone()).await;
                    if !expected_peers.contains(&id) {
                        assert!(result.is_ok());
                        expected_peers.insert(id);
                    } else {
                        assert!(result.is_err()); // Should fail if peer already exists
                    }
                } else {
                    let result = state.remove_peer(id).await;
                    if expected_peers.contains(&id) {
                        assert!(result.is_ok());
                        expected_peers.remove(&id);
                    } else {
                        assert!(result.is_err()); // Should fail if peer doesn't exist
                    }
                }
            }
            
            // Verify final state matches expected
            let actual_peers = state.get_peers().await;
            let actual_ids: HashSet<u64> = actual_peers.iter().map(|p| p.id).collect();
            assert_eq!(actual_ids, expected_peers);
        });
    }
}

// Test concurrent peer updates
proptest! {
    #[test]
    fn prop_concurrent_peer_updates(
        config in arb_node_config(),
        peer_id in 1u64..100,
        concurrent_updates in prop::collection::vec(any::<bool>(), 10..50),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = Arc::new(SharedNodeState::new(config));
            
            // Add a peer first
            state.add_peer(peer_id, "test:1234".to_string()).await.unwrap();
            
            // Concurrent connection status updates
            let mut handles = Vec::new();
            for is_connected in concurrent_updates {
                let state_clone = Arc::clone(&state);
                let handle = tokio::spawn(async move {
                    let _ = state_clone.update_peer_connection(peer_id, is_connected).await;
                });
                handles.push(handle);
            }
            
            // Wait for all updates
            for handle in handles {
                handle.await.unwrap();
            }
            
            // Verify peer still exists and has valid state
            let peer = state.get_peer(peer_id).await;
            assert!(peer.is_some());
            let peer = peer.unwrap();
            assert_eq!(peer.id, peer_id);
            assert_eq!(peer.address, "test:1234");
        });
    }
}

// Test Raft status consistency
proptest! {
    #[test]
    fn prop_raft_status_consistency(
        config in arb_node_config(),
        status_updates in prop::collection::vec(arb_raft_status(), 5..20),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = Arc::new(SharedNodeState::new(config.clone()));
            
            // Apply updates concurrently
            let mut handles = Vec::new();
            for status in status_updates.clone() {
                let state_clone = Arc::clone(&state);
                let handle = tokio::spawn(async move {
                    state_clone.update_raft_status(status).await;
                });
                handles.push(handle);
            }
            
            // Wait for all updates
            for handle in handles {
                handle.await.unwrap();
            }
            
            // Final status should be one of the applied statuses
            let final_status = state.get_raft_status().await.unwrap();
            let valid_statuses: Vec<u64> = status_updates.iter().map(|s| s.node_id).collect();
            assert!(valid_statuses.contains(&final_status.node_id));
            
            // is_leader should match the status
            assert_eq!(state.is_leader().await, final_status.is_leader);
        });
    }
}

// Test database lifecycle
proptest! {
    #[test]
    fn prop_database_lifecycle(
        config in arb_node_config(),
        operations in prop::collection::vec(any::<bool>(), 1..5), // Reduced operations
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = SharedNodeState::new(config);
            
            // Use a single temp dir for the test
            let temp_dir = tempfile::tempdir().unwrap();
            
            for (i, should_set) in operations.iter().enumerate() {
                if *should_set {
                    // Create a database with unique name
                    let db_path = temp_dir.path().join(format!("test{}.db", i));
                    let db = Arc::new(redb::Database::create(db_path).unwrap());
                    state.set_database(db.clone()).await;
                    
                    // Verify it's set
                    let retrieved = state.get_database().await;
                    assert!(retrieved.is_some());
                    
                    // Important: drop the retrieved Arc to avoid holding references
                    drop(retrieved);
                } else {
                    state.clear_database().await;
                    
                    // Verify it's cleared
                    let retrieved = state.get_database().await;
                    assert!(retrieved.is_none());
                }
            }
            
            // Shutdown should clear database
            state.shutdown_components().await;
            assert!(state.get_database().await.is_none());
        });
    }
}

// Test cluster status with various states
proptest! {
    #[test]
    fn prop_cluster_status_consistency(
        config in arb_node_config(),
        peer_ids in prop::collection::vec(1u64..100, 0..10),
        raft_status in arb_raft_status(),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = SharedNodeState::new(config.clone());
            
            // Must be initialized to get cluster status
            state.set_initialized(true).await;
            
            // Create a temporary database
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let db = Arc::new(redb::Database::create(db_path).unwrap());
            
            // Initialize the database tables
            blixard_core::storage::init_database_tables(&db).unwrap();
            
            // Initialize storage for single node
            let storage = blixard_core::storage::RedbRaftStorage { database: db.clone() };
            storage.initialize_single_node(config.id).unwrap();
            
            // Set the database
            state.set_database(db).await;
            
            // Add peers
            for (i, peer_id) in peer_ids.iter().enumerate() {
                let _ = state.add_peer(*peer_id, format!("peer{}:1234", i)).await;
            }
            
            // Update raft status
            state.update_raft_status(raft_status.clone()).await;
            
            // Get cluster status
            let result = state.get_cluster_status().await;
            assert!(result.is_ok());
            
            let (leader_id, node_ids, term) = result.unwrap();
            
            // Verify consistency
            assert_eq!(leader_id, raft_status.leader_id.unwrap_or(0));
            assert_eq!(term, raft_status.term);
            
            // Node IDs should include self (single-node cluster)
            // Note: peers are not automatically voters in Raft
            assert!(node_ids.contains(&config.id));
            
            // Should be sorted
            let mut sorted = node_ids.clone();
            sorted.sort();
            assert_eq!(node_ids, sorted);
        });
    }
}

// Test error handling for uninitialized operations
proptest! {
    #[test]
    fn prop_uninitialized_operations_fail(
        config in arb_node_config(),
        task_id in "[a-zA-Z0-9]+",
        cpu_cores in 1u32..8,
        memory_mb in 128u64..4096,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = SharedNodeState::new(config);
            
            // Don't initialize state
            state.set_initialized(false).await;
            
            // Task submission should fail
            let task = TaskSpec {
                command: "test".to_string(),
                args: vec![],
                resources: ResourceRequirements {
                    cpu_cores,
                    memory_mb,
                    disk_gb: 10,
                    required_features: vec![],
                },
                timeout_secs: 60,
            };
            let result = state.submit_task(&task_id, task).await;
            assert!(result.is_err());
            
            // Send operations should fail without channels
            let (from, msg) = (1, raft::prelude::Message::default());
            assert!(state.send_raft_message(from, msg).await.is_err());
            
            // Cluster status should fail when not initialized
            let result = state.get_cluster_status().await;
            assert!(result.is_err());
        });
    }
}

// Test concurrent task operations
proptest! {
    #[test]
    fn prop_concurrent_task_operations(
        config in arb_node_config(),
        task_ids in prop::collection::vec("[a-zA-Z0-9]+", 1..10),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = Arc::new(SharedNodeState::new(config));
            
            // Set up database
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("test.db");
            let db = Arc::new(redb::Database::create(db_path).unwrap());
            state.set_database(db).await;
            
            // Concurrent task status queries (should handle missing tasks gracefully)
            let mut handles = Vec::new();
            for task_id in task_ids {
                let state_clone = Arc::clone(&state);
                let task_id_clone = task_id.clone();
                let handle = tokio::spawn(async move {
                    let result = state_clone.get_task_status(&task_id_clone).await;
                    // Should either be Ok(None) or error, but not panic
                    match result {
                        Ok(None) => true,
                        Ok(Some(_)) => true,
                        Err(_) => true,
                    }
                });
                handles.push(handle);
            }
            
            // All operations should complete without panic
            for handle in handles {
                assert!(handle.await.unwrap());
            }
        });
    }
}