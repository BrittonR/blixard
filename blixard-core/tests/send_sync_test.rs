use blixard_core::node::Node;
use blixard_core::node_shared::SharedNodeState;
use blixard_core::types::NodeConfig;
use std::sync::Arc;

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
        vm_backend: "mock".to_string(),
        transport_config: None,
        topology: Default::default(),
    };

    let node = Node::new(config);
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

// Test removed: grpc_server module has been removed
// The SharedNodeState Send+Sync properties are still tested above
