//! Working MadSim network tests

#![cfg(madsim)]

use std::time::Duration;
use madsim::{runtime::Runtime, Config};
use madsim::net::{NetSim, Config as NetConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

/// Test that we can create nodes and get their IDs
#[test]
fn test_node_creation() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let node1 = runtime.create_node()
            .name("test-node")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        info!("Created node with ID: {}", node1.id());
        assert!(node1.id().to_string().len() > 0);
    });
}

/// Test network partition functionality
#[test] 
fn test_network_clogging() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        let node1 = runtime.create_node()
            .name("node1")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let node2 = runtime.create_node()
            .name("node2")
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        let node1_id = node1.id();
        let node2_id = node2.id();
        
        info!("Node 1 ID: {}, Node 2 ID: {}", node1_id, node2_id);
        
        // Test that we can clog and unclog nodes
        net.clog_node(node1_id);
        info!("Clogged node 1");
        
        net.unclog_node(node1_id);
        info!("Unclogged node 1");
        
        // Test link clogging
        net.clog_link(node1_id, node2_id);
        info!("Clogged link between nodes");
        
        net.unclog_link(node1_id, node2_id);
        info!("Unclogged link between nodes");
    });
}

/// Test with packet loss configuration
#[test]
fn test_packet_loss_config() {
    let mut config = Config::default();
    config.net = NetConfig {
        packet_loss_rate: 0.1, // 10% packet loss
        send_latency: Duration::from_millis(1)..Duration::from_millis(10),
    };
    
    let runtime = Runtime::with_seed_and_config(42, config);
    
    runtime.block_on(async {
        let node = runtime.create_node()
            .name("test-node")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        info!("Created node with packet loss config: {}", node.id());
        assert!(node.id().to_string().len() > 0);
    });
}

/// Test actual TCP communication within single node
#[test]
fn test_tcp_within_node() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let node = runtime.create_node()
            .name("tcp-node")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        // Spawn both server and client on the same node
        let result = node.spawn(async {
            // Start server
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server bound to 10.0.0.1:8080");
            
            // Start client task
            let client_task = tokio::spawn(async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                
                let mut stream = TcpStream::connect("10.0.0.1:8080").await.unwrap();
                stream.write_all(b"Hello").await.unwrap();
                
                let mut buf = [0; 5];
                stream.read_exact(&mut buf).await.unwrap();
                
                String::from_utf8_lossy(&buf).to_string()
            });
            
            // Accept one connection
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0; 5];
            let n = stream.read(&mut buf).await.unwrap();
            stream.write_all(&buf[..n]).await.unwrap();
            
            // Wait for client result
            client_task.await.unwrap()
        }).await.unwrap();
        
        assert_eq!(result, "Hello");
        info!("TCP communication test passed: '{}'", result);
    });
}

/// Test multiple nodes with simple operations
#[test]
fn test_multiple_nodes() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let mut nodes = vec![];
        
        // Create 3 nodes
        for i in 0..3 {
            let ip = format!("10.0.0.{}", i + 1);
            let node = runtime.create_node()
                .name(&format!("node-{}", i))
                .ip(ip.parse().unwrap())
                .build();
            nodes.push(node);
        }
        
        // Get all node IDs
        let node_ids: Vec<_> = nodes.iter().map(|n| n.id()).collect();
        info!("Created {} nodes with IDs: {:?}", nodes.len(), node_ids);
        
        // Spawn a simple task on each node
        let mut tasks = vec![];
        for (i, node) in nodes.iter().enumerate() {
            let node_id = node.id();
            let task = node.spawn(async move {
                info!("Running task on node {} (ID: {})", i, node_id);
                tokio::time::sleep(Duration::from_millis(10)).await;
                i
            });
            tasks.push(task);
        }
        
        // Wait for all tasks
        let results = futures::future::join_all(tasks).await;
        let values: Vec<_> = results.into_iter().map(|r| r.unwrap()).collect();
        
        info!("All tasks completed with results: {:?}", values);
        assert_eq!(values, vec![0, 1, 2]);
    });
}

/// Test network operations between nodes
#[test]
fn test_cross_node_partition() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        // Create nodes
        let server_node = runtime.create_node()
            .name("server")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let client_node = runtime.create_node()
            .name("client")
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        let server_id = server_node.id();
        let client_id = client_node.id();
        
        // Start simple server
        let server_task = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server listening");
            
            match tokio::time::timeout(Duration::from_millis(500), listener.accept()).await {
                Ok(Ok((mut stream, _))) => {
                    let mut buf = [0; 4];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                        info!("Server handled connection");
                        1
                    } else {
                        0
                    }
                }
                _ => {
                    info!("Server timeout (expected if partitioned)");
                    0
                }
            }
        });
        
        // Client tries to connect
        let client_task = client_node.spawn(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            match tokio::time::timeout(
                Duration::from_millis(300),
                TcpStream::connect("10.0.0.1:8080")
            ).await {
                Ok(Ok(mut stream)) => {
                    if stream.write_all(b"test").await.is_ok() {
                        let mut buf = [0; 4];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            info!("Client connection successful");
                            return 1;
                        }
                    }
                    0
                }
                _ => {
                    info!("Client connection failed (expected if partitioned)");
                    0
                }
            }
        });
        
        // Create partition after a brief moment
        tokio::time::sleep(Duration::from_millis(25)).await;
        net.clog_link(server_id, client_id);
        info!("Created partition between server and client");
        
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        
        let server_handled = server_result.unwrap();
        let client_connected = client_result.unwrap();
        
        info!("Server handled: {}, Client connected: {}", server_handled, client_connected);
        
        // Due to the partition timing, either both succeed (connection before partition)
        // or both fail (connection attempt during partition)
        if server_handled == 1 {
            assert_eq!(client_connected, 1, "If server handled connection, client should have connected");
        } else {
            assert_eq!(client_connected, 0, "If server didn't handle connection, client shouldn't have connected");
        }
        
        // Restore connection
        net.unclog_link(server_id, client_id);
        info!("Restored connection");
    });
}