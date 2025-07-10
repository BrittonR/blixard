//! Example implementations of ResourcePool for common use cases
//!
//! This module demonstrates how to use the generic ResourcePool pattern
//! for various resource management scenarios in Blixard.

use super::resource_pool::{PoolableResource, ResourceFactory, ResourcePool, PoolConfig};
use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Example 1: IP Address Pool
/// 
/// Manages a pool of IP addresses for VM allocation
#[derive(Debug, Clone)]
pub struct IpAddressResource {
    pub address: IpAddr,
    pub allocated_to: Option<String>,
}

impl PoolableResource for IpAddressResource {
    fn is_valid(&self) -> bool {
        // IP addresses are always valid unless explicitly marked
        true
    }
    
    fn reset(&mut self) -> BlixardResult<()> {
        // Clear allocation when returning to pool
        self.allocated_to = None;
        Ok(())
    }
}

/// Factory for creating IP addresses from a subnet
pub struct IpAddressFactory {
    subnet_base: Ipv4Addr,
    next_host: AtomicU32,
    max_hosts: u32,
}

impl IpAddressFactory {
    pub fn new(subnet: &str, max_hosts: u32) -> BlixardResult<Self> {
        let subnet_base: Ipv4Addr = subnet.parse()
            .map_err(|_| BlixardError::InvalidInput {
                field: "subnet".to_string(),
                value: subnet.to_string(),
                reason: "Invalid IPv4 address".to_string(),
            })?;
            
        Ok(Self {
            subnet_base,
            next_host: AtomicU32::new(1), // Start from .1, .0 is network
            max_hosts,
        })
    }
}

#[async_trait]
impl ResourceFactory<IpAddressResource> for IpAddressFactory {
    async fn create(&self) -> BlixardResult<IpAddressResource> {
        let host_num = self.next_host.fetch_add(1, Ordering::SeqCst);
        
        if host_num >= self.max_hosts {
            return Err(BlixardError::ResourceExhausted {
                resource_type: "IP addresses".to_string(),
                details: format!("All {} addresses allocated", self.max_hosts),
            });
        }
        
        let octets = self.subnet_base.octets();
        let address = Ipv4Addr::new(
            octets[0],
            octets[1],
            octets[2],
            octets[3].saturating_add(host_num as u8),
        );
        
        Ok(IpAddressResource {
            address: IpAddr::V4(address),
            allocated_to: None,
        })
    }
}

/// Example 2: Database Connection Pool
///
/// Manages database connections with health checking
#[derive(Debug)]
pub struct DatabaseConnection {
    pub id: u32,
    pub connected: bool,
    pub last_query_error: Option<String>,
}

impl PoolableResource for DatabaseConnection {
    fn is_valid(&self) -> bool {
        self.connected && self.last_query_error.is_none()
    }
    
    fn reset(&mut self) -> BlixardResult<()> {
        // Reset connection state
        self.last_query_error = None;
        // In real implementation, would rollback any transactions
        Ok(())
    }
}

pub struct DatabaseConnectionFactory {
    connection_counter: AtomicU32,
    database_url: String,
}

impl DatabaseConnectionFactory {
    pub fn new(database_url: String) -> Self {
        Self {
            connection_counter: AtomicU32::new(0),
            database_url,
        }
    }
}

#[async_trait]
impl ResourceFactory<DatabaseConnection> for DatabaseConnectionFactory {
    async fn create(&self) -> BlixardResult<DatabaseConnection> {
        let id = self.connection_counter.fetch_add(1, Ordering::SeqCst);
        
        // In real implementation, would actually connect to database
        // For now, simulate connection
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        Ok(DatabaseConnection {
            id,
            connected: true,
            last_query_error: None,
        })
    }
    
    async fn destroy(&self, mut connection: DatabaseConnection) -> BlixardResult<()> {
        // Clean disconnect
        connection.connected = false;
        // In real implementation, would close the connection
        Ok(())
    }
}

/// Example 3: Worker Thread Pool
///
/// Manages a pool of worker contexts for parallel processing
#[derive(Debug)]
pub struct WorkerContext {
    pub worker_id: u32,
    pub tasks_completed: u32,
    pub current_task: Option<String>,
}

impl PoolableResource for WorkerContext {
    fn is_valid(&self) -> bool {
        // Worker is valid if not currently processing
        self.current_task.is_none()
    }
    
    fn reset(&mut self) -> BlixardResult<()> {
        self.current_task = None;
        Ok(())
    }
}

