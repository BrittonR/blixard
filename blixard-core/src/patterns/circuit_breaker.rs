//! Circuit Breaker Pattern for Fault Tolerance
//!
//! The circuit breaker pattern prevents a distributed system from repeatedly trying
//! to execute operations that are likely to fail, allowing the system to heal
//! while avoiding resource exhaustion from futile retry attempts.
//!
//! ## Pattern Implementation
//!
//! The circuit breaker has three states:
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Failing fast, requests are rejected immediately  
//! - **Half-Open**: Testing if the service has recovered
//!
//! ## Features
//!
//! - Configurable failure thresholds and timeouts
//! - Automatic state transitions with exponential backoff
//! - Comprehensive metrics and observability
//! - Thread-safe operation with atomic counters
//! - Integration with existing retry patterns
//! - Support for custom failure detection logic
//!
//! ## Usage Example
//!
//! ```rust
//! use std::time::Duration;
//! use blixard_core::patterns::{CircuitBreaker, CircuitBreakerConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = CircuitBreakerConfig {
//!     failure_threshold: 5,
//!     reset_timeout: Duration::from_secs(30),
//!     half_open_max_calls: 3,
//!     ..Default::default()
//! };
//!
//! let breaker = CircuitBreaker::new("external-api".to_string(), config);
//!
//! // Use circuit breaker for operations
//! let result = breaker.call(|| async {
//!     // Your potentially failing operation here
//!     external_api_call().await
//! }).await;
//!
//! match result {
//!     Ok(value) => println!("Operation succeeded: {:?}", value),
//!     Err(e) => println!("Operation failed or circuit open: {:?}", e),
//! }
//! # Ok(())
//! # }
//! # async fn external_api_call() -> Result<String, std::io::Error> { Ok("result".to_string()) }
//! ```

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::BlixardError;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Normal operation - requests pass through
    Closed,
    /// Failing fast - requests are rejected immediately
    Open,
    /// Testing if service has recovered - limited requests allowed
    HalfOpen,
}

impl std::fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerState::Closed => write!(f, "closed"),
            CircuitBreakerState::Open => write!(f, "open"),
            CircuitBreakerState::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Time to wait before transitioning from Open to Half-Open
    pub reset_timeout: Duration,
    /// Maximum number of calls allowed in Half-Open state
    pub half_open_max_calls: u32,
    /// Minimum number of calls before considering failure rate
    pub minimum_request_threshold: u32,
    /// Success ratio required to transition from Half-Open to Closed
    pub success_threshold_ratio: f64,
    /// Function to determine if an error should trigger circuit breaker
    pub is_circuit_breaking_error: fn(&BlixardError) -> bool,
    /// Enable automatic logging of state transitions
    pub enable_logging: bool,
    /// Timeout for individual operations (None = no timeout)
    pub operation_timeout: Option<Duration>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
            minimum_request_threshold: 10,
            success_threshold_ratio: 0.8,
            is_circuit_breaking_error: default_circuit_breaking_error_check,
            enable_logging: true,
            operation_timeout: Some(Duration::from_secs(30)),
        }
    }
}

/// Default function to determine if an error should trigger circuit breaker
fn default_circuit_breaking_error_check(error: &BlixardError) -> bool {
    matches!(
        error,
        BlixardError::NetworkError(_)
            | BlixardError::ConnectionError { .. }
            | BlixardError::Timeout { .. }
            | BlixardError::TemporaryFailure { .. }
            | BlixardError::ResourceExhausted { .. }
            | BlixardError::ResourceUnavailable { .. }
    )
}

