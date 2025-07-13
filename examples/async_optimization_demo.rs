//! Demonstration of async optimization and connection pooling features
//!
//! This example showcases the advanced async patterns and connection pooling
//! implementations in Blixard, including HTTP client pooling, optimized peer
//! connections, streaming batch processing, and resource-aware concurrency.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tracing::{info, warn};

use blixard_core::{
    common::async_utils::{
        concurrent_map, concurrent_try_map, stream_process, 
        AdaptiveConcurrencyController, ResourceAwareConcurrentProcessor,
        GenericConnectionPool, WeightedLoadBalancer, LoadBalancingStrategy,
        CircuitBreaker, CircuitBreakerError,
    },
    transport::http_client_pool::{
        HttpClientPoolConfig, init_global_pool, get_global_client, get_global_pool_stats,
    },
    transport::optimized_peer_connector::{
        OptimizedPeerConnectorConfig, OptimizedPeerConnector,
    },
    raft::optimized_batch_processor::{
        OptimizedBatchConfig, EnhancedRaftProposal, ProposalPriority,
        create_optimized_batch_processor,
    },
    error::BlixardResult,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse()?)
                .add_directive("async_optimization_demo=info".parse()?),
        )
        .init();

    info!("Starting async optimization demo");

    // Demonstrate HTTP client pooling
    demo_http_client_pooling().await?;

    // Demonstrate advanced async patterns
    demo_async_patterns().await?;

    // Demonstrate resource-aware processing
    demo_resource_aware_processing().await?;

    // Demonstrate circuit breaker
    demo_circuit_breaker().await?;

    // Demonstrate load balancing
    demo_load_balancing().await?;

    // Demonstrate concurrent error handling
    demo_concurrent_error_handling().await?;

    // Demonstrate streaming processing
    demo_streaming_processing().await?;

    info!("Async optimization demo completed successfully!");
    Ok(())
}

/// Demonstrate HTTP client connection pooling
async fn demo_http_client_pooling() -> BlixardResult<()> {
    info!("\n=== HTTP Client Connection Pooling Demo ===");

    // Initialize global HTTP client pool
    let pool_config = HttpClientPoolConfig {
        max_clients_per_host: 3,
        max_total_clients: 20,
        idle_timeout: Duration::from_secs(60),
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive: true,
        cleanup_interval: Duration::from_secs(30),
    };

    init_global_pool(pool_config).await?;
    info!("Initialized HTTP client pool");

    // Simulate multiple requests to the same host
    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            // Get pooled client for httpbin.org
            if let Ok(client) = get_global_client("httpbin.org").await {
                info!("Request {}: Got pooled HTTP client", i + 1);
                
                // Simulate work with the client
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                // Client automatically returns to pool when dropped
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(format!("Request {} completed", i + 1))
            } else {
                Err("Failed to get client".into())
            }
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        let result = handle.await??;
        info!("  {}", result);
    }

    let elapsed = start.elapsed();
    info!("All HTTP requests completed in {:?}", elapsed);

    // Show pool statistics
    if let Ok(stats) = get_global_pool_stats().await {
        info!("Pool stats: {} total clients, {} requests", 
              stats.total_clients, stats.total_requests);
    }

    Ok(())
}

/// Demonstrate advanced async patterns
async fn demo_async_patterns() -> BlixardResult<()> {
    info!("\n=== Advanced Async Patterns Demo ===");

    // Demo 1: Concurrent map with limited parallelism
    info!("Demo 1: Concurrent processing with limited parallelism");
    let numbers = (1..=20).collect::<Vec<_>>();
    
    let start = Instant::now();
    let results = concurrent_map(
        numbers,
        5, // Max 5 concurrent operations
        |n| async move {
            // Simulate varying work times
            let delay = Duration::from_millis(10 + (n % 5) * 10);
            tokio::time::sleep(delay).await;
            n * 2
        },
    ).await;
    
    info!("  Processed {} items in {:?}", results.len(), start.elapsed());
    info!("  Results: {:?}", &results[..5]); // Show first 5

    // Demo 2: Adaptive concurrency control
    info!("Demo 2: Adaptive concurrency control");
    let mut adaptive_controller = AdaptiveConcurrencyController::new(1, 10);
    
    for round in 0..3 {
        let start = Instant::now();
        let concurrency = adaptive_controller.get_concurrency();
        
        info!("  Round {}: Using concurrency level {}", round + 1, concurrency);
        
        // Simulate work
        let work_items = (1..=20).collect::<Vec<_>>();
        let _results = concurrent_map(
            work_items,
            concurrency,
            |_| async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                "completed"
            },
        ).await;
        
        let elapsed = start.elapsed();
        adaptive_controller.record_performance(elapsed).await;
        
        info!("    Completed in {:?}", elapsed);
    }

    Ok(())
}

/// Demonstrate resource-aware processing
async fn demo_resource_aware_processing() -> BlixardResult<()> {
    info!("\n=== Resource-Aware Processing Demo ===");

    let mut processor = ResourceAwareConcurrentProcessor::new(
        0.8, // CPU threshold
        0.8, // Memory threshold
        8,   // Max concurrency
    );

    // Simulate different workload sizes
    let workloads = vec![
        ("Small workload", (1..=10).collect::<Vec<_>>()),
        ("Medium workload", (1..=50).collect::<Vec<_>>()),
        ("Large workload", (1..=100).collect::<Vec<_>>()),
    ];

    for (name, workload) in workloads {
        info!("Processing {}: {} items", name, workload.len());
        
        let start = Instant::now();
        let results = processor.process(
            workload,
            |item| async move {
                // Simulate CPU-intensive work
                tokio::time::sleep(Duration::from_millis(5)).await;
                item * item
            },
        ).await;
        
        info!("  Completed {} items in {:?}", results.len(), start.elapsed());
    }

    Ok(())
}

