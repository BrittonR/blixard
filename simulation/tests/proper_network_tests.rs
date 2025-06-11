//! Proper MadSim network tests using Runtime and NodeIds

#![cfg(madsim)]

use std::time::Duration;
use madsim::{runtime::Runtime, Config};
use madsim::net::{NetSim, Config as NetConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

/// Test basic node creation and communication
#[test]
fn test_node_creation_and_communication() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        // Create two nodes with specific IPs
        let node1 = runtime.create_node()
            .name("server")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let node2 = runtime.create_node()
            .name("client")
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        info!("Created nodes: server={}, client={}", node1.id(), node2.id());
        
        // Start echo server on node1
        let server_task = node1.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server listening on 10.0.0.1:8080");
            
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0; 5];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            String::from_utf8(buf.to_vec()).unwrap()
        });
        
        // Connect from node2
        let client_task = node2.spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let mut stream = TcpStream::connect("10.0.0.1:8080").await.unwrap();
            stream.write_all(b"Hello").await.unwrap();
            
            let mut buf = [0; 5];
            stream.read_exact(&mut buf).await.unwrap();
            String::from_utf8(buf.to_vec()).unwrap()
        });
        
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        
        assert_eq!(server_result.unwrap(), "Hello");
        assert_eq!(client_result.unwrap(), "Hello");
    });
}

/// Test network partition using clog_node
#[test]
fn test_network_partition() {
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
        
        info!("Server node ID: {}, Client node ID: {}", server_id, client_id);
        
        // Start server
        let server_task = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server started, accepting connections");
            
            // Accept first connection (should work)
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0; 5];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            info!("Server: handled first connection");
            
            // Try to accept second connection (might fail due to partition)
            match tokio::time::timeout(Duration::from_secs(2), listener.accept()).await {
                Ok(Ok((mut stream, _))) => {
                    let mut buf = [0; 5];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                        info!("Server: handled second connection");
                        2
                    } else {
                        1
                    }
                }
                _ => {
                    info!("Server: second connection failed/timed out");
                    1
                }
            }
        });
        
        // Client connections
        let client_task = client_node.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // First connection - should work
            let mut stream1 = TcpStream::connect("10.0.0.1:8080").await.unwrap();
            stream1.write_all(b"test1").await.unwrap();
            let mut buf = [0; 5];
            stream1.read_exact(&mut buf).await.unwrap();
            info!("Client: first connection successful");
            drop(stream1);
            
            // Wait a bit
            tokio::time::sleep(Duration::from_millis(200)).await;
            
            // Second connection - may fail if partitioned
            match tokio::time::timeout(
                Duration::from_secs(1),
                TcpStream::connect("10.0.0.1:8080")
            ).await {
                Ok(Ok(mut stream)) => {
                    if stream.write_all(b"test2").await.is_ok() {
                        let mut buf = [0; 5];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            info!("Client: second connection successful");
                            return 2;
                        }
                    }
                    1
                }
                _ => {
                    info!("Client: second connection failed");
                    1
                }
            }
        });
        
        // Simulate partition by clogging the client node after first connection
        tokio::time::sleep(Duration::from_millis(300)).await;
        info!("Creating network partition - clogging client node");
        net.clog_node(client_id);
        
        // Wait a bit then restore
        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("Healing partition - unclogging client node");
        net.unclog_node(client_id);
        
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        
        let server_connections = server_result.unwrap();
        let client_connections = client_result.unwrap();
        
        info!("Server handled {} connections", server_connections);
        info!("Client made {} connections", client_connections);
        
        // At least one connection should have worked
        assert!(server_connections >= 1);
        assert!(client_connections >= 1);
    });
}

