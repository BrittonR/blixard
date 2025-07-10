//! Retry and backoff patterns for handling transient failures
//!
//! This module provides configurable retry mechanisms with various backoff strategies
//! to handle transient failures in distributed systems operations.

use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Backoff strategy for retry operations
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed(Duration),
    /// Linear increase in delay (base * attempt)
    Linear { base: Duration, max: Duration },
    /// Exponential increase in delay (base * 2^attempt)
    Exponential { base: Duration, max: Duration, multiplier: f64 },
    /// Fibonacci sequence delays
    Fibonacci { base: Duration, max: Duration },
    /// Custom backoff function
    Custom(fn(attempt: u32) -> Duration),
}

impl BackoffStrategy {
    /// Calculate the delay for a given attempt
    pub fn delay(&self, attempt: u32) -> Duration {
        match self {
            BackoffStrategy::Fixed(duration) => *duration,
            
            BackoffStrategy::Linear { base, max } => {
                let delay = base.saturating_mul(attempt);
                std::cmp::min(delay, *max)
            }
            
            BackoffStrategy::Exponential { base, max, multiplier } => {
                let factor = multiplier.powf(attempt.saturating_sub(1) as f64);
                let delay_ms = (base.as_millis() as f64 * factor) as u64;
                let delay = Duration::from_millis(delay_ms);
                std::cmp::min(delay, *max)
            }
            
            BackoffStrategy::Fibonacci { base, max } => {
                let fib = fibonacci(attempt);
                let delay = base.saturating_mul(fib);
                std::cmp::min(delay, *max)
            }
            
            BackoffStrategy::Custom(f) => f(attempt),
        }
    }
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            max: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

/// Configuration for retry operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the initial attempt)
    pub max_attempts: u32,
    /// Backoff strategy to use
    pub backoff: BackoffStrategy,
    /// Whether to add jitter to delays (helps avoid thundering herd)
    pub jitter: bool,
    /// Maximum total time to spend retrying
    pub max_total_delay: Option<Duration>,
    /// Function to determine if an error is retryable
    pub is_retryable: fn(&BlixardError) -> bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::default(),
            jitter: true,
            max_total_delay: None,
            is_retryable: default_is_retryable,
        }
    }
}

impl RetryConfig {
    /// Create a simple fixed delay retry config
    pub fn fixed(attempts: u32, delay: Duration) -> Self {
        Self {
            max_attempts: attempts,
            backoff: BackoffStrategy::Fixed(delay),
            ..Default::default()
        }
    }
    
    /// Create an exponential backoff retry config
    pub fn exponential(attempts: u32) -> Self {
        Self {
            max_attempts: attempts,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(100),
                max: Duration::from_secs(30),
                multiplier: 2.0,
            },
            ..Default::default()
        }
    }
}

/// Default function to determine if an error is retryable
fn default_is_retryable(error: &BlixardError) -> bool {
    matches!(
        error,
        BlixardError::NetworkError(_) |
        BlixardError::Timeout { .. } |
        BlixardError::ConnectionError { .. } |
        BlixardError::TemporaryFailure { .. }
    )
}

/// Trait for operations that can be retried
#[async_trait]
pub trait Retryable {
    /// The output type of the operation
    type Output;
    
    /// Execute the operation
    async fn execute(&mut self) -> BlixardResult<Self::Output>;
    
    /// Optional callback when retry occurs
    fn on_retry(&mut self, attempt: u32, error: &BlixardError, delay: Duration) {
        warn!(
            "Retry attempt {} after error: {} (waiting {:?})",
            attempt, error, delay
        );
    }
}

/// Retry an async operation with the given configuration
pub async fn retry<F, T>(config: RetryConfig, mut operation: F) -> BlixardResult<T>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>>,
{
    let mut attempt = 0;
    let mut total_delay = Duration::ZERO;
    let start_time = tokio::time::Instant::now();
    
    loop {
        attempt += 1;
        
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("Operation succeeded after {} attempts", attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                // Check if we should retry
                if attempt >= config.max_attempts {
                    warn!("Max retry attempts ({}) reached", config.max_attempts);
                    return Err(error);
                }
                
                if !(config.is_retryable)(&error) {
                    debug!("Error is not retryable: {}", error);
                    return Err(error);
                }
                
                // Calculate delay
                let mut delay = config.backoff.delay(attempt);
                
                // Add jitter if configured
                if config.jitter {
                    use rand::Rng;
                    let jitter_factor = rand::thread_rng().gen_range(0.5..1.5);
                    delay = Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64);
                }
                
                // Check total delay limit
                total_delay = total_delay.saturating_add(delay);
                if let Some(max_total) = config.max_total_delay {
                    if total_delay > max_total {
                        warn!("Max total retry delay exceeded");
                        return Err(error);
                    }
                }
                
                warn!(
                    "Retry attempt {}/{} after error: {} (waiting {:?})",
                    attempt, config.max_attempts, error, delay
                );
                
                sleep(delay).await;
            }
        }
    }
}

/// Retry builder for more complex retry scenarios
pub struct RetryBuilder<T> {
    config: RetryConfig,
    operation: Option<Box<dyn FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>> + Send>>,
}

