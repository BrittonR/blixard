//! Async utilities for optimized async patterns
//!
//! This module provides reusable utilities for common async patterns
//! found throughout the Blixard codebase, helping to optimize
//! performance and reduce lock contention.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;

/// Async-optimized lock guard for quick read operations
/// 
/// This helper performs a quick read operation that clones the data
/// and immediately releases the lock, reducing contention.
pub async fn quick_read<T, F, R>(lock: &RwLock<T>, f: F) -> R
where
    T: Clone,
    F: FnOnce(&T) -> R,
{
    let guard = lock.read().await;
    let data = guard.clone();
    drop(guard); // Explicitly release lock quickly
    f(&data)
}

/// Async-optimized lock guard for quick read operations without cloning
/// 
/// This helper performs a read operation that extracts only the needed data
/// and immediately releases the lock.
pub async fn quick_read_extract<T, F, R>(lock: &RwLock<T>, f: F) -> R
where
    F: FnOnce(&T) -> R,
{
    let guard = lock.read().await;
    let result = f(&guard);
    drop(guard); // Explicitly release lock quickly
    result
}

/// Async-optimized conditional update pattern
/// 
/// This helper reads the current value, applies a condition check,
/// and only acquires a write lock if an update is needed.
pub async fn conditional_update<T, C, U>(
    lock: &RwLock<T>,
    condition: C,
    update: U,
) -> bool
where
    T: Clone,
    C: FnOnce(&T) -> bool,
    U: FnOnce(&mut T),
{
    // Quick read to check condition
    let needs_update = {
        let guard = lock.read().await;
        condition(&guard)
    };
    
    if needs_update {
        let mut guard = lock.write().await;
        update(&mut guard);
        true
    } else {
        false
    }
}

/// Async timeout wrapper with better error handling
pub async fn with_timeout<F, T>(
    future: F,
    duration: Duration,
    operation_name: &str,
) -> crate::error::BlixardResult<T>
where
    F: Future<Output = T>,
{
    timeout(duration, future)
        .await
        .map_err(|_| crate::error::BlixardError::Timeout {
            operation: operation_name.to_string(),
            duration,
        })
}

/// Batch processor for reducing lock overhead
/// 
/// Collects items and processes them in batches to reduce
/// the number of lock acquisitions.
pub struct BatchProcessor<T> {
    items: Vec<T>,
    max_batch_size: usize,
    flush_interval: Duration,
}

impl<T> BatchProcessor<T> {
    pub fn new(max_batch_size: usize, flush_interval: Duration) -> Self {
        Self {
            items: Vec::with_capacity(max_batch_size),
            max_batch_size,
            flush_interval,
        }
    }
    
    pub fn add_item(&mut self, item: T) -> bool {
        self.items.push(item);
        self.items.len() >= self.max_batch_size
    }
    
    pub fn take_items(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
    }
    
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Optimized async map for concurrent operations
/// 
/// Processes a collection concurrently with limited parallelism
/// to avoid overwhelming the system.
pub async fn concurrent_map<T, F, R, Fut>(
    items: Vec<T>,
    max_concurrent: usize,
    f: F,
) -> Vec<R>
where
    F: Fn(T) -> Fut + Clone,
    Fut: Future<Output = R>,
{
    use futures::stream::{self, StreamExt};
    
    stream::iter(items)
        .map(f)
        .buffer_unordered(max_concurrent)
        .collect()
        .await
}

/// Lock-free counter using atomic operations
/// 
/// Provides atomic increment/decrement operations without locks
/// for simple counters.
#[derive(Debug, Default)]
pub struct AtomicCounter {
    value: std::sync::atomic::AtomicU64,
}

impl AtomicCounter {
    pub fn new(initial: u64) -> Self {
        Self {
            value: std::sync::atomic::AtomicU64::new(initial),
        }
    }
    
    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
    
    pub fn decrement(&self) -> u64 {
        self.value.fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
    }
    
    pub fn get(&self) -> u64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
    
