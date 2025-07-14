//! Example showing how to migrate existing retry code to use the standardized retry pattern
//!
//! This demonstrates replacing manual retry loops with the retry pattern.

use crate::error::{BlixardError, BlixardResult};
use crate::patterns::retry::{retry, BackoffStrategy, RetryBuilder, RetryConfig};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Example: Original manual retry code (what we're replacing)
pub async fn old_connect_with_retry(address: &str) -> BlixardResult<String> {
    let max_attempts = 5;
    let mut attempt = 0;
    let mut delay = Duration::from_millis(100);

    loop {
        attempt += 1;

        match fake_connect(address).await {
            Ok(connection) => return Ok(connection),
            Err(e) => {
                if attempt >= max_attempts {
                    return Err(e);
                }

                // Manual exponential backoff
                tokio::time::sleep(delay).await;
                delay = delay * 2;

                if delay > Duration::from_secs(10) {
                    delay = Duration::from_secs(10);
                }
            }
        }
    }
}

/// Example: New code using retry pattern (simple version)
pub async fn new_connect_with_retry_simple(address: &str) -> BlixardResult<String> {
    let config = RetryConfig::exponential(5);
    let addr = address.to_string();

    retry(config, move || {
        let addr = addr.clone();
        Box::pin(async move { fake_connect(&addr).await })
    })
    .await
}

/// Example: New code using retry pattern (with custom configuration)
pub async fn new_connect_with_retry_custom(address: &str) -> BlixardResult<String> {
    let config = RetryConfig {
        max_attempts: 5,
        backoff: BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            max: Duration::from_secs(10),
            multiplier: 2.0,
        },
        jitter: true,
        jitter_strategy: crate::patterns::retry::JitterStrategy::Proportional,
        max_total_delay: Some(Duration::from_secs(30)),
        is_retryable: |e| matches!(e, BlixardError::ConnectionError { .. }),
        operation_name: Some("connect_with_retry".to_string()),
        enable_logging: true,
    };

    let addr = address.to_string();
    retry(config, move || {
        let addr = addr.clone();
        Box::pin(async move { fake_connect(&addr).await })
    })
    .await
}

/// Example: Using RetryBuilder for complex scenarios
pub async fn new_connect_with_retry_builder(address: &str) -> BlixardResult<String> {
    let addr = address.to_string();

    RetryBuilder::new()
        .max_attempts(5)
        .backoff(BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            max: Duration::from_secs(10),
            multiplier: 2.0,
        })
        .jitter(true)
        .is_retryable(|e| !matches!(e, BlixardError::ConfigurationError { .. }))
        .operation(move || {
            let addr = addr.clone();
            Box::pin(async move {
                info!("Attempting connection to {}", addr);
                fake_connect(&addr).await
            })
        })
        .execute()
        .await
}

/// Example: Using preset configurations
pub async fn new_connect_with_preset(address: &str) -> BlixardResult<String> {
    use crate::patterns::retry::presets::retry_network;

    let addr = address.to_string();
    retry_network(move || {
        let addr = addr.clone();
        Box::pin(async move { fake_connect(&addr).await })
    })
    .await
}

/// Example: Retrying with state/context
pub struct ConnectionManager {
    address: String,
    connection_count: u32,
}

impl ConnectionManager {
    pub fn new(address: String) -> Self {
        Self {
            address,
            connection_count: 0,
        }
    }

    pub async fn connect_with_retry(&mut self) -> BlixardResult<String> {
        let config = RetryConfig::exponential(3);

        // Use Arc<Mutex> if you need to share state across threads
        let addr = self.address.clone();
        let result = retry(config, move || {
            let addr = addr.clone();
            Box::pin(async move { fake_connect(&addr).await })
        })
        .await?;

        self.connection_count += 1;
        Ok(result)
    }
}

/// Example: Batch operations with retry
pub async fn batch_operations_with_retry(items: Vec<String>) -> BlixardResult<Vec<String>> {
    use futures::future::try_join_all;

    let futures = items.into_iter().map(|item| async move {
        let config = RetryConfig::fixed(3, Duration::from_millis(100));
        retry(config, move || {
            let item = item.clone();
            Box::pin(async move { process_item(&item).await })
        })
        .await
    });

    try_join_all(futures).await
}

/// Example: Circuit breaker pattern with retry
pub struct CircuitBreaker {
    failure_count: std::sync::atomic::AtomicU32,
    last_failure: std::sync::Mutex<Option<tokio::time::Instant>>,
    threshold: u32,
    reset_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_count: std::sync::atomic::AtomicU32::new(0),
            last_failure: std::sync::Mutex::new(None),
            threshold,
            reset_timeout,
        }
    }

    pub async fn call_with_retry<F, T>(&self, operation: F) -> BlixardResult<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<T>> + Send>>,
    {
        // Check if circuit is open
        if let Some(last_failure) = *self.last_failure.lock().unwrap() {
            if last_failure.elapsed() < self.reset_timeout {
                let failures = self.failure_count.load(std::sync::atomic::Ordering::SeqCst);
                if failures >= self.threshold {
                    return Err(BlixardError::TemporaryFailure {
                        details: "Circuit breaker is open".to_string(),
                    });
                }
            } else {
                // Reset circuit after timeout
                self.failure_count
                    .store(0, std::sync::atomic::Ordering::SeqCst);
                *self.last_failure.lock().unwrap() = None;
            }
        }

        let config = RetryConfig::exponential(3);
        match retry(config, operation).await {
            Ok(result) => {
                // Reset failure count on success
                self.failure_count
                    .store(0, std::sync::atomic::Ordering::SeqCst);
                Ok(result)
            }
            Err(e) => {
                // Increment failure count
                self.failure_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                *self.last_failure.lock().unwrap() = Some(tokio::time::Instant::now());
                Err(e)
            }
        }
    }
}

// Helper functions for examples
async fn fake_connect(address: &str) -> BlixardResult<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Simulate 70% failure rate
    if rng.gen_bool(0.7) {
        Err(BlixardError::ConnectionError {
            address: address.to_string(),
            details: "Connection refused".to_string(),
        })
    } else {
        Ok(format!("Connected to {}", address))
    }
}

async fn process_item(item: &str) -> BlixardResult<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Simulate 30% failure rate
    if rng.gen_bool(0.3) {
        Err(BlixardError::TemporaryFailure {
            details: format!("Failed to process {}", item),
        })
    } else {
        Ok(format!("Processed: {}", item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_migration_examples() {
        // These might fail due to randomness, but demonstrate the API
        let _ = new_connect_with_retry_simple("localhost:8080").await;
        let _ = new_connect_with_retry_custom("localhost:8080").await;
        let _ = new_connect_with_retry_builder("localhost:8080").await;
        let _ = new_connect_with_preset("localhost:8080").await;
    }

    #[tokio::test]
    async fn test_connection_manager() {
        let mut manager = ConnectionManager::new("localhost:8080".to_string());
        let _ = manager.connect_with_retry().await;
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let items = vec!["item1".to_string(), "item2".to_string()];
        let _ = batch_operations_with_retry(items).await;
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = Arc::new(CircuitBreaker::new(3, Duration::from_secs(10)));

        // Try multiple times to potentially trigger circuit breaker
        for _ in 0..5 {
            let _ = breaker
                .call_with_retry(|| Box::pin(async { fake_connect("localhost:8080").await }))
                .await;
        }
    }
}
