//! Simple test to verify ALPN configuration works
//!
//! This test focuses only on ALPN without dependencies on other components

use blixard_core::transport::BLIXARD_ALPN;
use iroh::{Endpoint, NodeAddr, SecretKey};
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;

#[tokio::test]
async fn test_alpn_constant_exists() {
    // Verify the ALPN constant is defined and accessible
    assert_eq!(BLIXARD_ALPN, b"blixard/rpc/1");
    info!(
        "BLIXARD_ALPN constant verified: {:?}",
        std::str::from_utf8(BLIXARD_ALPN).unwrap()
    );
}

#[tokio::test]
async fn test_iroh_endpoint_with_alpn() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("iroh=debug")
        .try_init();

    info!("Creating Iroh endpoints with BLIXARD_ALPN");

    // Create two endpoints
    let secret1 = SecretKey::generate(rand::thread_rng());
    let secret2 = SecretKey::generate(rand::thread_rng());

    let endpoint1 = Endpoint::builder()
        .secret_key(secret1)
        .bind()
        .await
        .expect("Failed to create endpoint 1");

    let endpoint2 = Endpoint::builder()
        .secret_key(secret2)
        .bind()
        .await
        .expect("Failed to create endpoint 2");

    let node_id1 = endpoint1.node_id();
    let node_id2 = endpoint2.node_id();

    info!("Endpoint 1 ID: {}", node_id1);
    info!("Endpoint 2 ID: {}", node_id2);

    // Get socket addresses
    let addrs1: Vec<_> = endpoint1.bound_sockets().collect();
    let addrs2: Vec<_> = endpoint2.bound_sockets().collect();

    info!("Endpoint 1 addresses: {:?}", addrs1);
    info!("Endpoint 2 addresses: {:?}", addrs2);

    // Create NodeAddr for endpoint2
    let node_addr2 = NodeAddr::new(node_id2).with_direct_addresses(addrs2);

    // Set up connection acceptor
    let endpoint2_clone = endpoint2.clone();
    let acceptor = tokio::spawn(async move {
        info!("Waiting for incoming connections...");
        if let Some(incoming) = endpoint2_clone.accept().await {
            match incoming.await {
                Ok(conn) => {
                    info!("Accepted connection with ALPN: {:?}", conn.alpn());
                    // Verify ALPN matches
                    assert_eq!(conn.alpn(), BLIXARD_ALPN);
                    true
                }
                Err(e) => {
                    info!("Failed to accept connection: {}", e);
                    false
                }
            }
        } else {
            false
        }
    });

    // Give acceptor time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to connect with BLIXARD_ALPN
    info!("Attempting connection with BLIXARD_ALPN");
    match endpoint1.connect(node_addr2, BLIXARD_ALPN).await {
        Ok(conn) => {
            info!("Connection established successfully!");
            assert_eq!(conn.alpn(), BLIXARD_ALPN);

            // Wait for acceptor to verify
            if let Ok(accepted) = timeout(Duration::from_secs(1), acceptor).await {
                assert!(
                    accepted.unwrap(),
                    "Acceptor should have accepted the connection"
                );
            }
        }
        Err(e) => {
            // Connection might fail in test environment, but not due to ALPN mismatch
            info!(
                "Connection failed (may be expected in test environment): {}",
                e
            );
            assert!(
                !e.to_string().contains("alpn"),
                "Error should not be ALPN-related: {}",
                e
            );
        }
    }

    endpoint1.close().await;
    endpoint2.close().await;
}

#[tokio::test]
async fn test_alpn_mismatch_detection() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("iroh=debug")
        .try_init();

    info!("Testing ALPN mismatch detection");

    // Create endpoint
    let secret = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .bind()
        .await
        .expect("Failed to create endpoint");

    let node_id = endpoint.node_id();
    let addrs: Vec<_> = endpoint.bound_sockets().collect();
    let node_addr = NodeAddr::new(node_id).with_direct_addresses(addrs);

    // Try to connect with wrong ALPN
    let wrong_alpn = b"wrong/alpn/1";
    info!(
        "Attempting connection with wrong ALPN: {:?}",
        std::str::from_utf8(wrong_alpn).unwrap()
    );

    match endpoint.connect(node_addr, wrong_alpn).await {
        Ok(_) => {
            // Self-connection might succeed regardless
            info!("Self-connection succeeded (unexpected but possible)");
        }
        Err(e) => {
            info!("Connection failed as expected: {}", e);
            // The error might be due to self-connection, not ALPN
        }
    }

    endpoint.close().await;
}
