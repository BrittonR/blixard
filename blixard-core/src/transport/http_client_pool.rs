//! HTTP client connection pooling for external services
//!
//! This module provides a connection pool for HTTP clients to optimize
//! performance by reusing connections and reducing the overhead of 
//! establishing new connections for each request.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::debug;

use crate::error::{BlixardError, BlixardResult};
use crate::common::async_utils::{AtomicCounter, with_timeout};

/// Configuration for HTTP client pool
#[derive(Debug, Clone)]
pub struct HttpClientPoolConfig {
    /// Maximum number of clients per host
    pub max_clients_per_host: usize,
    /// Maximum total clients in pool
    pub max_total_clients: usize,
    /// Client idle timeout before removal
    pub idle_timeout: Duration,
    /// Connection timeout for new requests
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Whether to enable connection keep-alive
    pub keep_alive: bool,
    /// Pool cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for HttpClientPoolConfig {
    fn default() -> Self {
        Self {
            max_clients_per_host: 10,
            max_total_clients: 100,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keep_alive: true,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// Pooled HTTP client with metadata
#[derive(Debug, Clone)]
struct PooledClient {
    client: reqwest::Client,
    _created_at: Instant,
    last_used: Arc<RwLock<Instant>>,
    request_count: AtomicCounter,
}

impl PooledClient {
    fn new(client: reqwest::Client) -> Self {
        let now = Instant::now();
        Self {
            client,
            _created_at: now,
            last_used: Arc::new(RwLock::new(now)),
            request_count: AtomicCounter::new(0),
        }
    }

    async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
        self.request_count.increment();
    }

    async fn is_idle(&self, idle_timeout: Duration) -> bool {
        let last_used = *self.last_used.read().await;
        last_used.elapsed() > idle_timeout
    }
}

/// HTTP client connection pool
pub struct HttpClientPool {
    config: HttpClientPoolConfig,
    /// Clients organized by host
    clients: Arc<RwLock<HashMap<String, Vec<PooledClient>>>>,
    /// Total client count across all hosts
    total_clients: AtomicCounter,
    /// Background cleanup task handle
    cleanup_handle: Option<tokio::task::JoinHandle<()>>,
}

impl HttpClientPool {
    /// Create a new HTTP client pool
    pub fn new(config: HttpClientPoolConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            total_clients: AtomicCounter::new(0),
            cleanup_handle: None,
        }
    }

    /// Start the pool and background cleanup tasks
    pub async fn start(&mut self) -> BlixardResult<()> {
        if self.cleanup_handle.is_some() {
            return Ok(()); // Already started
        }

        let cleanup_handle = self.start_cleanup_task();
        self.cleanup_handle = Some(cleanup_handle);

        debug!("HTTP client pool started with config: {:?}", self.config);
        Ok(())
    }

    /// Get a client for the specified host
    pub async fn get_client(&self, host: &str) -> BlixardResult<HttpClient> {
        let client = self.get_or_create_client(host).await?;
        client.touch().await;
        
        Ok(HttpClient {
            client: client.client.clone(),
            _host: host.to_string(),
            config: self.config.clone(),
        })
    }

    /// Get or create a client for the host
    async fn get_or_create_client(&self, host: &str) -> BlixardResult<PooledClient> {
        // Try to get an existing client first
        {
            let clients = self.clients.read().await;
            if let Some(host_clients) = clients.get(host) {
                if let Some(client) = host_clients.first() {
                    return Ok(client.clone());
                }
            }
        }

        // Need to create a new client
        self.create_client(host).await
    }

    /// Create a new client for the host
    async fn create_client(&self, host: &str) -> BlixardResult<PooledClient> {
        // Check total client limit
        if self.total_clients.get() >= self.config.max_total_clients as u64 {
            return Err(BlixardError::ResourceExhausted {
                resource: format!("HTTP client pool (limit: {})", self.config.max_total_clients),
            });
        }

        let mut clients = self.clients.write().await;
        
        // Check per-host limit
        let host_clients = clients.entry(host.to_string()).or_insert_with(Vec::new);
        if host_clients.len() >= self.config.max_clients_per_host {
            // Remove the oldest client
            if let Some(old_client) = host_clients.pop() {
                drop(old_client);
                self.total_clients.decrement();
            }
        }

        // Create new HTTP client with optimized configuration
        let client = reqwest::ClientBuilder::new()
            .timeout(self.config.request_timeout)
            .connect_timeout(self.config.connect_timeout)
            .tcp_keepalive(if self.config.keep_alive {
                Some(Duration::from_secs(60))
            } else {
                None
            })
            .pool_idle_timeout(Some(self.config.idle_timeout))
            .pool_max_idle_per_host(self.config.max_clients_per_host)
            .http2_prior_knowledge()
            .http2_keep_alive_interval(Some(Duration::from_secs(30)))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| BlixardError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let pooled_client = PooledClient::new(client);
        host_clients.insert(0, pooled_client.clone());
        self.total_clients.increment();

        debug!("Created new HTTP client for host: {}", host);
        Ok(pooled_client)
    }

    /// Start cleanup task to remove idle clients
    fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let clients = Arc::clone(&self.clients);
        let total_clients = self.total_clients.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.cleanup_interval);
            
