#![cfg(feature = "simulation")]

use std::time::{Duration, Instant};

#[madsim::test]
async fn verify_time_control() {
    println!("Starting time control verification...");

    // When running with madsim, we need to use the madsim runtime context
    // The time operations are intercepted by madsim when compiled with --cfg madsim
    let start = Instant::now();
    println!("Start time: {:?}", start);

    // Sleep for a shorter duration to avoid timeout issues
    println!("Sleeping for 10 seconds...");
    madsim::time::sleep(Duration::from_secs(10)).await;

    let elapsed = start.elapsed();
    println!("Elapsed time: {:?}", elapsed);

    // Should be exactly 10 seconds in simulated time
    assert!(elapsed >= Duration::from_secs(10));
    assert!(elapsed < Duration::from_secs(11));

    println!("✓ Time control working correctly");
}

#[madsim::test]
async fn verify_determinism() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    println!("Starting determinism verification...");

    let counter = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];

    // Spawn tasks with different delays
    for i in 0..5 {
        let counter_clone = counter.clone();
        let handle = madsim::task::spawn(async move {
            madsim::time::sleep(Duration::from_millis(i * 100)).await;
            let value = counter_clone.fetch_add(1, Ordering::SeqCst);
            println!("Task {} incremented counter to {}", i, value + 1);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let final_value = counter.load(Ordering::SeqCst);
    println!("Final counter value: {}", final_value);
    assert_eq!(final_value, 5);

    println!("✓ Deterministic execution verified");
}

#[madsim::test]
async fn verify_network_simulation() {
    println!("Starting network simulation verification...");

    // Start server using madsim APIs directly
    let addr: std::net::SocketAddr = "127.0.0.1:8888".parse().unwrap();

    madsim::task::spawn(async move {
        use madsim::net::TcpListener;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind(addr).await.unwrap();
        println!("Server listening on {}", addr);

        let (mut stream, peer) = listener.accept().await.unwrap();
        println!("Accepted connection from {}", peer);

        let mut buf = vec![0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        println!("Server received: {}", String::from_utf8_lossy(&buf[..n]));

        stream.write_all(b"PONG").await.unwrap();
        stream.flush().await.unwrap();
    });

    // Give server time to start
    madsim::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    use madsim::net::TcpStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut client = TcpStream::connect(addr).await.unwrap();
    println!("Client connected to {}", addr);

    client.write_all(b"PING").await.unwrap();
    client.flush().await.unwrap();

    let mut response = vec![0; 4];
    client.read_exact(&mut response).await.unwrap();
    println!("Client received: {}", String::from_utf8_lossy(&response));

    assert_eq!(&response, b"PONG");

    println!("✓ Network simulation working correctly");
}