pub struct WorkerFactory {
    next_worker_id: AtomicU32,
}

impl WorkerFactory {
    pub fn new() -> Self {
        Self {
            next_worker_id: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ResourceFactory<WorkerContext> for WorkerFactory {
    async fn create(&self) -> BlixardResult<WorkerContext> {
        let worker_id = self.next_worker_id.fetch_add(1, Ordering::SeqCst);
        
        Ok(WorkerContext {
            worker_id,
            tasks_completed: 0,
            current_task: None,
        })
    }
}

/// Example usage function showing how to use these pools
pub async fn demonstrate_resource_pools() -> BlixardResult<()> {
    // Create IP address pool
    let ip_factory = Arc::new(IpAddressFactory::new("192.168.1.0", 254)?);
    let ip_config = PoolConfig {
        max_size: 50,
        min_size: 10,
        reset_on_acquire: true,
        ..Default::default()
    };
    let ip_pool = ResourcePool::new(ip_factory, ip_config);
    ip_pool.initialize().await?;
    
    // Allocate an IP address
    {
        let mut ip_resource = ip_pool.acquire().await?;
        ip_resource.allocated_to = Some("vm-123".to_string());
        println!("Allocated IP: {} to {}", ip_resource.address, ip_resource.allocated_to.as_ref().unwrap());
    } // IP automatically returned to pool when dropped
    
    // Create database connection pool
    let db_factory = Arc::new(DatabaseConnectionFactory::new("postgres://localhost/blixard".to_string()));
    let db_config = PoolConfig {
        max_size: 20,
        min_size: 5,
        validate_on_acquire: true,
        ..Default::default()
    };
    let db_pool = ResourcePool::new(db_factory, db_config);
    db_pool.initialize().await?;
    
    // Use a database connection
    {
        let connection = db_pool.acquire().await?;
        println!("Got database connection #{}", connection.id);
        // Use connection...
    } // Connection returned to pool
    
    // Create worker pool
    let worker_factory = Arc::new(WorkerFactory::new());
    let worker_config = PoolConfig {
        max_size: 4, // 4 parallel workers
        min_size: 2,
        ..Default::default()
    };
    let worker_pool = ResourcePool::new(worker_factory, worker_config);
    worker_pool.initialize().await?;
    
    // Process tasks in parallel
    let mut handles = vec![];
    for i in 0..10 {
        let pool = worker_pool.clone();
        let handle = tokio::spawn(async move {
            let mut worker = pool.acquire().await?;
            worker.current_task = Some(format!("task-{}", i));
            
            // Simulate work
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            worker.tasks_completed += 1;
            worker.current_task = None;
            
            Ok::<_, BlixardError>(())
        });
        handles.push(handle);
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await??;
    }
    
    // Print pool statistics
    println!("\nPool Statistics:");
    println!("IP Pool: {:?}", ip_pool.stats().await);
    println!("DB Pool: {:?}", db_pool.stats().await);
    println!("Worker Pool: {:?}", worker_pool.stats().await);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_ip_address_pool() {
        let factory = Arc::new(IpAddressFactory::new("10.0.0.0", 10).unwrap());
        let config = PoolConfig {
            max_size: 5,
            ..Default::default()
        };
        
        let pool = ResourcePool::new(factory, config);
        
        // Acquire multiple IPs
        let ip1 = pool.acquire().await.unwrap();
        let ip2 = pool.acquire().await.unwrap();
        
        assert_eq!(ip1.address.to_string(), "10.0.0.1");
        assert_eq!(ip2.address.to_string(), "10.0.0.2");
        
        // Return and reacquire
        drop(ip1);
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let ip3 = pool.acquire().await.unwrap();
        assert_eq!(ip3.address.to_string(), "10.0.0.1"); // Reused
    }
    
    #[tokio::test]
    async fn test_worker_pool_concurrency() {
        let factory = Arc::new(WorkerFactory::new());
        let config = PoolConfig {
            max_size: 2, // Only 2 workers
            acquire_timeout: tokio::time::Duration::from_millis(100),
            ..Default::default()
        };
        
        let pool = ResourcePool::new(factory, config);
        
        // Try to acquire 3 workers (should fail on 3rd)
        let w1 = pool.acquire().await.unwrap();
        let w2 = pool.acquire().await.unwrap();
        let w3_result = pool.acquire().await;
        
        assert!(w3_result.is_err());
        assert!(matches!(w3_result, Err(BlixardError::Timeout { .. })));
        
        drop(w1);
        drop(w2);
    }
}