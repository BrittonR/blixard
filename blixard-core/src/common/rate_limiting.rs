//! Consolidated rate limiting utilities
//!
//! This module provides a unified approach to rate limiting
//! across different services and operations.

use crate::{
    error::{BlixardError, BlixardResult},
    quota_manager::QuotaManager,
    resource_quotas::ApiOperation,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Rate limiter trait for different implementations
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if an operation is allowed
    async fn check_rate_limit(&self, key: &str, operation: &ApiOperation) -> BlixardResult<()>;
    
    /// Record that an operation occurred
    async fn record_operation(&self, key: &str, operation: &ApiOperation);
    
    /// Check and record in one atomic operation
    async fn check_and_record(&self, key: &str, operation: &ApiOperation) -> BlixardResult<()> {
        self.check_rate_limit(key, operation).await?;
        self.record_operation(key, operation).await;
        Ok(())
    }
}

/// Rate limiter implementation using QuotaManager
pub struct QuotaBasedRateLimiter {
    quota_manager: Arc<QuotaManager>,
}

impl QuotaBasedRateLimiter {
    /// Create a new quota-based rate limiter
    pub fn new(quota_manager: Arc<QuotaManager>) -> Self {
        Self { quota_manager }
    }
}

#[async_trait::async_trait]
impl RateLimiter for QuotaBasedRateLimiter {
    async fn check_rate_limit(&self, key: &str, operation: &ApiOperation) -> BlixardResult<()> {
        self.quota_manager
            .check_rate_limit(key, operation)
            .await
            .map_err(|violation| BlixardError::Internal {
                message: format!("Rate limit exceeded: {}", violation),
            })
    }
    
    async fn record_operation(&self, key: &str, operation: &ApiOperation) {
        self.quota_manager.record_api_request(key, operation).await;
    }
}

/// Simple in-memory rate limiter for testing
pub struct InMemoryRateLimiter {
    limits: HashMap<ApiOperation, RateLimit>,
    state: Arc<RwLock<HashMap<String, HashMap<ApiOperation, OperationState>>>>,
}

#[derive(Clone)]
struct RateLimit {
    max_requests: u32,
    window: Duration,
}

struct OperationState {
    count: u32,
    window_start: Instant,
}

impl InMemoryRateLimiter {
    /// Create a new in-memory rate limiter
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        
        // Default limits
        limits.insert(ApiOperation::VmCreate, RateLimit {
            max_requests: 10,
            window: Duration::from_secs(60),
        });
        limits.insert(ApiOperation::VmDelete, RateLimit {
            max_requests: 10,
            window: Duration::from_secs(60),
        });
        limits.insert(ApiOperation::ClusterJoin, RateLimit {
            max_requests: 5,
            window: Duration::from_secs(300),
        });
        
        Self {
            limits,
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set a custom limit for an operation
    pub fn set_limit(&mut self, operation: ApiOperation, max_requests: u32, window: Duration) {
        self.limits.insert(operation, RateLimit { max_requests, window });
    }
}

impl Default for InMemoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check_rate_limit(&self, key: &str, operation: &ApiOperation) -> BlixardResult<()> {
        let limit = self.limits.get(operation).ok_or_else(|| {
            BlixardError::Internal {
                message: format!("No rate limit configured for operation: {:?}", operation),
            }
        })?;
        
        let mut state = self.state.write().await;
        let key_state = state.entry(key.to_string()).or_insert_with(HashMap::new);
        let op_state = key_state.entry(operation.clone()).or_insert_with(|| {
            OperationState {
                count: 0,
                window_start: Instant::now(),
            }
        });
        
        // Check if window has expired
        if op_state.window_start.elapsed() > limit.window {
            op_state.count = 0;
            op_state.window_start = Instant::now();
        }
        
        // Check limit
        if op_state.count >= limit.max_requests {
            return Err(BlixardError::Internal {
                message: format!(
                    "Rate limit exceeded for {}: {} requests in {:?}",
                    key, limit.max_requests, limit.window
                ),
            });
        }
        
