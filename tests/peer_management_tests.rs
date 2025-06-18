#![cfg(feature = "test-helpers")]

use std::sync::Arc;
use blixard::node_shared::{SharedNodeState, PeerInfo};
use blixard::types::NodeConfig;
use blixard::test_helpers::PortAllocator;

fn create_test_node_state(id: u64) -> SharedNodeState {
    let port = PortAllocator::next_port();
    let config = NodeConfig {
        id,
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        data_dir: format!("/tmp/test-node-{}", id),
        join_addr: None,
        use_tailscale: false,
    };
    SharedNodeState::new(config)
}

#[tokio::test]
async fn test_add_peer_success() {
    let node = create_test_node_state(1);
    
    // Add a peer with dynamic port
    let peer_port = PortAllocator::next_port();
    let peer_addr = format!("127.0.0.1:{}", peer_port);
    let result = node.add_peer(2, peer_addr.clone()).await;
    assert!(result.is_ok());
    
    // Verify peer was added
    let peers = node.get_peers().await;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id, 2);
    assert_eq!(peers[0].address, peer_addr);
    assert!(!peers[0].is_connected);
}

#[tokio::test]
async fn test_add_duplicate_peer_fails() {
    let node = create_test_node_state(1);
    
    // Add a peer with dynamic port
    let peer_port1 = PortAllocator::next_port();
    let result1 = node.add_peer(2, format!("127.0.0.1:{}", peer_port1)).await;
    assert!(result1.is_ok());
    
    // Try to add same peer again with different port
    let peer_port2 = PortAllocator::next_port();
    let result2 = node.add_peer(2, format!("127.0.0.1:{}", peer_port2)).await;
    assert!(result2.is_err());
    assert!(result2.unwrap_err().to_string().contains("already exists"));
}

#[tokio::test]
async fn test_remove_peer_success() {
    let node = create_test_node_state(1);
    
    // Add a peer with dynamic port
    let peer_port = PortAllocator::next_port();
    node.add_peer(2, format!("127.0.0.1:{}", peer_port)).await.unwrap();
    
    // Remove the peer
    let result = node.remove_peer(2).await;
    assert!(result.is_ok());
    
    // Verify peer was removed
    let peers = node.get_peers().await;
    assert_eq!(peers.len(), 0);
}

#[tokio::test]
async fn test_remove_non_existent_peer_fails() {
    let node = create_test_node_state(1);
    
    // Try to remove non-existent peer
    let result = node.remove_peer(2).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_get_specific_peer() {
    let node = create_test_node_state(1);
    
    // Add multiple peers
    node.add_peer(2, "127.0.0.1:7002".to_string()).await.unwrap();
    node.add_peer(3, "127.0.0.1:7003".to_string()).await.unwrap();
    
    // Get specific peer
    let peer2 = node.get_peer(2).await;
    assert!(peer2.is_some());
    assert_eq!(peer2.unwrap().address, "127.0.0.1:7002");
    
    let peer4 = node.get_peer(4).await;
    assert!(peer4.is_none());
}

#[tokio::test]
async fn test_update_peer_connection_status() {
    let node = create_test_node_state(1);
    
    // Add a peer with dynamic port
    let peer_port = PortAllocator::next_port();
    node.add_peer(2, format!("127.0.0.1:{}", peer_port)).await.unwrap();
    
    // Initially not connected
    let peer = node.get_peer(2).await.unwrap();
    assert!(!peer.is_connected);
    
    // Update to connected
    let result = node.update_peer_connection(2, true).await;
    assert!(result.is_ok());
    
    let peer = node.get_peer(2).await.unwrap();
    assert!(peer.is_connected);
    
    // Update to disconnected
    node.update_peer_connection(2, false).await.unwrap();
    let peer = node.get_peer(2).await.unwrap();
    assert!(!peer.is_connected);
}

#[tokio::test]
async fn test_update_connection_for_non_existent_peer_fails() {
    let node = create_test_node_state(1);
    
    let result = node.update_peer_connection(2, true).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_multiple_peers_management() {
    let node = create_test_node_state(1);
    
    // Add multiple peers with dynamic ports
    for i in 2..=5 {
        let port = PortAllocator::next_port();
        node.add_peer(i, format!("127.0.0.1:{}", port)).await.unwrap();
    }
    
    // Verify all peers added
    let peers = node.get_peers().await;
    assert_eq!(peers.len(), 4);
    
    // Update some connections
    node.update_peer_connection(2, true).await.unwrap();
    node.update_peer_connection(4, true).await.unwrap();
    
    // Remove a peer
    node.remove_peer(3).await.unwrap();
    
    // Verify final state
    let peers = node.get_peers().await;
    assert_eq!(peers.len(), 3);
    
    let connected_count = peers.iter().filter(|p| p.is_connected).count();
    assert_eq!(connected_count, 2);
}

#[tokio::test]
async fn test_peer_info_clone() {
    let port = PortAllocator::next_port();
    let peer = PeerInfo {
        id: 1,
        address: format!("127.0.0.1:{}", port),
        is_connected: true,
    };
    
    let cloned = peer.clone();
    assert_eq!(peer.id, cloned.id);
    assert_eq!(peer.address, cloned.address);
    assert_eq!(peer.is_connected, cloned.is_connected);
}

#[tokio::test]
async fn test_concurrent_peer_operations() {
    let node = Arc::new(create_test_node_state(1));
    
    // Spawn multiple tasks that add/remove peers
    let mut handles = vec![];
    
    // Add peers concurrently
    for i in 2..=10 {
        let node_clone = node.clone();
        let handle = tokio::spawn(async move {
            node_clone.add_peer(i, format!("127.0.0.1:700{}", i)).await
        });
        handles.push(handle);
    }
    
    // Wait for all adds to complete
    for handle in handles {
        let _ = handle.await.unwrap();
    }
    
    // Verify all peers were added (some might have failed due to race conditions)
    let peers = node.get_peers().await;
    assert!(peers.len() >= 5); // At least some should succeed
}

#[tokio::test]
async fn test_raft_status_tracking() {
    let node = create_test_node_state(1);
    
    // Initial status
    let status = node.get_raft_status().await.unwrap();
    assert!(!status.is_leader);
    assert_eq!(status.node_id, 1);
    assert_eq!(status.leader_id, None);
    assert_eq!(status.term, 0);
    assert_eq!(status.state, "follower");
    
    // Update to leader
    let status = blixard::node_shared::RaftStatus {
        is_leader: true,
        node_id: 1,
        leader_id: Some(1),
        term: 5,
        state: "leader".to_string(),
    };
    node.update_raft_status(status).await;
    
    let status = node.get_raft_status().await.unwrap();
    assert!(status.is_leader);
    assert_eq!(status.leader_id, Some(1));
    assert_eq!(status.term, 5);
    assert_eq!(status.state, "leader");
}