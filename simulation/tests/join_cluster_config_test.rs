#![cfg(feature = "test-helpers")]

use std::time::Duration;
use tracing::info;
use blixard_core::test_helpers::{TestNode, PortAllocator};
use blixard_core::proto::{cluster_service_client::ClusterServiceClient, HealthCheckRequest};

// Include common test utilities
mod common;
use common::test_timing::wait_for_condition;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_join_cluster_configuration_update() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("join_cluster_config_test=info,blixard=info,raft=info")
        .try_init();

    // Start leader node with dynamic port
    let leader_port = PortAllocator::next_port();
    let leader_node = TestNode::builder()
        .with_id(1)
        .with_port(leader_port)
        .build()
        .await
        .expect("Failed to start leader node");
    
    let addr1 = leader_node.addr;
    info!("Leader node started at {}", addr1);
    
    // Wait for leader to be ready
    wait_for_condition(
        || async {
            // Check if leader node is responsive via gRPC
            match ClusterServiceClient::connect(format!("http://{}", addr1)).await {
                Ok(mut client) => {
                    client.health_check(HealthCheckRequest {}).await.is_ok()
                }
                Err(_) => false,
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    ).await.expect("Leader node failed to become ready");
    
    // Check leader's initial configuration
    {
        let storage = blixard_core::storage::RedbRaftStorage { 
            database: leader_node.shared_state.get_database().await.unwrap() 
        };
        let conf_state = storage.load_conf_state().unwrap();
        info!("Leader initial conf_state: {:?}", conf_state);
        assert_eq!(conf_state.voters, vec![1], "Leader should have itself as sole voter");
    }
    
    // Start joining node with dynamic port
    let joining_port = PortAllocator::next_port();
    let joining_node = TestNode::builder()
        .with_id(2)
        .with_port(joining_port)
        .with_join_addr(Some(addr1))
        .build()
        .await
        .expect("Failed to start joining node");
    
    let addr2 = joining_node.addr;
    info!("Joining node started at {}", addr2);
    
    // Wait for join to complete and configuration to propagate
    let node2_shared = joining_node.shared_state.clone();
    wait_for_condition(
        || async {
            // Check if node2's configuration has been updated
            if let Some(db) = node2_shared.get_database().await {
                let storage = blixard_core::storage::RedbRaftStorage { database: db };
                if let Ok(conf_state) = storage.load_conf_state() {
                    // Join is complete when node2 sees both nodes in the configuration
                    conf_state.voters.len() == 2 && 
                    conf_state.voters.contains(&1) && 
                    conf_state.voters.contains(&2)
                } else {
                    false
                }
            } else {
                false
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(200),
    ).await.expect("Node failed to join cluster");
    
    // Check if joining node's configuration was updated
    {
        let storage = blixard_core::storage::RedbRaftStorage { 
            database: joining_node.shared_state.get_database().await.unwrap() 
        };
        let conf_state = storage.load_conf_state().unwrap();
        info!("Joining node final conf_state: voters={:?}, learners={:?}", 
            conf_state.voters, conf_state.learners);
        
        // The joining node should now have both nodes in its configuration
        assert!(conf_state.voters.contains(&1), "Joining node should have node 1 in voters");
        assert!(conf_state.voters.contains(&2), "Joining node should have itself in voters");
        assert_eq!(conf_state.voters.len(), 2, "Should have exactly 2 voters");
    }
    
    // Also check leader's configuration was updated
    {
        let storage = blixard_core::storage::RedbRaftStorage { 
            database: leader_node.shared_state.get_database().await.unwrap() 
        };
        let conf_state = storage.load_conf_state().unwrap();
        info!("Leader final conf_state: {:?}", conf_state);
        
        assert!(conf_state.voters.contains(&1), "Leader should still have itself in voters");
        assert!(conf_state.voters.contains(&2), "Leader should have node 2 in voters");
        assert_eq!(conf_state.voters.len(), 2, "Should have exactly 2 voters");
    }
    
    // Wait for both nodes to agree on the leader
    wait_for_condition(
        || async {
            if let (Ok((leader1, _, _)), Ok((leader2, _, _))) = 
                (leader_node.node.get_cluster_status().await, joining_node.node.get_cluster_status().await) {
                // Both nodes should see node 1 as leader
                leader1 == 1 && leader2 == 1
            } else {
                false
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    ).await.expect("Nodes failed to agree on leader");
    
    // Check cluster status from both nodes
    let (leader_id1, peers1, term1) = leader_node.node.get_cluster_status().await.unwrap();
    info!("Node 1 cluster status: leader={}, peers={:?}, term={}", leader_id1, peers1, term1);
    
    let (leader_id2, peers2, term2) = joining_node.node.get_cluster_status().await.unwrap();
    info!("Node 2 cluster status: leader={}, peers={:?}, term={}", leader_id2, peers2, term2);
    
    // Both nodes should agree on the leader
    assert_eq!(leader_id1, leader_id2, "Both nodes should agree on leader");
    assert_eq!(leader_id1, 1, "Node 1 should be the leader");
    
    // Clean up
    leader_node.shutdown().await;
    joining_node.shutdown().await;
}