/// Demonstrate circuit breaker pattern
async fn demo_circuit_breaker() -> BlixardResult<()> {
    info!("\n=== Circuit Breaker Demo ===");

    let circuit_breaker = CircuitBreaker::new(
        3, // Failure threshold
        Duration::from_secs(5), // Timeout
    );

    // Simulate operations with varying success rates
    let operations = vec![
        ("Successful operation", true),
        ("Another success", true),
        ("Failed operation", false),
        ("Another failure", false),
        ("Third failure", false), // This should open the circuit
        ("Fourth failure", false), // This should fail fast
        ("Attempted operation", true), // This should fail fast too
    ];

    for (description, should_succeed) in operations {
        let result = circuit_breaker.call(|| async {
            if should_succeed {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<&str, &str>("success")
            } else {
                Err("simulated failure")
            }
        }).await;

        match result {
            Ok(value) => info!("  {}: {:?}", description, value),
            Err(CircuitBreakerError::CircuitOpen) => {
                warn!("  {}: Circuit breaker is open!", description);
            }
            Err(CircuitBreakerError::OperationFailed(e)) => {
                warn!("  {}: Operation failed: {}", description, e);
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for circuit breaker to reset
    info!("Waiting for circuit breaker to reset...");
    tokio::time::sleep(Duration::from_secs(6)).await;

    // Try operation after reset
    let result = circuit_breaker.call(|| async {
        Ok::<&str, &str>("recovered")
    }).await;

    match result {
        Ok(value) => info!("  Recovery test: {:?}", value),
        Err(e) => warn!("  Recovery test failed: {:?}", e),
    }

    Ok(())
}

/// Demonstrate load balancing
async fn demo_load_balancing() -> BlixardResult<()> {
    info!("\n=== Load Balancing Demo ===");

    // Create load balancer with different strategies
    let strategies = vec![
        ("Round Robin", LoadBalancingStrategy::RoundRobin),
        ("Weighted Random", LoadBalancingStrategy::WeightedRandom),
        ("Least Connections", LoadBalancingStrategy::LeastConnections),
    ];

    for (name, strategy) in strategies {
        info!("Testing {} strategy:", name);
        
        let load_balancer = WeightedLoadBalancer::new(strategy);
        
        // Add resources with different weights
        load_balancer.add_resource("Server 1".to_string(), 1.0).await;
        load_balancer.add_resource("Server 2".to_string(), 2.0).await;
        load_balancer.add_resource("Server 3".to_string(), 0.5).await;

        // Select resources multiple times
        let mut selections = std::collections::HashMap::new();
        for _ in 0..20 {
            if let Some(server) = load_balancer.select_resource().await {
                *selections.entry(server.clone()).or_insert(0) += 1;
            }
        }

        info!("  Selection distribution: {:?}", selections);
    }

    Ok(())
}

/// Demonstrate concurrent error handling
async fn demo_concurrent_error_handling() -> BlixardResult<()> {
    info!("\n=== Concurrent Error Handling Demo ===");

    // Create mixed success/failure operations
    let operations = (1..=20).map(|i| {
        if i % 4 == 0 {
            format!("failing-operation-{}", i)
        } else {
            format!("success-operation-{}", i)
        }
    }).collect::<Vec<_>>();

    let (successes, errors) = concurrent_try_map(
        operations,
        5, // Max concurrent
        |op| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            
            if op.starts_with("failing") {
                Err(format!("Error in {}", op))
            } else {
                Ok(format!("Completed {}", op))
            }
        },
    ).await;

    info!("Concurrent error handling results:");
    info!("  Successes: {}", successes.len());
    info!("  Errors: {}", errors.len());
    info!("  First few successes: {:?}", &successes[..3.min(successes.len())]);
    info!("  First few errors: {:?}", &errors[..3.min(errors.len())]);

    Ok(())
}

/// Demonstrate streaming processing
async fn demo_streaming_processing() -> BlixardResult<()> {
    info!("\n=== Streaming Processing Demo ===");

    // Create a stream of items
    let stream = futures::stream::iter(1..=100);

    let start = Instant::now();
    let results = stream_process(
        stream,
        8, // Max concurrent processing
        |item| async move {
            // Simulate processing time
            tokio::time::sleep(Duration::from_millis(5)).await;
            item * 2
        },
    ).await;

    info!("Stream processing results:");
    info!("  Processed {} items in {:?}", results.len(), start.elapsed());
    info!("  First 10 results: {:?}", &results[..10]);
    info!("  Last 10 results: {:?}", &results[results.len()-10..]);

    Ok(())
}

/// Simulate a generic connection pool
async fn demo_connection_pooling() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("\n=== Generic Connection Pool Demo ===");

    // Create a connection pool for string "connections"
    let pool = GenericConnectionPool::new(
        5, // Max size
        Duration::from_secs(30), // Idle timeout
        Duration::from_secs(10), // Health check interval
        || async {
            // Connection factory
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(format!("Connection-{}", uuid::Uuid::new_v4()))
        },
        |_conn| async {
            // Health checker
            true
        },
    );

    // Get connections concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let pool_ref = &pool;
        let handle = tokio::spawn(async move {
            match pool_ref.get().await {
                Ok(conn) => {
                    info!("  Got connection {}: {}", i + 1, conn);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    pool_ref.return_resource(conn).await;
                    Ok(())
                }
                Err(e) => {
                    warn!("  Failed to get connection {}: {}", i + 1, e);
                    Err(e)
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        let _ = handle.await?;
    }

    // Cleanup pool
    pool.cleanup().await;
    info!("Connection pool demo completed");

    Ok(())
}