    pub fn set(&self, value: u64) {
        self.value.store(value, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Helper for managing optional Arc references
/// 
/// Provides utilities for working with Optional<Arc<T>> patterns
/// common in shared state management.
#[derive(Debug)]
pub struct OptionalArc<T> {
    inner: Arc<RwLock<Option<Arc<T>>>>,
}

impl<T> OptionalArc<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }
    
    pub fn new_with_value(value: Arc<T>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(value))),
        }
    }
    
    pub async fn get(&self) -> Option<Arc<T>> {
        let guard = self.inner.read().await;
        guard.clone()
    }
    
    pub async fn set(&self, value: Option<Arc<T>>) {
        let mut guard = self.inner.write().await;
        *guard = value;
    }
    
    pub async fn take(&self) -> Option<Arc<T>> {
        let mut guard = self.inner.write().await;
        guard.take()
    }
    
    pub async fn with_value<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Arc<T>) -> R,
    {
        let guard = self.inner.read().await;
        guard.as_ref().map(f)
    }
}

impl<T> Clone for OptionalArc<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Default for OptionalArc<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Async retry utility with exponential backoff
pub async fn retry_with_backoff<F, T, E, Fut>(
    mut operation: F,
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut delay = initial_delay;
    let mut last_error = None;
    
    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                last_error = Some(error);
                
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, Instant};
    
    #[tokio::test]
    async fn test_quick_read() {
        let data = Arc::new(RwLock::new(String::from("test")));
        
        let result = quick_read(&data, |s| s.len()).await;
        assert_eq!(result, 4);
    }
    
    #[tokio::test]
    async fn test_conditional_update() {
        let counter = Arc::new(RwLock::new(0u32));
        
        // Should update when condition is true
        let updated = conditional_update(
            &counter,
            |&n| n < 5,
            |n| *n += 1,
        ).await;
        
        assert!(updated);
        assert_eq!(*counter.read().await, 1);
        
        // Should not update when condition is false
        let updated = conditional_update(
            &counter,
            |&n| n > 10,
            |n| *n += 1,
        ).await;
        
        assert!(!updated);
        assert_eq!(*counter.read().await, 1);
    }
    
    #[tokio::test]
    async fn test_with_timeout() {
        // Test successful operation
        let result = with_timeout(
            async { "success" },
            Duration::from_millis(100),
            "test_op",
        ).await;
        assert!(result.is_ok());
        
        // Test timeout
        let result = with_timeout(
            async {
                sleep(Duration::from_millis(200)).await;
                "never reached"
            },
            Duration::from_millis(50),
            "test_timeout",
        ).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_atomic_counter() {
        let counter = AtomicCounter::new(0);
        
        assert_eq!(counter.get(), 0);
        counter.increment();
        assert_eq!(counter.get(), 1);
        counter.decrement();
        assert_eq!(counter.get(), 0);
    }
    
    #[tokio::test]
    async fn test_optional_arc() {
        let opt = OptionalArc::<String>::new();
        
        assert!(opt.get().await.is_none());
        
        let value = Arc::new(String::from("test"));
        opt.set(Some(value.clone())).await;
        
        assert_eq!(opt.get().await.unwrap().as_str(), "test");
        
        let taken = opt.take().await;
        assert_eq!(taken.unwrap().as_str(), "test");
        assert!(opt.get().await.is_none());
    }
    
    #[tokio::test]
    async fn test_concurrent_map() {
        let items = vec![1, 2, 3, 4, 5];
        let counter = Arc::new(AtomicUsize::new(0));
        
        let results = concurrent_map(
            items,
            2, // max concurrent
            {
                let counter = counter.clone();
                move |x| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::Relaxed);
                        sleep(Duration::from_millis(10)).await;
                        x * 2
                    }
                }
            },
        ).await;
        
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }
    
    #[tokio::test]
    async fn test_retry_with_backoff() {
        let counter = Arc::new(AtomicUsize::new(0));
        
        // Test successful retry
        let result = retry_with_backoff(
            {
                let counter = counter.clone();
                move || {
                    let counter = counter.clone();
                    async move {
                        let count = counter.fetch_add(1, Ordering::Relaxed);
                        if count < 2 {
                            Err("not yet")
                        } else {
                            Ok("success")
                        }
                    }
                }
            },
            5,
            Duration::from_millis(1),
            Duration::from_millis(10),
        ).await;
        
        assert_eq!(result, Ok("success"));
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }
}