/// Test with configured packet loss and latency
#[test]
fn test_packet_loss_and_latency() {
    let mut config = Config::default();
    config.net = NetConfig {
        packet_loss_rate: 0.1, // 10% packet loss
        send_latency: Duration::from_millis(10)..Duration::from_millis(50),
    };
    
    let runtime = Runtime::with_seed_and_config(42, config);
    
    runtime.block_on(async {
        // Create nodes
        let server_node = runtime.create_node()
            .name("server")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let client_node = runtime.create_node()
            .name("client") 
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        // Start server
        let server_task = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            let mut successful_connections = 0;
            
            for i in 0..20 {
                match tokio::time::timeout(Duration::from_secs(1), listener.accept()).await {
                    Ok(Ok((mut stream, _))) => {
                        let mut buf = [0; 4];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            if stream.write_all(&buf).await.is_ok() {
                                successful_connections += 1;
                                info!("Server: successful connection {}", i);
                            }
                        }
                    }
                    _ => {
                        info!("Server: connection {} failed/timed out", i);
                    }
                }
            }
            successful_connections
        });
        
        // Client makes many connections
        let client_task = client_node.spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut successful_connections = 0;
            
            for i in 0..20 {
                match tokio::time::timeout(
                    Duration::from_secs(2),
                    async {
                        let mut stream = TcpStream::connect("10.0.0.1:8080").await?;
                        stream.write_all(b"test").await?;
                        let mut buf = [0; 4];
                        stream.read_exact(&mut buf).await?;
                        Ok::<_, std::io::Error>(())
                    }
                ).await {
                    Ok(Ok(())) => {
                        successful_connections += 1;
                        info!("Client: successful connection {}", i);
                    }
                    _ => {
                        info!("Client: connection {} failed", i);
                    }
                }
                
                // Small delay between attempts
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            successful_connections
        });
        
        let (server_result, client_result) = tokio::join!(server_task, client_task);
        
        let server_count = server_result.unwrap();
        let client_count = client_result.unwrap();
        
        info!("With 10% packet loss: server handled {}/20, client succeeded {}/20", 
              server_count, client_count);
        
        // With 10% packet loss, we expect some failures but not complete failure
        assert!(server_count >= 10, "Server handled too few connections: {}/20", server_count);
        assert!(client_count >= 10, "Client succeeded too few times: {}/20", client_count);
        assert!(server_count <= 20, "Server handled impossible count: {}/20", server_count);
        assert!(client_count <= 20, "Client succeeded impossible count: {}/20", client_count);
    });
}

/// Test asymmetric network partitions
#[test]
fn test_asymmetric_partition() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        // Create 3 nodes
        let node1 = runtime.create_node()
            .name("node1")
            .ip("10.0.0.1".parse().unwrap())
            .build();
            
        let node2 = runtime.create_node()
            .name("node2")
            .ip("10.0.0.2".parse().unwrap())
            .build();
            
        let node3 = runtime.create_node()
            .name("node3")
            .ip("10.0.0.3".parse().unwrap())
            .build();
        
        let _node1_id = node1.id();
        let node2_id = node2.id();
        let _node3_id = node3.id();
        
        // Start servers on all nodes
        let server1 = node1.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            let mut connections = 0;
            while let Ok(Ok((mut stream, _))) = tokio::time::timeout(
                Duration::from_millis(500), 
                listener.accept()
            ).await {
                let mut buf = [0; 4];
                if stream.read_exact(&mut buf).await.is_ok() {
                    stream.write_all(&buf).await.ok();
                    connections += 1;
                }
            }
            connections
        });
        
        let server2 = node2.spawn(async {
            let listener = TcpListener::bind("10.0.0.2:8080").await.unwrap();
            let mut connections = 0;
            while let Ok(Ok((mut stream, _))) = tokio::time::timeout(
                Duration::from_millis(500),
                listener.accept()
            ).await {
                let mut buf = [0; 4];
                if stream.read_exact(&mut buf).await.is_ok() {
                    stream.write_all(&buf).await.ok();
                    connections += 1;
                }
            }
            connections
        });
        
        let server3 = node3.spawn(async {
            let listener = TcpListener::bind("10.0.0.3:8080").await.unwrap();
            let mut connections = 0;
            while let Ok(Ok((mut stream, _))) = tokio::time::timeout(
                Duration::from_millis(500),
                listener.accept()
            ).await {
                let mut buf = [0; 4];
                if stream.read_exact(&mut buf).await.is_ok() {
                    stream.write_all(&buf).await.ok();
                    connections += 1;
                }
            }
            connections
        });
        
        // Wait for servers to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test connectivity from node1 to others
        let client_task = node1.spawn(async {
            let mut results = vec![];
            
            // Try to connect to node2
            match TcpStream::connect("10.0.0.2:8080").await {
                Ok(mut stream) => {
                    if stream.write_all(b"test").await.is_ok() {
                        results.push("node2_ok");
                    }
                }
                Err(_) => results.push("node2_fail"),
            }
            
            // Try to connect to node3  
            match TcpStream::connect("10.0.0.3:8080").await {
                Ok(mut stream) => {
                    if stream.write_all(b"test").await.is_ok() {
                        results.push("node3_ok");
                    }
                }
                Err(_) => results.push("node3_fail"),
            }
            
            results
        });
        
        // Create asymmetric partition: block node2's outgoing connections
        tokio::time::sleep(Duration::from_millis(50)).await;
        info!("Creating asymmetric partition - blocking node2 outgoing");
        net.clog_node_out(node2_id);
        
        let connectivity_results = client_task.await.unwrap();
        
        info!("Connectivity results: {:?}", connectivity_results);
        
        // Cleanup
        net.unclog_node_out(node2_id);
        
        let (s1, s2, s3) = tokio::join!(server1, server2, server3);
        info!("Server connections: node1={}, node2={}, node3={}", 
              s1.unwrap(), s2.unwrap(), s3.unwrap());
        
        // At least some connectivity should have worked
        assert!(!connectivity_results.is_empty());
    });
}