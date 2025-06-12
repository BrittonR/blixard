//! MadSim tests for gRPC-like functionality without using actual tonic
//!
//! Since MadSim's tonic fork has compatibility issues, we test the
//! business logic directly without going through the gRPC layer.

#![cfg(madsim)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use madsim::{runtime::Handle, time::sleep};
use blixard::{
    node::Node,
    types::{NodeConfig, VmCommand, VmConfig},
};

#[madsim::test]
async fn test_node_grpc_operations() {
    let handle = Handle::current();
    
    // Create server node
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    server_node.spawn(async move {
        // Create node configuration
        let config = NodeConfig {
            id: 1,
            bind_addr: server_addr,
            data_dir: "/tmp/test".to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        // Create data directory - MadSim simulates the filesystem
        std::fs::create_dir_all("/tmp/test").expect("Failed to create data dir");
        
        // Create and initialize node
        let mut node = Node::new(config);
        node.initialize().await.expect("Failed to initialize node");
        node.start().await.expect("Failed to start node");
        
        let node_arc = Arc::new(node);
        
        // Simulate gRPC operations directly
        
        // Test health check
        let node_id = node_arc.get_id();
        assert_eq!(node_id, 1);
        
        // Test VM create
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            config_path: "/path/to/config".to_string(),
            vcpus: 2,
            memory: 1024,
        };
        
        let command = VmCommand::Create {
            config: vm_config,
            node_id,
        };
        
        node_arc.send_vm_command(command).await
            .expect("Failed to send VM create command");
        
        // Test VM list (should be empty since creation is not implemented)
        let vms = node_arc.list_vms().await.expect("Failed to list VMs");
        assert_eq!(vms.len(), 0);
        
        // Test cluster status
        match node_arc.get_cluster_status().await {
            Err(e) => {
                // Expected - not implemented yet
                println!("Got expected error: {}", e);
                assert!(e.to_string().contains("not yet implemented") || 
                        e.to_string().contains("NotImplemented") ||
                        e.to_string().contains("Feature not implemented"));
            }
            Ok(_) => panic!("Expected not implemented error"),
        }
        
        // Keep node running for a bit
        sleep(Duration::from_secs(2)).await;
    }).await.unwrap();
}

#[madsim::test]
async fn test_concurrent_node_operations() {
    let handle = Handle::current();
    
    // Create multiple nodes
    let mut nodes = vec![];
    for i in 0..3 {
        let addr: SocketAddr = format!("10.0.0.{}:7001", i + 1).parse().unwrap();
        let node = handle.create_node()
            .name(&format!("node{}", i))
            .ip(addr.ip())
            .build();
        nodes.push((node, addr, i as u64 + 1));
    }
    
    // Start all nodes
    let mut handles = vec![];
    for (node, addr, id) in nodes {
        let handle = node.spawn(async move {
            let config = NodeConfig {
                id,
                bind_addr: addr,
                data_dir: format!("/tmp/test{}", id),
                join_addr: None,
                use_tailscale: false,
            };
            
            // Create data directory
            std::fs::create_dir_all(&format!("/tmp/test{}", id)).unwrap();
            
            let mut node = Node::new(config);
            node.initialize().await.expect("Failed to initialize node");
            node.start().await.expect("Failed to start node");
            
            let node_arc = Arc::new(node);
            
            // Simulate concurrent VM operations
            for j in 0..5 {
                let vm_config = VmConfig {
                    name: format!("vm-{}-{}", id, j),
                    config_path: "/path/to/config".to_string(),
                    vcpus: 1,
                    memory: 512,
                };
                
                let command = VmCommand::Create {
                    config: vm_config,
                    node_id: id,
                };
                
                node_arc.send_vm_command(command).await
                    .expect("Failed to send VM command");
                
                sleep(Duration::from_millis(100)).await;
            }
            
            // List VMs at the end
            let vms = node_arc.list_vms().await.expect("Failed to list VMs");
            println!("Node {} has {} VMs", id, vms.len());
            
            sleep(Duration::from_secs(1)).await;
        });
        handles.push(handle);
    }
    
    // Wait for all nodes to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[madsim::test]
async fn test_node_lifecycle_with_network_issues() {
    let handle = Handle::current();
    let net = madsim::net::NetSim::current();
    
    // Create two nodes
    let node1_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let node2_addr: SocketAddr = "10.0.0.2:7001".parse().unwrap();
    
    let node1 = handle.create_node()
        .name("node1")
        .ip(node1_addr.ip())
        .build();
    
    let node2 = handle.create_node()
        .name("node2")
        .ip(node2_addr.ip())
        .build();
    
    // Start node1
    node1.spawn(async move {
        // Create data directory
        std::fs::create_dir_all("/tmp/test1").unwrap();
        
        let config = NodeConfig {
            id: 1,
            bind_addr: node1_addr,
            data_dir: "/tmp/test1".to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        
        // Keep running
        sleep(Duration::from_secs(10)).await;
    });
    
    // Start node2
    node2.spawn(async move {
        // Create data directory
        std::fs::create_dir_all("/tmp/test2").unwrap();
        
        let config = NodeConfig {
            id: 2,
            bind_addr: node2_addr,
            data_dir: "/tmp/test2".to_string(),
            join_addr: Some(node1_addr),
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        
        // Try to join cluster (will fail - not implemented)
        match node.join_cluster(Some(node1_addr)).await {
            Err(e) => {
                assert!(e.to_string().contains("not yet implemented") || 
                        e.to_string().contains("NotImplemented") ||
                        e.to_string().contains("Feature not implemented"));
            }
            Ok(_) => panic!("Expected not implemented error"),
        }
        
        sleep(Duration::from_secs(5)).await;
    });
    
    // Simulate network partition after 2 seconds
    sleep(Duration::from_secs(2)).await;
    net.clog_node(node1.id());
    
    // Heal partition after 3 more seconds
    sleep(Duration::from_secs(3)).await;
    net.unclog_node(node1.id());
    
    // Let things settle
    sleep(Duration::from_secs(2)).await;
}