impl<T> RetryBuilder<T> {
    /// Create a new retry builder
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
            operation: None,
        }
    }
    
    /// Set the maximum number of attempts
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.config.max_attempts = attempts;
        self
    }
    
    /// Set the backoff strategy
    pub fn backoff(mut self, strategy: BackoffStrategy) -> Self {
        self.config.backoff = strategy;
        self
    }
    
    /// Enable or disable jitter
    pub fn jitter(mut self, enabled: bool) -> Self {
        self.config.jitter = enabled;
        self
    }
    
    /// Set the maximum total delay
    pub fn max_total_delay(mut self, duration: Duration) -> Self {
        self.config.max_total_delay = Some(duration);
        self
    }
    
    /// Set the retryable error checker
    pub fn is_retryable(mut self, f: fn(&BlixardError) -> bool) -> Self {
        self.config.is_retryable = f;
        self
    }
    
    /// Set the operation to retry
    pub fn operation<F>(mut self, f: F) -> Self
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>> + Send + 'static,
    {
        self.operation = Some(Box::new(f));
        self
    }
    
    /// Execute the retry operation
    pub async fn execute(mut self) -> BlixardResult<T> {
        let operation = self.operation.take()
            .ok_or_else(|| BlixardError::Internal {
                message: "No operation provided to retry".to_string(),
            })?;
            
        retry(self.config, operation).await
    }
}

/// Helper function to calculate Fibonacci number
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a = 0;
            let mut b = 1;
            for _ in 2..=n {
                let temp = a + b;
                a = b;
                b = temp;
            }
            b
        }
    }
}

/// Convenience functions for common retry patterns
pub mod presets {
    use super::*;
    
    /// Quick retry for network operations
    pub async fn retry_network<F, T>(operation: F) -> BlixardResult<T>
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>>,
    {
        let config = RetryConfig {
            max_attempts: 5,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(100),
                max: Duration::from_secs(10),
                multiplier: 2.0,
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(30)),
            is_retryable: |e| matches!(
                e,
                BlixardError::NetworkError(_) |
                BlixardError::ConnectionError { .. } |
                BlixardError::Timeout { .. }
            ),
        };
        
        retry(config, operation).await
    }
    
    /// Quick retry for database operations
    pub async fn retry_database<F, T>(operation: F) -> BlixardResult<T>
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>>,
    {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(100)),
            jitter: false,
            max_total_delay: Some(Duration::from_secs(5)),
            is_retryable: |e| matches!(
                e,
                BlixardError::DatabaseError { .. } |
                BlixardError::TemporaryFailure { .. }
            ),
        };
        
        retry(config, operation).await
    }
    
    /// Aggressive retry for critical operations
    pub async fn retry_critical<F, T>(operation: F) -> BlixardResult<T>
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = BlixardResult<T>> + Send>>,
    {
        let config = RetryConfig {
            max_attempts: 10,
            backoff: BackoffStrategy::Fibonacci {
                base: Duration::from_millis(100),
                max: Duration::from_secs(60),
            },
            jitter: true,
            max_total_delay: Some(Duration::from_secs(300)), // 5 minutes
            is_retryable: |e| !matches!(
                e,
                BlixardError::InvalidInput { .. } |
                BlixardError::NotImplemented { .. } |
                BlixardError::ConfigError(_)
            ),
        };
        
        retry(config, operation).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_successful_on_first_attempt() {
        let config = RetryConfig::fixed(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(config, move || {
            let count = counter_clone.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(count)
            })
        }).await.unwrap();
        
        assert_eq!(result, 0);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
    
    #[tokio::test]
    async fn test_retry_on_failure() {
        let config = RetryConfig::fixed(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(config, move || {
            let count = counter_clone.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if count < 2 {
                    Err(BlixardError::NetworkError("Temporary failure".to_string()))
                } else {
                    Ok(count)
                }
            })
        }).await.unwrap();
        
        assert_eq!(result, 2);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
    
    #[tokio::test]
    async fn test_max_attempts_exceeded() {
        let config = RetryConfig::fixed(2, Duration::from_millis(10));
        
        let result = retry(config, move || {
            Box::pin(async move {
                Err::<(), _>(BlixardError::NetworkError("Always fails".to_string()))
            })
        }).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_non_retryable_error() {
        let config = RetryConfig::fixed(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(config, move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Err::<(), _>(BlixardError::InvalidInput {
                    field: "test".to_string(),
                    value: "value".to_string(),
                    reason: "Not retryable".to_string(),
                })
            })
        }).await;
        
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Only one attempt
    }
    
    #[tokio::test]
    async fn test_exponential_backoff() {
        let start = tokio::time::Instant::now();
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_millis(10),
                max: Duration::from_secs(1),
                multiplier: 2.0,
            },
            jitter: false,
            ..Default::default()
        };
        
        let _ = retry(config, move || {
            Box::pin(async move {
                Err::<(), _>(BlixardError::NetworkError("Always fails".to_string()))
            })
        }).await;
        
        let elapsed = start.elapsed();
        // Should have delays of 10ms and 20ms = 30ms total (plus some overhead)
        assert!(elapsed >= Duration::from_millis(30));
        assert!(elapsed < Duration::from_millis(100));
    }
    
    #[tokio::test]
    async fn test_retry_builder() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = RetryBuilder::new()
            .max_attempts(5)
            .backoff(BackoffStrategy::Fixed(Duration::from_millis(5)))
            .jitter(false)
            .operation(move || {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    if count < 3 {
                        Err(BlixardError::TemporaryFailure {
                            details: "Not ready yet".to_string(),
                        })
                    } else {
                        Ok("Success")
                    }
                })
            })
            .execute()
            .await
            .unwrap();
        
        assert_eq!(result, "Success");
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }
}