use std::sync::Arc;
use blixard::node::Node;
use blixard::node_shared::SharedNodeState;
use blixard::types::NodeConfig;

#[test]
fn test_shared_node_state_is_send_sync() {
    // This test verifies that SharedNodeState is Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}
    
    assert_send_sync::<SharedNodeState>();
    assert_send_sync::<Arc<SharedNodeState>>();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_node_can_be_shared_via_arc() {
    let config = NodeConfig {
        id: 1,
        data_dir: "/tmp/test".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node = Node::new(config);
    let shared = node.shared();
    
    // This should work now - we can share the state across threads
    let shared_clone = Arc::clone(&shared);
    let handle = tokio::spawn(async move {
        // Access shared state from another task
        let id = shared_clone.get_id();
        assert_eq!(id, 1);
    });
    
    handle.await.unwrap();
}

#[tokio::test] 
async fn test_grpc_server_can_use_shared_state() {
    use blixard::grpc_server::BlixardGrpcService;
    
    let config = NodeConfig {
        id: 1,
        data_dir: "/tmp/test".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let node = Node::new(config);
    let shared = node.shared();
    
    // This should work - BlixardGrpcService can now use SharedNodeState
    let _service = BlixardGrpcService::new(shared);
}