#[cfg(test)]
mod worker_registration_tests {
    use blixard::node::Node;
    use blixard::types::NodeConfig;
    use std::net::SocketAddr;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    async fn setup_test_node(node_id: u64, bind_port: u16, join_addr: Option<SocketAddr>) -> (Node, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeConfig {
            id: node_id,
            data_dir: temp_dir.path().to_str().unwrap().to_string(),
            bind_addr: format!("127.0.0.1:{}", bind_port).parse().unwrap(),
            join_addr: join_addr.map(|addr| addr.to_string()),
            use_tailscale: false,
        };
        
        let node = Node::new(config);
        (node, temp_dir)
    }

    #[tokio::test]
    async fn test_worker_registration_consistency() {
        // Start node 1 as bootstrap node
        let (mut node1, _temp1) = setup_test_node(1, 9001, None).await;
        node1.start().await.unwrap();
        
        // Wait for bootstrap to complete
        sleep(Duration::from_millis(500)).await;
        
        // Start node 2 joining the cluster
        let join_addr = "127.0.0.1:9001".parse().unwrap();
        let (mut node2, _temp2) = setup_test_node(2, 9002, Some(join_addr)).await;
        node2.start().await.unwrap();
        
        // Wait for node 2 to join and register as worker
        sleep(Duration::from_secs(2)).await;
        
        // Start node 3 joining the cluster
        let (mut node3, _temp3) = setup_test_node(3, 9003, Some(join_addr)).await;
        node3.start().await.unwrap();
        
        // Wait for node 3 to join and register as worker
        sleep(Duration::from_secs(2)).await;
        
        // Check cluster membership
        let (leader1, nodes1, _) = node1.get_cluster_status().await.unwrap();
        let (leader2, nodes2, _) = node2.get_cluster_status().await.unwrap();
        let (leader3, nodes3, _) = node3.get_cluster_status().await.unwrap();
        
        // All nodes should see the same leader
        assert_eq!(leader1, leader2, "Nodes 1 and 2 should agree on leader");
        assert_eq!(leader2, leader3, "Nodes 2 and 3 should agree on leader");
        
        // All nodes should see 3 members
        assert_eq!(nodes1.len(), 3, "Node 1 should see 3 nodes");
        assert_eq!(nodes2.len(), 3, "Node 2 should see 3 nodes");
        assert_eq!(nodes3.len(), 3, "Node 3 should see 3 nodes");
        
        // Verify node IDs
        let mut sorted_nodes1 = nodes1.clone();
        let mut sorted_nodes2 = nodes2.clone();
        let mut sorted_nodes3 = nodes3.clone();
        
        sorted_nodes1.sort();
        sorted_nodes2.sort();
        sorted_nodes3.sort();
        
        assert_eq!(sorted_nodes1, vec![1, 2, 3], "Node 1 should see nodes 1, 2, 3");
        assert_eq!(sorted_nodes2, vec![1, 2, 3], "Node 2 should see nodes 1, 2, 3");
        assert_eq!(sorted_nodes3, vec![1, 2, 3], "Node 3 should see nodes 1, 2, 3");
        
        // Clean up
        node3.stop().await.unwrap();
        node2.stop().await.unwrap();
        node1.stop().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_worker_registration_after_restart() {
        // Start bootstrap node
        let (mut node1, temp1) = setup_test_node(1, 9011, None).await;
        node1.start().await.unwrap();
        sleep(Duration::from_millis(500)).await;
        
        // Get initial status
        let (_, initial_nodes, _) = node1.get_cluster_status().await.unwrap();
        assert_eq!(initial_nodes.len(), 1, "Should have 1 node initially");
        
        // Stop the node
        node1.stop().await.unwrap();
        sleep(Duration::from_millis(500)).await;
        
        // Create new node with same config
        let config = NodeConfig {
            id: 1,
            data_dir: temp1.path().to_str().unwrap().to_string(),
            bind_addr: "127.0.0.1:9011".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node1_restarted = Node::new(config);
        node1_restarted.start().await.unwrap();
        sleep(Duration::from_millis(500)).await;
        
        // Check node is still registered
        let (_, nodes_after_restart, _) = node1_restarted.get_cluster_status().await.unwrap();
        assert_eq!(nodes_after_restart.len(), 1, "Should still have 1 node after restart");
        assert_eq!(nodes_after_restart[0], 1, "Node should be node 1");
        
        // Clean up
        node1_restarted.stop().await.unwrap();
    }
}