            loop {
                interval.tick().await;

                let mut removed_count = 0;
                let mut clients_guard = clients.write().await;
                
                for (host, host_clients) in clients_guard.iter_mut() {
                    let initial_len = host_clients.len();
                    
                    // Keep only non-idle clients
                    host_clients.retain(|client| {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                !client.is_idle(config.idle_timeout).await
                            })
                        })
                    });
                    
                    let removed = initial_len - host_clients.len();
                    removed_count += removed;
                    
                    if removed > 0 {
                        debug!("Removed {} idle clients for host: {}", removed, host);
                    }
                }

                // Remove empty host entries
                clients_guard.retain(|_, clients| !clients.is_empty());

                // Update total count
                for _ in 0..removed_count {
                    total_clients.decrement();
                }

                if removed_count > 0 {
                    debug!("Cleanup removed {} idle HTTP clients", removed_count);
                }
            }
        })
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> HttpClientPoolStats {
        let clients = self.clients.read().await;
        let mut total_per_host = HashMap::new();
        let mut total_requests = 0u64;

        for (host, host_clients) in clients.iter() {
            total_per_host.insert(host.clone(), host_clients.len());
            
            for client in host_clients {
                total_requests += client.request_count.get();
            }
        }

        HttpClientPoolStats {
            total_clients: self.total_clients.get() as usize,
            clients_per_host: total_per_host,
            total_requests,
        }
    }

    /// Shutdown the pool
    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }

        let mut clients = self.clients.write().await;
        clients.clear();
        self.total_clients.set(0);

        debug!("HTTP client pool shut down");
    }
}

impl Drop for HttpClientPool {
    fn drop(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }
}

/// HTTP client with automatic connection management
pub struct HttpClient {
    client: reqwest::Client,
    _host: String,
    config: HttpClientPoolConfig,
}

impl HttpClient {
    /// Perform a GET request with timeout
    pub async fn get(&self, url: &str) -> BlixardResult<reqwest::Response> {
        self.request(self.client.get(url)).await
    }

    /// Perform a POST request with timeout
    pub async fn post(&self, url: &str) -> BlixardResult<reqwest::RequestBuilder> {
        Ok(self.client.post(url))
    }

    /// Perform a PUT request with timeout
    pub async fn put(&self, url: &str) -> BlixardResult<reqwest::RequestBuilder> {
        Ok(self.client.put(url))
    }

    /// Perform a DELETE request with timeout
    pub async fn delete(&self, url: &str) -> BlixardResult<reqwest::RequestBuilder> {
        Ok(self.client.delete(url))
    }

