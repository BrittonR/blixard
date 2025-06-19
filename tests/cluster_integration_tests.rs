#![cfg(feature = "test-helpers")]

use std::time::Duration;

use blixard::{
    proto::{
        cluster_service_client::ClusterServiceClient,
        TaskRequest, HealthCheckRequest, ClusterStatusRequest,
        CreateVmRequest, ListVmsRequest,
    },
    test_helpers::{TestNode, TestCluster},
};

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_node_grpc_health_check() {
    // Use TestNode for proper lifecycle management
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    // Get client from TestNode
    let mut client = node.client().await.unwrap();
    
    // Health check
    let response = client.health_check(HealthCheckRequest {})
        .await
        .unwrap();
    
    assert!(response.into_inner().healthy);
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cluster_formation_via_grpc() {
    // Use TestCluster for multi-node testing
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .unwrap();
    
    // Wait for cluster to converge
    cluster.wait_for_convergence(Duration::from_secs(10)).await.unwrap();
    
    // Verify all nodes see the same cluster state
    for node in cluster.nodes().values() {
        let mut client = node.client().await.unwrap();
        let status = client.get_cluster_status(ClusterStatusRequest {})
            .await
            .unwrap()
            .into_inner();
        
        // Should see 3 nodes
        assert_eq!(status.nodes.len(), 3);
        assert!(status.leader_id > 0);
    }
    
    // Cleanup
    cluster.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cluster_task_submission() {
    // Use TestCluster for multi-node testing
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .unwrap();
    
    // Wait for cluster to converge
    cluster.wait_for_convergence(Duration::from_secs(10)).await.unwrap();
    
    // Get leader client
    let mut leader_client = cluster.leader_client().await.unwrap();
    
    // Submit a task
    let request = TaskRequest {
        task_id: "test-task".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        cpu_cores: 1,
        memory_mb: 128,
        disk_gb: 1,
        required_features: vec![],
        timeout_secs: 60,
    };
    
    let response = leader_client.submit_task(request)
        .await
        .unwrap();
    
    assert!(response.into_inner().accepted);
    
    // Cleanup
    cluster.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_operations_on_cluster() {
    // Use TestCluster for multi-node testing
    let cluster = TestCluster::builder()
        .with_nodes(2)
        .build()
        .await
        .unwrap();
    
    // Wait for cluster to converge
    cluster.wait_for_convergence(Duration::from_secs(10)).await.unwrap();
    
    // Get leader client
    let mut leader_client = cluster.leader_client().await.unwrap();
    
    // Create a VM
    let create_request = CreateVmRequest {
        name: "test-vm".to_string(),
        config_path: "/path/to/config".to_string(),
        vcpus: 2,
        memory_mb: 512,
    };
    
    let create_response = leader_client.create_vm(create_request)
        .await
        .unwrap();
    
    assert!(create_response.into_inner().success);
    
    // List VMs
    let list_response = leader_client.list_vms(ListVmsRequest {})
        .await
        .unwrap();
    
    let vms = list_response.into_inner().vms;
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "test-vm");
    
    // Cleanup
    cluster.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Known issue: Non-leader nodes don't properly update conf state after RemoveNode - see RAFT_CONSENSUS_ENFORCEMENT.md"]
async fn test_node_failure_handling() {
    // Use TestCluster for multi-node testing
    let mut cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .unwrap();
    
    // Wait for cluster to converge
    cluster.wait_for_convergence(Duration::from_secs(10)).await.unwrap();
    
    // Get initial leader
    let initial_leader_id = {
        let status = cluster.nodes().values().next().unwrap()
            .shared_state.get_raft_status().await.unwrap();
        status.leader_id.unwrap()
    };
    
    // Remove a non-leader node
    let node_to_remove = cluster.nodes().keys()
        .find(|&&id| id != initial_leader_id)
        .cloned()
        .unwrap();
    
    eprintln!("Initial leader: {}, removing node: {}", initial_leader_id, node_to_remove);
    cluster.remove_node(node_to_remove).await.unwrap();
    
    // Wait for configuration change to propagate
    let start = tokio::time::Instant::now();
    let timeout = Duration::from_secs(10);
    let mut all_nodes_see_removal = false;
    
    while start.elapsed() < timeout && !all_nodes_see_removal {
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        all_nodes_see_removal = true;
        for node in cluster.nodes().values() {
            let mut client = match node.client().await {
                Ok(c) => c,
                Err(_) => {
                    all_nodes_see_removal = false;
                    continue;
                }
            };
            
            let status = match client.get_cluster_status(ClusterStatusRequest {}).await {
                Ok(s) => s.into_inner(),
                Err(_) => {
                    all_nodes_see_removal = false;
                    continue;
                }
            };
            
            if status.nodes.len() != 2 {
                all_nodes_see_removal = false;
                eprintln!("Node {} still sees {} nodes: {:?}", node.id, status.nodes.len(), status.nodes);
                break;
            }
        }
    }
    
    if !all_nodes_see_removal {
        eprintln!("Timeout waiting for nodes to see removal after {} seconds", timeout.as_secs());
    }
    
    // Now verify all nodes see the correct state
    for node in cluster.nodes().values() {
        let mut client = node.client().await.unwrap();
        let status = client.get_cluster_status(ClusterStatusRequest {})
            .await
            .unwrap()
            .into_inner();
        
        assert!(status.leader_id > 0, "Node {} doesn't see a leader", node.id);
        assert_eq!(status.nodes.len(), 2, 
            "Node {} sees {} nodes instead of 2. Nodes: {:?}", 
            node.id, status.nodes.len(), status.nodes);
    }
    
    // Cleanup
    cluster.shutdown().await;
}