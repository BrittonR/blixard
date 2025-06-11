#![cfg(feature = "simulation")]

use std::sync::Arc;
use std::time::{Duration, Instant};

#[madsim::test]
async fn test_basic_madsim_functionality() {
    // Test basic time operations - time is simulated when run with --cfg madsim
    let start = Instant::now();
    madsim::time::sleep(Duration::from_secs(5)).await;
    let elapsed = start.elapsed();

    // In simulation, time advances exactly
    assert!(elapsed >= Duration::from_secs(5));
    assert!(elapsed < Duration::from_secs(6));
}

#[madsim::test]
async fn test_deterministic_execution() {
    // Test that execution is deterministic
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let mut handles = vec![];
    for i in 0..10 {
        let counter_clone = counter.clone();
        let handle = madsim::task::spawn(async move {
            madsim::time::sleep(Duration::from_millis(i as u64 * 100)).await;
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 10);
}

#[madsim::test]
async fn test_network_simulation() {
    use madsim::net::{TcpListener, TcpStream};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Use a unique port for this test
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    madsim::task::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        stream.write_all(&buf[..n]).await.unwrap();
        stream.flush().await.unwrap();
    });

    // Give server time to start
    madsim::time::sleep(Duration::from_millis(10)).await;

    // Connect as client
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(b"Hello, world!").await.unwrap();
    stream.flush().await.unwrap();

    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"Hello, world!");
}
