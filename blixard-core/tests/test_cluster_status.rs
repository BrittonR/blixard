// Simple test to verify GetClusterStatus works

#![cfg(feature = "test-helpers")]

#[cfg(test)]
mod tests {
    use blixard_core::{
        node::Node,
        node_shared::SharedNodeState,
        types::NodeConfig,
        grpc_server::services::cluster_service::ClusterServiceImpl,
        proto::{
            cluster_service_server::ClusterService,
            ClusterStatusRequest,
        },
    };
    use std::sync::Arc;
    use std::net::SocketAddr;
    use tonic::Request;

    #[tokio::test]
    async fn test_get_cluster_status_works() {
        // Create a simple node configuration
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test-node-1".to_string(),
            bind_addr: "127.0.0.1:7001".parse::<SocketAddr>().unwrap(),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
        };

        // Create shared node state
        let shared = Arc::new(SharedNodeState::new(config.clone()));

        // Create cluster service
        let service = ClusterServiceImpl::new(shared.clone(), None);

        // Create request
        let request = Request::new(ClusterStatusRequest {});

        // Call get_cluster_status
        match service.get_cluster_status(request).await {
            Ok(response) => {
                let resp = response.into_inner();
                println!("GetClusterStatus succeeded!");
                println!("Leader ID: {}", resp.leader_id);
                println!("Term: {}", resp.term);
                println!("Nodes: {:?}", resp.nodes);
                
                assert_eq!(resp.nodes.len(), 1); // Should have at least self
                assert_eq!(resp.nodes[0].id, 1); // Should be our node ID
            }
            Err(e) => {
                eprintln!("GetClusterStatus failed: {}", e);
                panic!("GetClusterStatus should not fail");
            }
        }
    }
}

#[cfg(not(test))]
fn main() {
    println!("This file is for testing only. Run with: cargo test --features test-helpers test_cluster_status -- --nocapture");
}