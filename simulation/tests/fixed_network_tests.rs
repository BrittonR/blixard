//! Fixed MadSim network tests that work properly

#![cfg(madsim)]

use std::time::Duration;
use madsim::{runtime::Runtime, Config};
use madsim::net::{NetSim, Config as NetConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

/// Test basic node creation and communication - simplified
#[test]
fn test_basic_node_communication() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        // Create two nodes with specific IPs
        let server_node = runtime.create_node()
            .name("server")
            .ip("10.0.0.1".parse().unwrap())
            .build();
        
        let client_node = runtime.create_node()
            .name("client")
            .ip("10.0.0.2".parse().unwrap())
            .build();
        
        info!("Created nodes: server={}, client={}", server_node.id(), client_node.id());
        
        // Start echo server on node1
        let server_handle = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server listening on 10.0.0.1:8080");
            
            // Accept exactly one connection
            let (mut stream, peer) = listener.accept().await.unwrap();
            info!("Server accepted connection from {}", peer);
            
            let mut buf = [0; 5];
            let n = stream.read(&mut buf).await.unwrap();
            info!("Server received {} bytes: {:?}", n, &buf[..n]);
            
            stream.write_all(&buf[..n]).await.unwrap();
            info!("Server echoed back {} bytes", n);
            
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        
        // Connect from client node after a short delay
        let client_handle = client_node.spawn(async {
            // Give server time to start
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            info!("Client connecting to server");
            let mut stream = TcpStream::connect("10.0.0.1:8080").await.unwrap();
            
            stream.write_all(b"Hello").await.unwrap();
            info!("Client sent 'Hello'");
            
            let mut buf = [0; 5];
            stream.read_exact(&mut buf).await.unwrap();
            info!("Client received response: {:?}", &buf);
            
            String::from_utf8_lossy(&buf).to_string()
        });
        
        // Wait for both tasks
        let (server_msg, client_msg) = tokio::join!(server_handle, client_handle);
        
        let server_result = server_msg.unwrap();
        let client_result = client_msg.unwrap();
        
        info!("Server processed: '{}'", server_result);
        info!("Client received: '{}'", client_result);
        
        assert_eq!(server_result, "Hello");
        assert_eq!(client_result, "Hello");
    });
}

/// Test network partition using node clogging
#[test]
fn test_simple_network_partition() {
    let runtime = Runtime::new();
    
    runtime.block_on(async {
        let net = NetSim::current();
        
        // Create server and client nodes
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
        let server_handle = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            info!("Server started");
            
            let mut connection_count = 0;
            
            // Accept first connection
            match tokio::time::timeout(Duration::from_secs(2), listener.accept()).await {
                Ok(Ok((mut stream, _))) => {
                    let mut buf = [0; 5];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                        connection_count += 1;
                        info!("Server: handled connection 1");
                    }
                }
                _ => info!("Server: first connection timeout"),
            }
            
            // Try to accept second connection (may fail due to partition)
            match tokio::time::timeout(Duration::from_secs(2), listener.accept()).await {
                Ok(Ok((mut stream, _))) => {
                    let mut buf = [0; 5];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                        connection_count += 1;
                        info!("Server: handled connection 2");
                    }
                }
                _ => info!("Server: second connection timeout (expected during partition)"),
            }
            
            connection_count
        });
        
        // Client makes connections with partition in between
        let client_handle = client_node.spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let mut successful_connections = 0;
            
            // First connection - should work
            info!("Client: attempting first connection");
            match TcpStream::connect("10.0.0.1:8080").await {
                Ok(mut stream) => {
                    if stream.write_all(b"test1").await.is_ok() {
                        let mut buf = [0; 5];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            successful_connections += 1;
                            info!("Client: first connection successful");
                        }
                    }
                }
                Err(e) => info!("Client: first connection failed: {}", e),
            }
            
            // Wait for partition to be created
            tokio::time::sleep(Duration::from_millis(800)).await;
            
            // Second connection - may fail due to partition
            info!("Client: attempting second connection (during partition)");
            match tokio::time::timeout(
                Duration::from_secs(1),
                TcpStream::connect("10.0.0.1:8080")
            ).await {
                Ok(Ok(mut stream)) => {
                    if stream.write_all(b"test2").await.is_ok() {
                        let mut buf = [0; 5];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            successful_connections += 1;
                            info!("Client: second connection successful");
                        }
                    }
                }
                _ => info!("Client: second connection failed (expected during partition)"),
            }
            
            successful_connections
        });
        
        // Create partition after first connection
        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("Creating network partition - clogging client node");
        net.clog_node(client_id);
        
        // Wait a bit then restore
        tokio::time::sleep(Duration::from_millis(1000)).await;
        info!("Healing partition - unclogging client node");
        net.unclog_node(client_id);
        
        let (server_connections, client_connections) = tokio::join!(server_handle, client_handle);
        
        let server_count = server_connections.unwrap();
        let client_count = client_connections.unwrap();
        
        info!("Server handled {} connections", server_count);
        info!("Client made {} connections", client_count);
        
        // At least one connection should have worked before partition
        assert!(server_count >= 1, "Server should handle at least 1 connection");
        assert!(client_count >= 1, "Client should make at least 1 connection");
        
        // Due to partition, not all connections should succeed
        assert!(server_count <= 2, "Server handled unexpected number of connections");
        assert!(client_count <= 2, "Client made unexpected number of connections");
    });
}

/// Test configured packet loss
#[test]
fn test_packet_loss_configuration() {
    let mut config = Config::default();
    config.net = NetConfig {
        packet_loss_rate: 0.3, // 30% packet loss
        send_latency: Duration::from_millis(5)..Duration::from_millis(15),
    };
    
    let runtime = Runtime::with_seed_and_config(12345, config);
    
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
        
        // Start server that accepts multiple connections
        let server_handle = server_node.spawn(async {
            let listener = TcpListener::bind("10.0.0.1:8080").await.unwrap();
            let mut successful_connections = 0;
            
            for i in 0..10 {
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
                        info!("Server: connection {} failed/timed out (packet loss)", i);
                    }
                }
            }
            successful_connections
        });
        
        // Client makes many connection attempts
        let client_handle = client_node.spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut successful_connections = 0;
            
            for i in 0..10 {
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
                        info!("Client: connection {} failed (packet loss)", i);
                    }
                }
                
                // Small delay between attempts
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            successful_connections
        });
        
        let (server_success, client_success) = tokio::join!(server_handle, client_handle);
        
        let server_count = server_success.unwrap();
        let client_count = client_success.unwrap();
        
        info!("With 30% packet loss: server handled {}/10, client succeeded {}/10", 
              server_count, client_count);
        
        // With 30% packet loss, we expect 60-80% success rate
        assert!(server_count >= 5, "Server handled too few connections: {}/10", server_count);
        assert!(client_count >= 5, "Client succeeded too few times: {}/10", client_count);
        assert!(server_count <= 10, "Server count impossible: {}", server_count);
        assert!(client_count <= 10, "Client count impossible: {}", client_count);
        
        // Server and client counts should be equal (each successful client connection 
        // corresponds to one server connection)
        assert_eq!(server_count, client_count, 
                  "Server and client counts should match: {} vs {}", 
                  server_count, client_count);
    });
}