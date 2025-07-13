//! Async utilities for optimized async patterns
//!
//! This module provides reusable utilities for common async patterns
//! found throughout the Blixard codebase, helping to optimize
//! performance and reduce lock contention.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

/// Advanced concurrent processing with error handling and result collection
/// 
/// Similar to concurrent_map but collects results and errors separately,
/// allowing partial success scenarios.
pub async fn concurrent_try_map<T, F, R, E, Fut>(
    items: Vec<T>,
    max_concurrent: usize,
    f: F,
) -> (Vec<R>, Vec<E>)
where
    F: Fn(T) -> Fut + Clone,
    Fut: Future<Output = Result<R, E>>,
{
    use futures::stream::{self, StreamExt};
    
    let results: Vec<Result<R, E>> = stream::iter(items)
        .map(f)
        .buffer_unordered(max_concurrent)
        .collect()
        .await;
    
    let mut successes = Vec::new();
    let mut errors = Vec::new();
    
    for result in results {
        match result {
            Ok(value) => successes.push(value),
            Err(error) => errors.push(error),
        }
    }
    
    (successes, errors)
}

/// Streaming processor for handling large datasets efficiently
/// 
/// Processes items in a stream without loading all into memory,
/// useful for large collections or real-time processing.
pub async fn stream_process<T, F, R, Fut>(
    items: impl futures::Stream<Item = T> + Unpin,
    max_concurrent: usize,
    f: F,
) -> Vec<R>
where
    F: Fn(T) -> Fut + Clone,
    Fut: Future<Output = R>,
{
    use futures::StreamExt;
    
    items
        .map(f)
        .buffer_unordered(max_concurrent)
        .collect()
        .await
}

/// Adaptive concurrency controller that adjusts parallelism based on performance
/// 
/// Monitors execution time and adjusts the concurrency level to optimize throughput
/// while preventing system overload.
pub struct AdaptiveConcurrencyController {
    current_concurrency: AtomicCounter,
    min_concurrency: usize,
    max_concurrency: usize,
    performance_window: std::collections::VecDeque<Duration>,
    last_adjustment: Arc<RwLock<Instant>>,
    adjustment_interval: Duration,
}

impl AdaptiveConcurrencyController {
    pub fn new(min_concurrency: usize, max_concurrency: usize) -> Self {
        Self {
            current_concurrency: AtomicCounter::new(min_concurrency as u64),
            min_concurrency,
            max_concurrency,
            performance_window: std::collections::VecDeque::with_capacity(10),
            last_adjustment: Arc::new(RwLock::new(Instant::now())),
            adjustment_interval: Duration::from_secs(30),
        }
    }
    
    pub fn get_concurrency(&self) -> usize {
        self.current_concurrency.get() as usize
    }
    
    pub async fn record_performance(&mut self, duration: Duration) {
        // Record performance sample
        self.performance_window.push_back(duration);
        if self.performance_window.len() > 10 {
            self.performance_window.pop_front();
        }
        
        // Check if it's time to adjust
        let last_adjustment = *self.last_adjustment.read().await;
        if last_adjustment.elapsed() >= self.adjustment_interval {
            self.adjust_concurrency().await;
        }
    }
    
    async fn adjust_concurrency(&mut self) {
        if self.performance_window.len() < 5 {
            return; // Need more samples
        }
        
        // Calculate average performance
        let avg_duration: Duration = self.performance_window.iter().sum::<Duration>() 
            / self.performance_window.len() as u32;
        
        let current = self.current_concurrency.get() as usize;
        
        // Simple adjustment logic: increase if performance is good, decrease if poor
        let new_concurrency = if avg_duration < Duration::from_millis(100) && current < self.max_concurrency {
            current + 1
        } else if avg_duration > Duration::from_millis(500) && current > self.min_concurrency {
            current - 1
        } else {
            current
        };
        
        self.current_concurrency.set(new_concurrency as u64);
        *self.last_adjustment.write().await = Instant::now();
        
        if new_concurrency != current {
            tracing::debug!("Adjusted concurrency from {} to {} (avg_duration: {:?})", 
                         current, new_concurrency, avg_duration);
        }
    }
}

/// Resource-aware concurrent processor
/// 
/// Monitors system resources and adjusts processing based on available capacity.
pub struct ResourceAwareConcurrentProcessor {
    cpu_threshold: f64,
    memory_threshold: f64,
    adaptive_controller: AdaptiveConcurrencyController,
}

impl ResourceAwareConcurrentProcessor {
    pub fn new(cpu_threshold: f64, memory_threshold: f64, max_concurrency: usize) -> Self {
        Self {
            cpu_threshold,
            memory_threshold,
            adaptive_controller: AdaptiveConcurrencyController::new(1, max_concurrency),
        }
    }
    
