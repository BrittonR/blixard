//! Time abstractions for testability
//!
//! This module provides trait-based abstractions for time operations,
//! enabling deterministic testing of time-dependent code.

use async_trait::async_trait;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Instant in time (monotonic clock)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(u64); // Microseconds since some epoch

impl Instant {
    /// Create from microseconds
    pub fn from_micros(micros: u64) -> Self {
        Self(micros)
    }
    
    /// Get microseconds value
    pub fn as_micros(&self) -> u64 {
        self.0
    }
    
    /// Duration since another instant
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        let micros = self.0.saturating_sub(earlier.0);
        Duration::from_micros(micros)
    }
    
    /// Time elapsed since this instant
    pub fn elapsed(&self, now: Instant) -> Duration {
        now.duration_since(*self)
    }
}

/// Abstraction for time operations
#[async_trait]
pub trait Clock: Send + Sync {
    /// Get current instant (monotonic)
    fn now(&self) -> Instant;
    
    /// Get current system time
    fn system_time(&self) -> SystemTime;
    
    /// Sleep for a duration
    async fn sleep(&self, duration: Duration);
}

/// Extension trait for timeout operations
/// This is separate from Clock to maintain dyn compatibility
#[async_trait]
pub trait ClockExt: Clock {
    /// Create a timeout future
    async fn timeout<F, T>(&self, duration: Duration, future: F) -> Result<T, ()>
    where
        F: std::future::Future<Output = T> + Send,
        T: Send;
}

// Blanket implementation for all Clock types
#[async_trait]
impl<C: Clock + ?Sized> ClockExt for C {
    async fn timeout<F, T>(&self, duration: Duration, future: F) -> Result<T, ()>
    where
        F: std::future::Future<Output = T> + Send,
        T: Send,
    {
        let sleep_future = self.sleep(duration);
        tokio::pin!(sleep_future);
        tokio::pin!(future);
        
        tokio::select! {
            result = future => Ok(result),
            _ = sleep_future => Err(()),
        }
    }
}

// Production implementation using std/tokio time

/// Production clock using system time
pub struct SystemClock;

impl SystemClock {
    /// Create new instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        let now = std::time::Instant::now();
        // Convert to microseconds since some epoch
        // This is not perfect but works for relative time measurements
        let micros = now.elapsed().as_micros() as u64;
        Instant::from_micros(micros)
    }
    
    fn system_time(&self) -> SystemTime {
        SystemTime::now()
    }
    
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

// Mock implementation for testing

use std::sync::Arc;
use tokio::sync::{RwLock, Notify};

/// Mock clock for deterministic testing
pub struct MockClock {
    /// Current time in microseconds
    current_micros: Arc<RwLock<u64>>,
    /// Notify when time advances
    time_advanced: Arc<Notify>,
}

impl MockClock {
    /// Create new mock clock starting at zero
    pub fn new() -> Self {
        Self {
            current_micros: Arc::new(RwLock::new(0)),
            time_advanced: Arc::new(Notify::new()),
        }
    }
    
    /// Create with specific starting time
    pub fn with_time(micros: u64) -> Self {
        Self {
            current_micros: Arc::new(RwLock::new(micros)),
            time_advanced: Arc::new(Notify::new()),
        }
    }
    
    /// Advance time by duration
    pub async fn advance(&self, duration: Duration) {
        let micros = duration.as_micros() as u64;
        *self.current_micros.write().await += micros;
        self.time_advanced.notify_waiters();
    }
    
    /// Set absolute time
    pub async fn set_time(&self, micros: u64) {
        *self.current_micros.write().await = micros;
        self.time_advanced.notify_waiters();
    }
    
    /// Get current mock time
    pub async fn current_time(&self) -> u64 {
        *self.current_micros.read().await
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Clock for MockClock {
    fn now(&self) -> Instant {
        // This is not async-safe but is the best we can do with the trait
        let micros = futures::executor::block_on(async {
            *self.current_micros.read().await
        });
        Instant::from_micros(micros)
    }
    
    fn system_time(&self) -> SystemTime {
        let micros = futures::executor::block_on(async {
            *self.current_micros.read().await
        });
        UNIX_EPOCH + Duration::from_micros(micros)
    }
    
    async fn sleep(&self, duration: Duration) {
        let target_micros = {
            let current = *self.current_micros.read().await;
            current + duration.as_micros() as u64
        };
        
        // Wait until time advances past target
        loop {
            let current = *self.current_micros.read().await;
            if current >= target_micros {
                break;
            }
            
            // Wait for time to advance
            let notified = self.time_advanced.notified();
            tokio::pin!(notified);
            
            // Check again in case time advanced while we were setting up
            let current = *self.current_micros.read().await;
            if current >= target_micros {
                break;
            }
            
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::ClockExt;
    
    #[tokio::test]
    async fn test_mock_clock_sleep() {
        let clock = MockClock::new();
        
        // Start sleep in background
        let clock_clone = clock.clone();
        let sleep_handle = tokio::spawn(async move {
            clock_clone.sleep(Duration::from_secs(5)).await;
        });
        
        // Verify sleep doesn't complete immediately
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!sleep_handle.is_finished());
        
        // Advance time
        clock.advance(Duration::from_secs(5)).await;
        
        // Sleep should now complete
        sleep_handle.await.unwrap();
    }
    
    #[tokio::test]
    async fn test_mock_clock_timeout() {
        let clock = MockClock::new();
        
        // Test timeout succeeds
        let result = clock.timeout(
            Duration::from_secs(5),
            async { 42 }
        ).await;
        assert_eq!(result, Ok(42));
        
        // Test timeout fails
        let clock_clone = clock.clone();
        let timeout_future = clock_clone.timeout(
            Duration::from_secs(5),
            std::future::pending::<()>()
        );
        
        let handle = tokio::spawn(timeout_future);
        
        // Advance time
        clock.advance(Duration::from_secs(6)).await;
        
        // Timeout should fail
        let result = handle.await.unwrap();
        assert_eq!(result, Err(()));
    }
}