use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use std::sync::Arc;
use raft::prelude::*;

#[tokio::test]
async fn test_join_cluster_configuration_update() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("join_cluster_config_test=info,blixard=info,raft=info")
        .try_init();

    // Start leader node on fixed port
    let temp_dir1 = tempfile::tempdir().unwrap();
    let addr1: std::net::SocketAddr = "127.0.0.1:19701".parse().unwrap();
    let config1 = blixard::types::NodeConfig {
        id: 1,
        bind_addr: addr1,
        data_dir: temp_dir1.path().to_string_lossy().to_string(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node1 = blixard::node::Node::new(config1);
    node1.initialize().await.unwrap();
    node1.start().await.unwrap();
    
    info!("Leader node started at {}", addr1);
    
    // Start gRPC server for leader
    let shared1 = node1.shared();
    let server_handle1 = tokio::spawn(async move {
        blixard::grpc_server::start_grpc_server(shared1, addr1).await.unwrap();
    });
    
    // Wait for leader to be ready
    sleep(Duration::from_secs(2)).await;
    
    // Check leader's initial configuration
    {
        let storage = blixard::storage::RedbRaftStorage { 
            database: node1.shared().get_database().await.unwrap() 
        };
        let conf_state = storage.load_conf_state().unwrap();
        info!("Leader initial conf_state: {:?}", conf_state);
        assert_eq!(conf_state.voters, vec![1], "Leader should have itself as sole voter");
    }
    
    // Start joining node on fixed port
    let temp_dir2 = tempfile::tempdir().unwrap();
    let addr2: std::net::SocketAddr = "127.0.0.1:19702".parse().unwrap();
    let config2 = blixard::types::NodeConfig {
        id: 2,
        bind_addr: addr2,
        data_dir: temp_dir2.path().to_string_lossy().to_string(),
        join_addr: Some(addr1),
        use_tailscale: false,
    };
    
    let mut node2 = blixard::node::Node::new(config2);
    
    // Check joining node's initial configuration (before initialization)
    {
        // Create storage directly to check initial state
        let db_path = format!("{}/blixard.db", temp_dir2.path().to_string_lossy());
        let database = redb::Database::create(&db_path).unwrap();
        let storage = blixard::storage::RedbRaftStorage { 
            database: std::sync::Arc::new(database) 
        };
        storage.initialize_joining_node().unwrap();
        let conf_state = storage.load_conf_state().unwrap();
        info!("Joining node initial conf_state: {:?}", conf_state);
        assert!(conf_state.voters.is_empty(), "Joining node should start with no voters");
    }
    
    // Initialize and start the joining node
    node2.initialize().await.unwrap();
    node2.start().await.unwrap();
    
    info!("Joining node started at {}", addr2);
    
    // Start gRPC server for joining node
    let shared2 = node2.shared();
    let server_handle2 = tokio::spawn(async move {
        blixard::grpc_server::start_grpc_server(shared2, addr2).await.unwrap();
    });
    
    // Wait for join to complete and configuration to propagate
    sleep(Duration::from_secs(5)).await;
    
    // Check if joining node's configuration was updated
    {
        let storage = blixard::storage::RedbRaftStorage { 
            database: node2.shared().get_database().await.unwrap() 
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
        let storage = blixard::storage::RedbRaftStorage { 
            database: node1.shared().get_database().await.unwrap() 
        };
        let conf_state = storage.load_conf_state().unwrap();
        info!("Leader final conf_state: {:?}", conf_state);
        
        assert!(conf_state.voters.contains(&1), "Leader should still have itself in voters");
        assert!(conf_state.voters.contains(&2), "Leader should have node 2 in voters");
        assert_eq!(conf_state.voters.len(), 2, "Should have exactly 2 voters");
    }
    
    // Check cluster status from both nodes
    let (leader_id1, peers1, term1) = node1.get_cluster_status().await.unwrap();
    info!("Node 1 cluster status: leader={}, peers={:?}, term={}", leader_id1, peers1, term1);
    
    let (leader_id2, peers2, term2) = node2.get_cluster_status().await.unwrap();
    info!("Node 2 cluster status: leader={}, peers={:?}, term={}", leader_id2, peers2, term2);
    
    // Both nodes should agree on the leader
    assert_eq!(leader_id1, leader_id2, "Both nodes should agree on leader");
    assert_eq!(leader_id1, 1, "Node 1 should be the leader");
    
    // Clean up
    server_handle1.abort();
    server_handle2.abort();
    node1.stop().await.unwrap();
    node2.stop().await.unwrap();
}