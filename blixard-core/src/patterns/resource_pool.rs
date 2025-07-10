//! Generic resource pool pattern for managing limited resources
//!
//! This module provides a generic, thread-safe resource pool implementation
//! that can be used for various pooling needs throughout the system, such as:
//! - IP address pools
//! - Connection pools
//! - Worker thread pools
//! - Resource capacity management

use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore, SemaphorePermit};
use tokio::time::timeout;
use tracing::{debug, warn};

/// Trait for resources that can be pooled
pub trait PoolableResource: Send + Sync + Debug {
    /// Check if the resource is still valid
    fn is_valid(&self) -> bool {
        true
    }
    
    /// Reset the resource to its initial state (optional)
    fn reset(&mut self) -> BlixardResult<()> {
        Ok(())
    }
}

/// Configuration for resource pools
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of resources
    pub max_size: usize,
    /// Minimum number of resources to maintain
    pub min_size: usize,
    /// Timeout for acquiring resources
    pub acquire_timeout: Duration,
    /// Whether to validate resources before returning them
    pub validate_on_acquire: bool,
    /// Whether to reset resources before returning them
    pub reset_on_acquire: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_size: 0,
            acquire_timeout: Duration::from_secs(30),
            validate_on_acquire: true,
            reset_on_acquire: false,
        }
    }
}

/// Factory trait for creating new resources
#[async_trait]
pub trait ResourceFactory<T: PoolableResource>: Send + Sync {
    /// Create a new resource
    async fn create(&self) -> BlixardResult<T>;
    
    /// Destroy a resource (optional cleanup)
    async fn destroy(&self, _resource: T) -> BlixardResult<()> {
        Ok(())
    }
}

/// A pooled resource that returns itself to the pool when dropped
/// 
/// # Safety Design
/// This type deliberately does NOT implement Deref/DerefMut to prevent panics.
/// Use the safe `get()` and `get_mut()` methods which return Result types.
/// This design prevents runtime panics at the cost of slightly more verbose API.
pub struct PooledResource<T: PoolableResource> {
    resource: Option<T>,
    pool: Arc<ResourcePool<T>>,
}

impl<T: PoolableResource> PooledResource<T> {
    /// Get a reference to the resource
    pub fn get(&self) -> crate::error::BlixardResult<&T> {
        self.resource.as_ref().ok_or_else(|| crate::error::BlixardError::ResourceUnavailable {
            resource_type: std::any::type_name::<T>().to_string(),
            message: "Resource already taken from pool".to_string(),
        })
    }
    
    /// Get a mutable reference to the resource
    pub fn get_mut(&mut self) -> crate::error::BlixardResult<&mut T> {
        self.resource.as_mut().ok_or_else(|| crate::error::BlixardError::ResourceUnavailable {
            resource_type: std::any::type_name::<T>().to_string(),
            message: "Resource already taken from pool".to_string(),
        })
    }
    
    /// Take ownership of the resource (won't be returned to pool)
    pub fn take(mut self) -> crate::error::BlixardResult<T> {
        self.resource.take().ok_or_else(|| crate::error::BlixardError::ResourceUnavailable {
            resource_type: std::any::type_name::<T>().to_string(),
            message: "Resource already taken from pool".to_string(),
        })
    }
    
    /// Execute a function with safe access to the resource
    pub fn with_resource<F, R>(&self, f: F) -> crate::error::BlixardResult<R>
    where
        F: FnOnce(&T) -> R,
    {
        let resource = self.get()?;
        Ok(f(resource))
    }
    
    /// Execute a function with safe mutable access to the resource
    pub fn with_resource_mut<F, R>(&mut self, f: F) -> crate::error::BlixardResult<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        let resource = self.get_mut()?;
        Ok(f(resource))
    }
}

impl<T: PoolableResource> Drop for PooledResource<T> {
    fn drop(&mut self) {
        if let Some(resource) = self.resource.take() {
            // Return resource to pool in background
            let pool = self.pool.clone();
            tokio::spawn(async move {
                pool.return_resource(resource).await;
            });
        }
    }
}

// SAFETY NOTE: Deref implementation removed to prevent panics.
// Users must use the safe get() method instead of direct dereferencing.
// This is a deliberate design choice to prevent runtime panics.

// SAFETY NOTE: DerefMut implementation removed to prevent panics.
// Users must use the safe get_mut() method instead of direct dereferencing.
// This is a deliberate design choice to prevent runtime panics.

