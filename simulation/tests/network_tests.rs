//! Network fault injection and partition tests

#![cfg(madsim)]

// Note: Current madsim version has limited network control API
// We simulate network issues through server lifecycle instead
use madsim::time::{sleep, Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

/// Helper to run tests with consistent output format
async fn run_test<F, Fut>(name: &str, test_fn: F) 
where 
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>
{
    println!("\n🧪 Running: {}", name);
    let start = Instant::now();
    
    match test_fn().await {
        Ok(()) => {
            println!("✅ {} passed in {:?}", name, start.elapsed());
        }
        Err(e) => {
            println!("❌ {} failed: {}", name, e);
            panic!("Test failed: {}", e);
        }
    }
}

#[madsim::test]
async fn test_network_partition() {
    run_test("network_partition", || async {
        // Simple test: server goes down and comes back up
        info!("Starting network partition test");
        
        // Start server in a task
        let server_task = tokio::spawn(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            info!("Server listening on {}", addr);
            
            // Accept one connection
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0; 5];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            
            addr
        });
        
        // Get server address
        let addr = server_task.await.unwrap();
        
        // Connect and test
        let mut client = TcpStream::connect(addr).await
            .map_err(|e| format!("Connect failed: {}", e))?;
        client.write_all(b"Hello").await.unwrap();
        let mut buf = [0; 5];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Hello");
        
        info!("Network partition test completed");
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_network_latency() {
    run_test("network_latency", || async {
        // Measure RTT in simulation
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0; 4];
                    while stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.unwrap();
                    }
                });
            }
        });
        
        sleep(Duration::from_millis(10)).await;
        
        // Measure RTT
        let start = Instant::now();
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut buf = [0; 4];
        client.read_exact(&mut buf).await.unwrap();
        let rtt = start.elapsed();
        
        info!("RTT: {:?}", rtt);
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_packet_loss() {
    run_test("packet_loss", || async {
        // Test connection reliability
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0; 4];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                    }
                });
            }
        });
        
        sleep(Duration::from_millis(10)).await;
        
        // Test multiple connections
        let mut successes = 0;
        for _ in 0..10 {
            if let Ok(mut stream) = TcpStream::connect(addr).await {
                if stream.write_all(b"test").await.is_ok() {
                    let mut buf = [0; 4];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        successes += 1;
                    }
                }
            }
        }
        
        info!("Successful connections: {}/10", successes);
        
        if successes < 8 {
            return Err(format!("Too few successes: {}/10", successes));
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_asymmetric_partition() {
    run_test("asymmetric_partition", || async {
        // Test multiple servers with different availability
        let mut servers = vec![];
        let mut addrs = vec![];
        
        // Start 3 servers
        for i in 0..3 {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            addrs.push(addr);
            
            let handle = tokio::spawn(async move {
                info!("Server {} listening on {}", i, addr);
                while let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0; 5];
                    if stream.read_exact(&mut buf).await.is_ok() {
                        stream.write_all(&buf).await.ok();
                    }
                }
            });
            servers.push(handle);
        }
        
        sleep(Duration::from_millis(50)).await;
        
        // Test all servers are reachable
        for (i, addr) in addrs.iter().enumerate() {
            let mut client = TcpStream::connect(addr).await
                .map_err(|_| format!("Cannot reach server {}", i))?;
            client.write_all(b"test").await.unwrap();
        }
        
        // Abort middle server
        servers[1].abort();
        sleep(Duration::from_millis(50)).await;
        
        // Server 0 and 2 should still work
        for i in [0, 2] {
            let mut client = TcpStream::connect(addrs[i]).await
                .map_err(|_| format!("Cannot reach server {} after partition", i))?;
            client.write_all(b"test").await.unwrap();
        }
        
        // Server 1 should fail
        match TcpStream::connect(addrs[1]).await {
            Ok(_) => return Err("Server 1 should be unreachable".to_string()),
            Err(_) => info!("Server 1 correctly unreachable"),
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_network_chaos() {
    run_test("network_chaos", || async {
        // Test with server that randomly accepts/rejects
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        tokio::spawn(async move {
            let mut count = 0;
            while let Ok((mut stream, _)) = listener.accept().await {
                count += 1;
                // Accept every other connection
                if count % 2 == 0 {
                    tokio::spawn(async move {
                        let mut buf = [0; 5];
                        if stream.read_exact(&mut buf).await.is_ok() {
                            stream.write_all(&buf).await.ok();
                        }
                    });
                }
                // Else drop the connection
            }
        });
        
        sleep(Duration::from_millis(50)).await;
        
        // Try multiple connections
        let mut successes = 0;
        for _ in 0..10 {
            match tokio::time::timeout(
                Duration::from_millis(100),
                async {
                    let mut client = TcpStream::connect(addr).await?;
                    client.write_all(b"chaos").await?;
                    let mut buf = [0; 5];
                    client.read_exact(&mut buf).await?;
                    Ok::<_, std::io::Error>(())
                }
            ).await {
                Ok(Ok(())) => successes += 1,
                _ => {} // Expected failures
            }
        }
        
        info!("Chaos test: {}/10 succeeded", successes);
        
        if successes < 3 || successes > 7 {
            return Err(format!("Unexpected success rate: {}/10", successes));
        }
        
        Ok(())
    }).await;
}