    /// Execute a request with timeout and error handling
    pub async fn request(&self, request_builder: reqwest::RequestBuilder) -> BlixardResult<reqwest::Response> {
        let request = request_builder.build().map_err(|e| {
            BlixardError::NetworkError(format!("Failed to build request: {}", e))
        })?;

        let url = request.url().to_string();
        
        with_timeout(
            self.client.execute(request),
            self.config.request_timeout,
            &format!("HTTP request to {}", url),
        )
        .await?
        .map_err(|e| BlixardError::NetworkError(format!("HTTP request failed: {}", e)))
    }

    /// Get the underlying reqwest client
    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}

/// Pool statistics
#[derive(Debug)]
pub struct HttpClientPoolStats {
    pub total_clients: usize,
    pub clients_per_host: HashMap<String, usize>,
    pub total_requests: u64,
}

/// Global HTTP client pool instance
static HTTP_CLIENT_POOL: tokio::sync::OnceCell<Arc<Mutex<HttpClientPool>>> = tokio::sync::OnceCell::const_new();

/// Initialize the global HTTP client pool
pub async fn init_global_pool(config: HttpClientPoolConfig) -> BlixardResult<()> {
    let pool = Arc::new(Mutex::new(HttpClientPool::new(config)));
    pool.lock().await.start().await?;
    
    HTTP_CLIENT_POOL.set(pool).map_err(|_| {
        BlixardError::Internal {
            message: "HTTP client pool already initialized".to_string(),
        }
    })?;

    Ok(())
}

/// Get a client from the global pool
pub async fn get_global_client(host: &str) -> BlixardResult<HttpClient> {
    let pool = HTTP_CLIENT_POOL.get().ok_or_else(|| {
        BlixardError::NotInitialized {
            component: "HTTP client pool".to_string(),
        }
    })?;

    pool.lock().await.get_client(host).await
}

/// Get global pool statistics
pub async fn get_global_pool_stats() -> BlixardResult<HttpClientPoolStats> {
    let pool = HTTP_CLIENT_POOL.get().ok_or_else(|| {
        BlixardError::NotInitialized {
            component: "HTTP client pool".to_string(),
        }
    })?;

    Ok(pool.lock().await.get_stats().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_client_pool_creation() {
        let config = HttpClientPoolConfig::default();
        let mut pool = HttpClientPool::new(config);
        pool.start().await.unwrap();

        let _client = pool.get_client("httpbin.org").await.unwrap();
        // Client host is now private (_host)

        let stats = pool.get_stats().await;
        assert_eq!(stats.total_clients, 1);
        assert_eq!(stats.clients_per_host["httpbin.org"], 1);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_client_reuse() {
        let config = HttpClientPoolConfig::default();
        let mut pool = HttpClientPool::new(config);
        pool.start().await.unwrap();

        // Get multiple clients for the same host
        let _client1 = pool.get_client("httpbin.org").await.unwrap();
        let _client2 = pool.get_client("httpbin.org").await.unwrap();

        let stats = pool.get_stats().await;
        // Should still be 1 client as they're reused
        assert_eq!(stats.total_clients, 1);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_per_host_limits() {
        let config = HttpClientPoolConfig {
            max_clients_per_host: 2,
            ..Default::default()
        };
        let mut pool = HttpClientPool::new(config);
        pool.start().await.unwrap();

        // Create multiple clients for different hosts
        let _client1 = pool.get_client("host1.com").await.unwrap();
        let _client2 = pool.get_client("host2.com").await.unwrap();
        let _client3 = pool.get_client("host1.com").await.unwrap(); // Should reuse

        let stats = pool.get_stats().await;
        assert_eq!(stats.total_clients, 2); // One per unique host

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_idle_timeout() {
        let config = HttpClientPoolConfig {
            idle_timeout: Duration::from_millis(100),
            cleanup_interval: Duration::from_millis(50),
            ..Default::default()
        };
        let mut pool = HttpClientPool::new(config);
        pool.start().await.unwrap();

        let _client = pool.get_client("test.com").await.unwrap();
        
        // Wait for idle timeout and cleanup
        tokio::time::sleep(Duration::from_millis(200)).await;

        let stats = pool.get_stats().await;
        // Client should be removed due to idle timeout
        assert_eq!(stats.total_clients, 0);

        pool.shutdown().await;
    }
}