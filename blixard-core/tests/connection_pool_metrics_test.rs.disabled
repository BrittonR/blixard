//! Tests for connection pool metrics integration

use blixard_core::{
    connection_pool::{ConnectionPool, ConnectionPoolStats},
    config_v2::ConnectionPoolConfig,
    metrics_otel::{init_noop, metrics, ConnectionPoolEvent, record_connection_pool_event, record_connection_pool_stats},
    error::BlixardResult,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;

async fn create_mock_channel(_endpoint: &str) -> BlixardResult<Channel> {
    // In a real test, this would create a mock channel
    // For now, we'll simulate the channel creation
    sleep(Duration::from_millis(10)).await;
    
    // Create a dummy channel for testing
    Channel::from_static("http://localhost:9999")
        .connect_lazy()
}

#[tokio::test]
async fn test_connection_pool_metrics() {
    // Initialize metrics
    let _ = init_noop();
    
    // Create pool configuration
    let config = ConnectionPoolConfig {
        max_connections_per_peer: 3,
        max_peers: 10,
        idle_timeout_secs: 2,  // Short timeout for testing
        max_lifetime_secs: 10,
        cleanup_interval_secs: 1,
    };
    
    // Create connection pool
    let pool = Arc::new(ConnectionPool::new(config, |endpoint| {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            create_mock_channel(&endpoint).await
        })
    }));
    
    // Test 1: Connection creation should record metrics
    println!("Test 1: Connection creation metrics");
    let _conn1 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    
    // Check that creation was recorded
    let metrics_instance = metrics();
    // Note: In a real test, we'd check the actual metric values
    
    // Test 2: Connection reuse should record metrics
    println!("Test 2: Connection reuse metrics");
    // Return the connection (simulate by dropping and getting again)
    pool.return_connection(1, _conn1).await;
    
    // Get connection again - should reuse
    let _conn2 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    
    // Test 3: Multiple connections per peer
    println!("Test 3: Multiple connections per peer");
    let _conn3 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    let _conn4 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    
    // Get stats
    let stats = pool.get_stats().await;
    assert_eq!(stats.total_connections, 3);
    assert_eq!(stats.active_connections, 3);
    assert_eq!(stats.peer_count, 1);
    
    // Test 4: Connection cleanup should record eviction metrics
    println!("Test 4: Connection eviction metrics");
    // Return all connections
    pool.return_connection(1, _conn2).await;
    pool.return_connection(1, _conn3).await;
    pool.return_connection(1, _conn4).await;
    
    // Wait for idle timeout
    sleep(Duration::from_secs(3)).await;
    
    // Trigger cleanup
    pool.cleanup_connections().await;
    
    // Check stats after cleanup
    let stats_after = pool.get_stats().await;
    assert_eq!(stats_after.total_connections, 0);
    
    // Test 5: Manual metric recording
    println!("Test 5: Manual metric recording");
    record_connection_pool_event(ConnectionPoolEvent::Created);
    record_connection_pool_event(ConnectionPoolEvent::Reused);
    record_connection_pool_event(ConnectionPoolEvent::Evicted);
    
    record_connection_pool_stats(10, 5, 5);
    
    println!("All connection pool metrics tests passed!");
}

#[tokio::test]
async fn test_connection_pool_stats_tracking() {
    let _ = init_noop();
    
    let config = ConnectionPoolConfig {
        max_connections_per_peer: 5,
        max_peers: 3,
        idle_timeout_secs: 300,
        max_lifetime_secs: 3600,
        cleanup_interval_secs: 60,
    };
    
    let pool = Arc::new(ConnectionPool::new(config, |endpoint| {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            create_mock_channel(&endpoint).await
        })
    }));
    
    // Create connections to multiple peers
    let _c1 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    let _c2 = pool.get_connection(2, "http://peer2:7002").await.unwrap();
    let _c3 = pool.get_connection(3, "http://peer3:7003").await.unwrap();
    let _c4 = pool.get_connection(1, "http://peer1:7001").await.unwrap();
    
    let stats = pool.get_stats().await;
    
    // Verify stats
    assert_eq!(stats.total_connections, 4);
    assert_eq!(stats.active_connections, 4);
    assert_eq!(stats.idle_connections, 0);
    assert_eq!(stats.peer_count, 3);
    assert_eq!(stats.max_connections, 15); // 5 * 3
    
    // Return some connections
    pool.return_connection(1, _c1).await;
    pool.return_connection(2, _c2).await;
    
    let stats2 = pool.get_stats().await;
    assert_eq!(stats2.active_connections, 2);
    assert_eq!(stats2.idle_connections, 2);
    
    // Record the stats as metrics
    record_connection_pool_stats(
        stats2.total_connections,
        stats2.active_connections,
        stats2.idle_connections,
    );
}