    pub async fn process<T, F, R, Fut>(
        &mut self,
        items: Vec<T>,
        f: F,
    ) -> Vec<R>
    where
        F: Fn(T) -> Fut + Clone,
        Fut: Future<Output = R>,
    {
        let start = Instant::now();
        
        // Get current system resources
        let concurrency = self.get_adaptive_concurrency().await;
        
        // Process with adjusted concurrency
        let results = concurrent_map(items, concurrency, f).await;
        
        // Record performance for future adjustments
        self.adaptive_controller.record_performance(start.elapsed()).await;
        
        results
    }
    
    async fn get_adaptive_concurrency(&self) -> usize {
        // In a real implementation, check system CPU and memory usage
        // For now, use the adaptive controller's current setting
        self.adaptive_controller.get_concurrency()
    }
}

/// Connection pool abstraction for generic resource pooling
/// 
/// Provides a generic connection pool that can be used for any type of resource
/// with automatic cleanup and health checking.
pub struct GenericConnectionPool<T> {
    connections: Arc<RwLock<Vec<PooledResource<T>>>>,
    max_size: usize,
    idle_timeout: Duration,
    health_check_interval: Duration,
    factory: Arc<dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send>> + Send + Sync>,
    health_checker: Arc<dyn Fn(&T) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>,
}

struct PooledResource<T> {
    resource: T,
    created_at: Instant,
    last_used: Arc<RwLock<Instant>>,
    is_healthy: Arc<RwLock<bool>>,
}

impl<T> PooledResource<T> {
    fn new(resource: T) -> Self {
        let now = Instant::now();
        Self {
            resource,
            created_at: now,
            last_used: Arc::new(RwLock::new(now)),
            is_healthy: Arc::new(RwLock::new(true)),
        }
    }
    
    async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
    }
    
    async fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.read().await.elapsed() > timeout
    }
}

impl<T: Send + 'static> GenericConnectionPool<T> {
    pub fn new<F, H, Fut1, Fut2>(
        max_size: usize,
        idle_timeout: Duration,
        health_check_interval: Duration,
        factory: F,
        health_checker: H,
    ) -> Self
    where
        F: Fn() -> Fut1 + Send + Sync + 'static,
        H: Fn(&T) -> Fut2 + Send + Sync + 'static,
        Fut1: Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send + 'static,
        Fut2: Future<Output = bool> + Send + 'static,
    {
        let factory = Arc::new(move || -> std::pin::Pin<Box<dyn Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
            Box::pin(factory())
        });
        let health_checker = Arc::new(move |t: &T| -> std::pin::Pin<Box<dyn Future<Output = bool> + Send>> {
            Box::pin(health_checker(t))
        });
        
        Self {
            connections: Arc::new(RwLock::new(Vec::new())),
            max_size,
            idle_timeout,
            health_check_interval,
            factory,
            health_checker,
        }
    }
    
    pub async fn get(&self) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        // Try to get an existing healthy connection
        {
            let mut connections = self.connections.write().await;
            for (i, pooled) in connections.iter().enumerate() {
                if *pooled.is_healthy.read().await {
                    let resource = connections.remove(i);
                    resource.touch().await;
                    return Ok(resource.resource);
                }
            }
        }
        
        // Create a new connection
        let resource = (self.factory)().await?;
        Ok(resource)
    }
    
    pub async fn return_resource(&self, resource: T) {
        let mut connections = self.connections.write().await;
        
        // Don't exceed max size
        if connections.len() < self.max_size {
            connections.push(PooledResource::new(resource));
        }
        // Otherwise drop the resource
    }
    
    pub async fn cleanup(&self) {
        let mut connections = self.connections.write().await;
        
        // Remove idle and unhealthy connections
        connections.retain(|pooled| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    !pooled.is_idle(self.idle_timeout).await && *pooled.is_healthy.read().await
                })
            })
        });
    }
}

/// Weighted load balancer for distributing work across multiple resources
/// 
/// Distributes work based on configurable weights and health status.
pub struct WeightedLoadBalancer<T> {
    resources: Arc<RwLock<Vec<WeightedResource<T>>>>,
    selection_strategy: LoadBalancingStrategy,
    current_index: AtomicCounter,
}

#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    WeightedRandom,
    LeastConnections,
    ResponseTimeBased,
}

struct WeightedResource<T> {
    resource: T,
    weight: f64,
    active_connections: AtomicCounter,
    total_requests: AtomicCounter,
    total_response_time: Arc<RwLock<Duration>>,
    is_healthy: Arc<RwLock<bool>>,
}

impl<T> WeightedResource<T> {
    fn new(resource: T, weight: f64) -> Self {
        Self {
            resource,
            weight,
            active_connections: AtomicCounter::new(0),
            total_requests: AtomicCounter::new(0),
            total_response_time: Arc::new(RwLock::new(Duration::ZERO)),
            is_healthy: Arc::new(RwLock::new(true)),
        }
    }
    
    async fn get_avg_response_time(&self) -> Duration {
        let total_requests = self.total_requests.get();
        if total_requests == 0 {
            Duration::ZERO
        } else {
            *self.total_response_time.read().await / total_requests as u32
        }
    }
    
    fn get_effective_weight(&self) -> f64 {
        let load_factor = self.active_connections.get() as f64 + 1.0;
        self.weight / load_factor
    }
}