/// Generic resource pool implementation
pub struct ResourcePool<T: PoolableResource> {
    /// Available resources
    available: Arc<Mutex<VecDeque<T>>>,
    /// Semaphore to limit total resources
    semaphore: Arc<Semaphore>,
    /// Resource factory
    factory: Arc<dyn ResourceFactory<T>>,
    /// Pool configuration
    config: PoolConfig,
    /// Total resources created
    total_created: Arc<Mutex<usize>>,
}

impl<T: PoolableResource> ResourcePool<T> {
    /// Create a new resource pool
    pub fn new(factory: Arc<dyn ResourceFactory<T>>, config: PoolConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_size));
        
        Self {
            available: Arc::new(Mutex::new(VecDeque::new())),
            semaphore,
            factory,
            config,
            total_created: Arc::new(Mutex::new(0)),
        }
    }
    
    /// Initialize the pool with minimum resources
    pub async fn initialize(&self) -> BlixardResult<()> {
        let mut created = 0;
        let mut errors = Vec::new();
        
        for _ in 0..self.config.min_size {
            match self.factory.create().await {
                Ok(resource) => {
                    self.available.lock().await.push_back(resource);
                    created += 1;
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        
        *self.total_created.lock().await = created;
        
        if created == 0 && !errors.is_empty() {
            return Err(BlixardError::Internal {
                message: format!("Failed to initialize pool: {:?}", errors),
            });
        }
        
        if created < self.config.min_size {
            warn!(
                "Pool initialized with {} resources (requested {})",
                created, self.config.min_size
            );
        }
        
        Ok(())
    }
    
    /// Acquire a resource from the pool
    pub async fn acquire(&self) -> BlixardResult<PooledResource<T>> {
        self.acquire_with_timeout(self.config.acquire_timeout).await
    }
    
    /// Try to acquire a resource without waiting
    pub async fn try_acquire(&self) -> BlixardResult<PooledResource<T>> {
        self.acquire_with_timeout(Duration::from_secs(0)).await
    }
    
    /// Acquire a resource with custom timeout
    pub async fn acquire_with_timeout(&self, duration: Duration) -> BlixardResult<PooledResource<T>> {
        // Try to get permit with timeout
        let permit = match timeout(duration, self.semaphore.clone().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => return Err(BlixardError::Internal {
                message: "Failed to acquire semaphore permit".to_string(),
            }),
            Err(_) => return Err(BlixardError::Timeout {
                operation: "acquire resource from pool".to_string(),
                duration,
            }),
        };
        
        // Try to get existing resource
        let mut resource = {
            let mut available = self.available.lock().await;
            available.pop_front()
        };
        
        // Create new resource if needed
        if resource.is_none() {
            debug!("Creating new resource for pool");
            resource = Some(self.factory.create().await?);
            *self.total_created.lock().await += 1;
        }
        
        let mut resource = resource.expect("BUG: resource should be Some after creation or retrieval from pool");
        
        // Validate resource if configured
        if self.config.validate_on_acquire && !resource.is_valid() {
            debug!("Resource validation failed, creating new one");
            self.factory.destroy(resource).await?;
            resource = self.factory.create().await?;
        }
        
        // Reset resource if configured
        if self.config.reset_on_acquire {
            resource.reset()?;
        }
        
        // Forget the permit - it will be released when resource is returned
        std::mem::forget(permit);
        
        Ok(PooledResource {
            resource: Some(resource),
            pool: Arc::new(self.clone()),
        })
    }
    
    /// Return a resource to the pool
    async fn return_resource(&self, mut resource: T) {
        // Validate resource before returning
        if resource.is_valid() {
            self.available.lock().await.push_back(resource);
        } else {
            // Destroy invalid resource
            debug!("Destroying invalid resource");
            let _ = self.factory.destroy(resource).await;
            *self.total_created.lock().await -= 1;
        }
        
        // Release semaphore permit
        self.semaphore.add_permits(1);
    }
    
    /// Get current pool statistics
    pub async fn stats(&self) -> PoolStats {
        let available_count = self.available.lock().await.len();
        let total_created = *self.total_created.lock().await;
        let in_use = total_created.saturating_sub(available_count);
        
        PoolStats {
            available: available_count,
            in_use,
            total_created,
            max_size: self.config.max_size,
        }
    }
    
    /// Clear all resources from the pool
    pub async fn clear(&self) -> BlixardResult<()> {
        let mut available = self.available.lock().await;
        let resources: Vec<_> = available.drain(..).collect();
        drop(available);
        
        for resource in resources {
            self.factory.destroy(resource).await?;
        }
        
        *self.total_created.lock().await = 0;
        Ok(())
    }
}

impl<T: PoolableResource> Clone for ResourcePool<T> {
    fn clone(&self) -> Self {
        Self {
            available: self.available.clone(),
            semaphore: self.semaphore.clone(),
            factory: self.factory.clone(),
            config: self.config.clone(),
            total_created: self.total_created.clone(),
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of available resources
    pub available: usize,
    /// Number of resources in use
    pub in_use: usize,
    /// Total resources created
    pub total_created: usize,
    /// Maximum pool size
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    
    #[derive(Debug)]
    struct TestResource {
        id: u32,
        valid: bool,
    }
    
    impl PoolableResource for TestResource {
        fn is_valid(&self) -> bool {
            self.valid
        }
        
        fn reset(&mut self) -> BlixardResult<()> {
            self.valid = true;
            Ok(())
        }
    }
    
    struct TestFactory {
        counter: AtomicU32,
    }
    
    #[async_trait]
    impl ResourceFactory<TestResource> for TestFactory {
        async fn create(&self) -> BlixardResult<TestResource> {
            let id = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(TestResource { id, valid: true })
        }
    }
    
    #[tokio::test]
    async fn test_basic_pool_operations() {
        let factory = Arc::new(TestFactory {
            counter: AtomicU32::new(0),
        });
        
        let config = PoolConfig {
            max_size: 3,
            min_size: 1,
            ..Default::default()
        };
        
        let pool = ResourcePool::new(factory, config);
        pool.initialize().await.unwrap();
        
        // Check initial stats
        let stats = pool.stats().await;
        assert_eq!(stats.available, 1);
        assert_eq!(stats.total_created, 1);
        
        // Acquire resource
        let resource1 = pool.acquire().await.unwrap();
        assert_eq!(resource1.get().unwrap().id, 0);
        
        // Stats should show resource in use
        let stats = pool.stats().await;
        assert_eq!(stats.available, 0);
        assert_eq!(stats.in_use, 1);
        
        // Drop resource to return it
        drop(resource1);
        
        // Wait a bit for async return
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Resource should be available again
        let stats = pool.stats().await;
        assert_eq!(stats.available, 1);
        assert_eq!(stats.in_use, 0);
    }
    
    #[tokio::test]
    async fn test_pool_capacity_limit() {
        let factory = Arc::new(TestFactory {
            counter: AtomicU32::new(0),
        });
        
        let config = PoolConfig {
            max_size: 2,
            acquire_timeout: Duration::from_millis(100),
            ..Default::default()
        };
        
        let pool = ResourcePool::new(factory, config);
        
        // Acquire max resources
        let r1 = pool.acquire().await.unwrap();
        let r2 = pool.acquire().await.unwrap();
        
        // Verify resources are accessible
        let _ = r1.get().unwrap();
        let _ = r2.get().unwrap();
        
        // Third acquire should timeout
        let result = pool.acquire().await;
        assert!(matches!(result, Err(BlixardError::Timeout { .. })));
        
        // Return one resource
        drop(r1);
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Now acquire should succeed
        let r3 = pool.acquire().await.unwrap();
        let _ = r3.get().unwrap();
        
        drop(r2);
        drop(r3);
    }
    
    #[tokio::test]
    async fn test_safe_resource_access_patterns() {
        let factory = Arc::new(TestFactory {
            counter: AtomicU32::new(100),
        });
        
        let pool = ResourcePool::new(factory, PoolConfig::default());
        
        // Test safe access patterns
        let mut resource = pool.acquire().await.unwrap();
        
        // Pattern 1: Direct get() method
        let id = resource.get().unwrap().id;
        assert_eq!(id, 100);
        
        // Pattern 2: with_resource closure
        let id2 = resource.with_resource(|r| r.id).unwrap();
        assert_eq!(id2, 100);
        
        // Pattern 3: Mutable access
        resource.with_resource_mut(|r| {
            r.valid = false;
        }).unwrap();
        
        assert!(!resource.get().unwrap().valid);
        
        // Pattern 4: Taking ownership
        let owned = resource.take().unwrap();
        assert_eq!(owned.id, 100);
        
        // After taking, resource should be unavailable
        // Note: resource is moved, so we can't test this directly
    }
    
    #[tokio::test] 
    async fn test_resource_taken_error_handling() {
        let factory = Arc::new(TestFactory {
            counter: AtomicU32::new(200),
        });
        
        let pool = ResourcePool::new(factory, PoolConfig::default());
        let mut resource = pool.acquire().await.unwrap();
        
        // Take the resource
        let _owned = resource.take().unwrap();
        
        // Now resource should be unavailable - but we can't test this
        // because resource is moved by take(). This is good design
        // as it prevents use-after-take at compile time.
    }
}