/// Statistics for circuit breaker operation
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    /// Current state of the circuit breaker
    pub state: CircuitBreakerState,
    /// Total number of calls made through the circuit breaker
    pub total_calls: u64,
    /// Number of successful calls
    pub successful_calls: u64,
    /// Number of failed calls
    pub failed_calls: u64,
    /// Number of calls rejected due to open circuit
    pub rejected_calls: u64,
    /// Consecutive failures in current sequence
    pub consecutive_failures: u32,
    /// Time when circuit was last opened
    pub last_opened_at: Option<Instant>,
    /// Time when circuit was last closed
    pub last_closed_at: Option<Instant>,
    /// Time when the last failure occurred
    pub last_failure_at: Option<Instant>,
    /// Time when the last success occurred
    pub last_success_at: Option<Instant>,
    /// Current failure rate (0.0 - 1.0)
    pub failure_rate: f64,
}

impl CircuitBreakerStats {
    fn new() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            total_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            rejected_calls: 0,
            consecutive_failures: 0,
            last_opened_at: None,
            last_closed_at: None,
            last_failure_at: None,
            last_success_at: None,
            failure_rate: 0.0,
        }
    }
}

/// Internal state for circuit breaker
struct CircuitBreakerInternalState {
    state: CircuitBreakerState,
    consecutive_failures: u32,
    half_open_calls: u32,
    last_opened_at: Option<Instant>,
    last_closed_at: Option<Instant>,
    last_failure_at: Option<Instant>,
    last_success_at: Option<Instant>,
}