        Ok(())
    }
    
    async fn record_operation(&self, key: &str, operation: &ApiOperation) {
        let mut state = self.state.write().await;
        let key_state = state.entry(key.to_string()).or_insert_with(HashMap::new);
        let op_state = key_state.entry(operation.clone()).or_insert_with(|| {
            OperationState {
                count: 0,
                window_start: Instant::now(),
            }
        });
        
        op_state.count += 1;
    }
}

/// Rate limiting middleware for gRPC requests
pub struct RateLimitingMiddleware<R: RateLimiter> {
    rate_limiter: Arc<R>,
    extract_key: Box<dyn Fn(&tonic::MetadataMap) -> String + Send + Sync>,
}

impl<R: RateLimiter> RateLimitingMiddleware<R> {
    /// Create new rate limiting middleware
    pub fn new(rate_limiter: Arc<R>) -> Self {
        Self {
            rate_limiter,
            extract_key: Box::new(|metadata| {
                // Default: extract tenant ID from metadata
                metadata
                    .get("tenant-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("default")
                    .to_string()
            }),
        }
    }
    
    /// Set custom key extraction function
    pub fn with_key_extractor<F>(mut self, f: F) -> Self
    where
        F: Fn(&tonic::MetadataMap) -> String + Send + Sync + 'static,
    {
        self.extract_key = Box::new(f);
        self
    }
    
    /// Check rate limit for a request
    pub async fn check_rate_limit<T>(
        &self,
        request: &tonic::Request<T>,
        operation: ApiOperation,
    ) -> Result<String, tonic::Status> {
        // Safely extract metadata without type transmutation
        let key = (self.extract_key)(request.metadata());
        
        self.rate_limiter
            .check_and_record(&key, &operation)
            .await
            .map_err(|e| tonic::Status::resource_exhausted(e.to_string()))?;
            
        Ok(key)
    }
}

/// Builder for rate limiting configuration
pub struct RateLimitBuilder {
    limits: HashMap<ApiOperation, (u32, Duration)>,
}

impl RateLimitBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            limits: HashMap::new(),
        }
    }
    
    /// Set limit for an operation
    pub fn limit(mut self, operation: ApiOperation, max_requests: u32, window: Duration) -> Self {
        self.limits.insert(operation, (max_requests, window));
        self
    }
    
    /// Set VM operation limits
    pub fn vm_limits(mut self, requests_per_minute: u32) -> Self {
        let window = Duration::from_secs(60);
        self.limits.insert(ApiOperation::VmCreate, (requests_per_minute, window));
        self.limits.insert(ApiOperation::VmDelete, (requests_per_minute, window));
        self
    }
    
    /// Set cluster operation limits
    pub fn cluster_limits(mut self, requests_per_5min: u32) -> Self {
        let window = Duration::from_secs(300);
        self.limits.insert(ApiOperation::ClusterJoin, (requests_per_5min, window));
        self
    }
    
    /// Build an in-memory rate limiter
    pub fn build_in_memory(self) -> InMemoryRateLimiter {
        let mut limiter = InMemoryRateLimiter::new();
        for (op, (max, window)) in self.limits {
            limiter.set_limit(op, max, window);
        }
        limiter
    }
}

impl Default for RateLimitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_in_memory_rate_limiter() {
        let limiter = RateLimitBuilder::new()
            .vm_limits(2)
            .build_in_memory();
            
        // First two requests should succeed
        assert!(limiter.check_and_record("test", &ApiOperation::VmCreate).await.is_ok());
        assert!(limiter.check_and_record("test", &ApiOperation::VmCreate).await.is_ok());
        
        // Third request should fail
        assert!(limiter.check_and_record("test", &ApiOperation::VmCreate).await.is_err());
        
        // Different key should succeed
        assert!(limiter.check_and_record("other", &ApiOperation::VmCreate).await.is_ok());
    }
}