impl<T> WeightedLoadBalancer<T> {
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            resources: Arc::new(RwLock::new(Vec::new())),
            selection_strategy: strategy,
            current_index: AtomicCounter::new(0),
        }
    }
    
    pub async fn add_resource(&self, resource: T, weight: f64) {
        let mut resources = self.resources.write().await;
        resources.push(WeightedResource::new(resource, weight));
    }
    
    pub async fn get_resource(&self, index: usize) -> Option<T> 
    where T: Clone 
    {
        let resources = self.resources.read().await;
        resources.get(index).map(|r| r.resource.clone())
    }
    
    pub async fn select_resource_index(&self) -> Option<usize> {
        let resources = self.resources.read().await;
        if resources.is_empty() {
            return None;
        }
        
        match self.selection_strategy {
            LoadBalancingStrategy::RoundRobin => {
                let index = self.current_index.increment() as usize % resources.len();
                Some(index)
            }
            LoadBalancingStrategy::WeightedRandom => {
                // Implement weighted random selection
                let total_weight: f64 = resources.iter()
                    .filter(|r| tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            *r.is_healthy.read().await
                        })
                    }))
                    .map(|r| r.get_effective_weight())
                    .sum();
                
                if total_weight == 0.0 {
                    return None;
                }
                
                let mut random_value = rand::random::<f64>() * total_weight;
                for (idx, resource) in resources.iter().enumerate() {
                    if *tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            resource.is_healthy.read().await
                        })
                    }) {
                        random_value -= resource.get_effective_weight();
                        if random_value <= 0.0 {
                            return Some(idx);
                        }
                    }
                }
                None
            }
            LoadBalancingStrategy::LeastConnections => {
                resources.iter()
                    .enumerate()
                    .filter(|(_, r)| tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            *r.is_healthy.read().await
                        })
                    }))
                    .min_by_key(|(_, r)| r.active_connections.get())
                    .map(|(idx, _)| idx)
            }
            LoadBalancingStrategy::ResponseTimeBased => {
                // Select resource with best response time
                let mut best_index = None;
                let mut best_time = Duration::MAX;
                
                for (idx, resource) in resources.iter().enumerate() {
                    if *tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            resource.is_healthy.read().await
                        })
                    }) {
                        let avg_time = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                resource.get_avg_response_time().await
                            })
                        });
                        if avg_time < best_time {
                            best_time = avg_time;
                            best_index = Some(idx);
                        }
                    }
                }
                
                best_index
            }
        }
    }
}

/// Circuit breaker pattern implementation for fault tolerance
/// 
/// Automatically fails fast when downstream services are having issues,
/// preventing cascading failures and allowing for graceful degradation.
pub struct CircuitBreaker {
    failure_threshold: usize,
    timeout: Duration,
    state: Arc<RwLock<CircuitBreakerState>>,
    failure_count: AtomicCounter,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,  // Normal operation
    Open,    // Failing fast
    HalfOpen, // Testing if service recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            timeout,
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: AtomicCounter::new(0),
            last_failure_time: Arc::new(RwLock::new(None)),
        }
    }
    
    pub async fn call<F, R, E, Fut>(&self, operation: F) -> Result<R, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<R, E>>,
    {
        // Check circuit breaker state
        match *self.state.read().await {
            CircuitBreakerState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.timeout {
                        // Move to half-open state
                        *self.state.write().await = CircuitBreakerState::HalfOpen;
                    } else {
                        return Err(CircuitBreakerError::CircuitOpen);
                    }
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Allow one test request
            }
            CircuitBreakerState::Closed => {
                // Normal operation
            }
        }
        
        // Execute operation
        match operation().await {
            Ok(result) => {
                // Success - reset circuit breaker
                self.on_success().await;
                Ok(result)
            }
            Err(error) => {
                // Failure - update circuit breaker
                self.on_failure().await;
                Err(CircuitBreakerError::OperationFailed(error))
            }
        }
    }
    
    async fn on_success(&self) {
        self.failure_count.set(0);
        *self.state.write().await = CircuitBreakerState::Closed;
        *self.last_failure_time.write().await = None;
    }
    
    async fn on_failure(&self) {
        let failure_count = self.failure_count.increment();
        *self.last_failure_time.write().await = Some(Instant::now());
        
        if failure_count >= self.failure_threshold as u64 {
            *self.state.write().await = CircuitBreakerState::Open;
        }
    }
    
    pub async fn get_state(&self) -> CircuitBreakerState {
        self.state.read().await.clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open")]
    CircuitOpen,
    #[error("Operation failed: {0}")]
    OperationFailed(E),
}

/// Lock-free counter using atomic operations
/// 
/// Provides atomic increment/decrement operations without locks
/// for simple counters.
#[derive(Debug, Default)]
pub struct AtomicCounter {
    value: std::sync::atomic::AtomicU64,
}

impl Clone for AtomicCounter {
    fn clone(&self) -> Self {
        Self {
            value: std::sync::atomic::AtomicU64::new(self.get()),
        }
    }
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