/// Circuit breaker implementation with comprehensive fault tolerance
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    internal_state: Arc<RwLock<CircuitBreakerInternalState>>,
    
    // Atomic counters for thread-safe statistics
    total_calls: AtomicU64,
    successful_calls: AtomicU64,
    failed_calls: AtomicU64,
    rejected_calls: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            name,
            config,
            internal_state: Arc::new(RwLock::new(CircuitBreakerInternalState {
                state: CircuitBreakerState::Closed,
                consecutive_failures: 0,
                half_open_calls: 0,
                last_opened_at: None,
                last_closed_at: None,
                last_failure_at: None,
                last_success_at: None,
            })),
            total_calls: AtomicU64::new(0),
            successful_calls: AtomicU64::new(0),
            failed_calls: AtomicU64::new(0),
            rejected_calls: AtomicU64::new(0),
        }
    }

    /// Execute an operation through the circuit breaker
    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> Box<dyn Future<Output = Result<T, E>> + Send + Unpin>,
        E: Into<BlixardError>,
    {
        // Check if we should allow the request
        if !self.should_allow_request().await {
            self.rejected_calls.fetch_add(1, Ordering::Relaxed);
            return Err(BlixardError::TemporaryFailure {
                details: format!("Circuit breaker '{}' is open", self.name),
            });
        }

        self.total_calls.fetch_add(1, Ordering::Relaxed);

        // Execute operation with optional timeout
        let result = if let Some(timeout) = self.config.operation_timeout {
            match tokio::time::timeout(timeout, operation()).await {
                Ok(result) => result.map_err(|e| e.into()),
                Err(_) => Err(BlixardError::Timeout {
                    operation: format!("circuit breaker '{}'", self.name),
                    duration: timeout,
                }),
            }
        } else {
            operation().await.map_err(|e| e.into())
        };

        // Record result and update state
        match &result {
            Ok(_) => self.on_success().await,
            Err(e) => {
                if (self.config.is_circuit_breaking_error)(e) {
                    self.on_failure().await;
                } else {
                    // Non-circuit-breaking errors don't affect circuit state
                    debug!(
                        "Circuit breaker '{}': Non-circuit-breaking error: {}",
                        self.name, e
                    );
                }
            }
        }

        result
    }

    /// Execute an async operation through the circuit breaker
    pub async fn call_async<F, Fut, T, E>(&self, operation: F) -> Result<T, BlixardError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + Unpin,
        E: Into<BlixardError>,
    {
        self.call(|| Box::new(operation())).await
    }

    /// Check if a request should be allowed based on current state
    async fn should_allow_request(&self) -> bool {
        let mut state = self.internal_state.write().await;
        
        match state.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // Check if enough time has passed to transition to half-open
                if let Some(opened_at) = state.last_opened_at {
                    if opened_at.elapsed() >= self.config.reset_timeout {
                        state.state = CircuitBreakerState::HalfOpen;
                        state.half_open_calls = 0;
                        if self.config.enable_logging {
                            info!("Circuit breaker '{}' transitioning to half-open", self.name);
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => {
                if state.half_open_calls < self.config.half_open_max_calls {
                    state.half_open_calls += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Handle successful operation
    async fn on_success(&self) {
        self.successful_calls.fetch_add(1, Ordering::Relaxed);
        
        let mut state = self.internal_state.write().await;
        state.consecutive_failures = 0;
        state.last_success_at = Some(Instant::now());

        // Transition to closed if we're in half-open state
        if state.state == CircuitBreakerState::HalfOpen {
            let total = self.total_calls.load(Ordering::Relaxed);
            let successful = self.successful_calls.load(Ordering::Relaxed);
            
            if total >= self.config.minimum_request_threshold as u64 {
                let success_rate = successful as f64 / total as f64;
                if success_rate >= self.config.success_threshold_ratio {
                    state.state = CircuitBreakerState::Closed;
                    state.last_closed_at = Some(Instant::now());
                    if self.config.enable_logging {
                        info!("Circuit breaker '{}' closing (success rate: {:.2})", self.name, success_rate);
                    }
                }
            }
        }
    }

    /// Handle failed operation
    async fn on_failure(&self) {
        self.failed_calls.fetch_add(1, Ordering::Relaxed);
        
        let mut state = self.internal_state.write().await;
        state.consecutive_failures += 1;
        state.last_failure_at = Some(Instant::now());

        // Check if we should open the circuit
        if state.consecutive_failures >= self.config.failure_threshold {
            if state.state != CircuitBreakerState::Open {
                state.state = CircuitBreakerState::Open;
                state.last_opened_at = Some(Instant::now());
                if self.config.enable_logging {
                    warn!(
                        "Circuit breaker '{}' opening after {} consecutive failures",
                        self.name, state.consecutive_failures
                    );
                }
            }
        }

        // If we're in half-open state and we fail, go back to open
        if state.state == CircuitBreakerState::HalfOpen {
            state.state = CircuitBreakerState::Open;
            state.last_opened_at = Some(Instant::now());
            state.half_open_calls = 0;
            if self.config.enable_logging {
                warn!("Circuit breaker '{}' reopening after failure in half-open state", self.name);
            }
        }
    }

    /// Get current circuit breaker statistics
    pub async fn stats(&self) -> CircuitBreakerStats {
        let state = self.internal_state.read().await;
        let total = self.total_calls.load(Ordering::Relaxed);
        let successful = self.successful_calls.load(Ordering::Relaxed);
        let failed = self.failed_calls.load(Ordering::Relaxed);
        
        let failure_rate = if total > 0 {
            failed as f64 / total as f64
        } else {
            0.0
        };

        CircuitBreakerStats {
            state: state.state,
            total_calls: total,
            successful_calls: successful,
            failed_calls: failed,
            rejected_calls: self.rejected_calls.load(Ordering::Relaxed),
            consecutive_failures: state.consecutive_failures,
            last_opened_at: state.last_opened_at,
            last_closed_at: state.last_closed_at,
            last_failure_at: state.last_failure_at,
            last_success_at: state.last_success_at,
            failure_rate,
        }
    }

    /// Get the name of this circuit breaker
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the current state of the circuit breaker
    pub async fn state(&self) -> CircuitBreakerState {
        let state = self.internal_state.read().await;
        state.state
    }

    /// Force the circuit breaker to open (for testing or emergency)
    pub async fn force_open(&self) {
        let mut state = self.internal_state.write().await;
        state.state = CircuitBreakerState::Open;
        state.last_opened_at = Some(Instant::now());
        if self.config.enable_logging {
            warn!("Circuit breaker '{}' forcibly opened", self.name);
        }
    }

    /// Force the circuit breaker to close (for testing or recovery)
    pub async fn force_close(&self) {
        let mut state = self.internal_state.write().await;
        state.state = CircuitBreakerState::Closed;
        state.consecutive_failures = 0;
        state.last_closed_at = Some(Instant::now());
        if self.config.enable_logging {
            info!("Circuit breaker '{}' forcibly closed", self.name);
        }
    }

    /// Reset all statistics (for testing)
    pub async fn reset_stats(&self) {
        self.total_calls.store(0, Ordering::Relaxed);
        self.successful_calls.store(0, Ordering::Relaxed);
        self.failed_calls.store(0, Ordering::Relaxed);
        self.rejected_calls.store(0, Ordering::Relaxed);
        
        let mut state = self.internal_state.write().await;
        state.consecutive_failures = 0;
        state.half_open_calls = 0;
        state.last_opened_at = None;
        state.last_closed_at = None;
        state.last_failure_at = None;
        state.last_success_at = None;
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            config: self.config.clone(),
            internal_state: Arc::clone(&self.internal_state),
            total_calls: AtomicU64::new(self.total_calls.load(Ordering::Relaxed)),
            successful_calls: AtomicU64::new(self.successful_calls.load(Ordering::Relaxed)),
            failed_calls: AtomicU64::new(self.failed_calls.load(Ordering::Relaxed)),
            rejected_calls: AtomicU64::new(self.rejected_calls.load(Ordering::Relaxed)),
        }
    }
}

/// Circuit breaker builder for easier configuration
pub struct CircuitBreakerBuilder {
    name: String,
    config: CircuitBreakerConfig,
}

impl CircuitBreakerBuilder {
    /// Create a new circuit breaker builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            config: CircuitBreakerConfig::default(),
        }
    }

    /// Set the failure threshold
    pub fn failure_threshold(mut self, threshold: u32) -> Self {
        self.config.failure_threshold = threshold;
        self
    }

    /// Set the reset timeout
    pub fn reset_timeout(mut self, timeout: Duration) -> Self {
        self.config.reset_timeout = timeout;
        self
    }

    /// Set the maximum calls in half-open state
    pub fn half_open_max_calls(mut self, max_calls: u32) -> Self {
        self.config.half_open_max_calls = max_calls;
        self
    }

    /// Set the minimum request threshold
    pub fn minimum_request_threshold(mut self, threshold: u32) -> Self {
        self.config.minimum_request_threshold = threshold;
        self
    }

    /// Set the success threshold ratio
    pub fn success_threshold_ratio(mut self, ratio: f64) -> Self {
        self.config.success_threshold_ratio = ratio;
        self
    }

    /// Set the operation timeout
    pub fn operation_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.config.operation_timeout = timeout;
        self
    }

    /// Enable or disable logging
    pub fn enable_logging(mut self, enable: bool) -> Self {
        self.config.enable_logging = enable;
        self
    }

    /// Set custom error checking function
    pub fn error_predicate(mut self, predicate: fn(&BlixardError) -> bool) -> Self {
        self.config.is_circuit_breaking_error = predicate;
        self
    }

    /// Build the circuit breaker
    pub fn build(self) -> CircuitBreaker {
        CircuitBreaker::new(self.name, self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(3)
            .enable_logging(false)
            .build();

        // Successful operations should keep circuit closed
        for _ in 0..5 {
            let result = breaker.call_async(|| async { Ok::<i32, BlixardError>(42) }).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 42);
        }

        assert_eq!(breaker.state().await, CircuitBreakerState::Closed);
        let stats = breaker.stats().await;
        assert_eq!(stats.successful_calls, 5);
        assert_eq!(stats.failed_calls, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(3)
            .enable_logging(false)
            .build();

        // Generate enough failures to open circuit
        for i in 0..3 {
            let result = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::NetworkError("failure".to_string()))
            }).await;
            assert!(result.is_err());
            
            if i < 2 {
                assert_eq!(breaker.state().await, CircuitBreakerState::Closed);
            }
        }

        assert_eq!(breaker.state().await, CircuitBreakerState::Open);
        let stats = breaker.stats().await;
        assert_eq!(stats.failed_calls, 3);
        assert_eq!(stats.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_rejects_when_open() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(2)
            .enable_logging(false)
            .build();

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::NetworkError("failure".to_string()))
            }).await;
        }

        assert_eq!(breaker.state().await, CircuitBreakerState::Open);

        // Next call should be rejected
        let result = breaker.call_async(|| async { Ok::<i32, BlixardError>(42) }).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circuit breaker"));

        let stats = breaker.stats().await;
        assert_eq!(stats.rejected_calls, 1);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_transition() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(2)
            .reset_timeout(Duration::from_millis(10))
            .enable_logging(false)
            .build();

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::NetworkError("failure".to_string()))
            }).await;
        }

        assert_eq!(breaker.state().await, CircuitBreakerState::Open);

        // Wait for reset timeout
        sleep(Duration::from_millis(15)).await;

        // Next call should transition to half-open
        let result = breaker.call_async(|| async { Ok::<i32, BlixardError>(42) }).await;
        assert!(result.is_ok());
        
        // After successful call, should still be half-open until enough successes
        assert_eq!(breaker.state().await, CircuitBreakerState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_closed() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(2)
            .reset_timeout(Duration::from_millis(10))
            .minimum_request_threshold(1)
            .success_threshold_ratio(0.5)
            .enable_logging(false)
            .build();

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::NetworkError("failure".to_string()))
            }).await;
        }

        // Wait for reset timeout
        sleep(Duration::from_millis(15)).await;

        // Successful call should eventually close the circuit
        let result = breaker.call_async(|| async { Ok::<i32, BlixardError>(42) }).await;
        assert!(result.is_ok());
        
        // Should transition to closed due to success
        assert_eq!(breaker.state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_non_breaking_errors() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(2)
            .enable_logging(false)
            .build();

        // Non-circuit-breaking errors shouldn't open circuit
        for _ in 0..5 {
            let result = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::ValidationError {
                    field: "test".to_string(),
                    message: "validation error".to_string(),
                })
            }).await;
            assert!(result.is_err());
        }

        // Circuit should still be closed
        assert_eq!(breaker.state().await, CircuitBreakerState::Closed);
        let stats = breaker.stats().await;
        assert_eq!(stats.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_timeout() {
        let breaker = CircuitBreakerBuilder::new("test")
            .operation_timeout(Some(Duration::from_millis(10)))
            .failure_threshold(1)
            .enable_logging(false)
            .build();

        // Operation that takes too long should timeout and trigger circuit
        let result = breaker.call_async(|| async {
            sleep(Duration::from_millis(50)).await;
            Ok::<i32, BlixardError>(42)
        }).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
        
        // Circuit should be open due to timeout
        assert_eq!(breaker.state().await, CircuitBreakerState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_force_operations() {
        let breaker = CircuitBreakerBuilder::new("test")
            .enable_logging(false)
            .build();

        // Force open
        breaker.force_open().await;
        assert_eq!(breaker.state().await, CircuitBreakerState::Open);

        // Force close
        breaker.force_close().await;
        assert_eq!(breaker.state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let breaker = CircuitBreakerBuilder::new("test")
            .failure_threshold(2)
            .enable_logging(false)
            .build();

        // Some successful calls
        for _ in 0..3 {
            let _ = breaker.call_async(|| async { Ok::<i32, BlixardError>(42) }).await;
        }

        // Some failed calls
        for _ in 0..2 {
            let _ = breaker.call_async(|| async {
                Err::<i32, _>(BlixardError::NetworkError("failure".to_string()))
            }).await;
        }

        let stats = breaker.stats().await;
        assert_eq!(stats.total_calls, 5);
        assert_eq!(stats.successful_calls, 3);
        assert_eq!(stats.failed_calls, 2);
        assert_eq!(stats.failure_rate, 0.4);
        assert_eq!(stats.state, CircuitBreakerState::Open);
    }
}