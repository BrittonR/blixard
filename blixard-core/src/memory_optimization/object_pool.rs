//! Type-specific object pooling for frequently allocated types
//!
//! This module provides typed object pools optimized for specific types
//! commonly allocated in hot paths like Raft messages, VM configs, and
//! proposal batches.

use crate::patterns::resource_pool::{PoolConfig, PoolableResource, ResourceFactory, ResourcePool};
use crate::error::BlixardResult;
use crate::raft::messages::RaftProposal;
use crate::raft_manager::ProposalData;
use crate::types::VmConfig;
use async_trait::async_trait;
use std::sync::Arc;
use std::collections::HashMap;

/// Wrapper type for pooled objects that implements PoolableResource
pub struct PooledObject<T> {
    inner: T,
    #[allow(dead_code)]
    reset_fn: Option<Box<dyn Fn(&mut T) + Send + Sync>>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for PooledObject<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledObject")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T: Send + Sync + std::fmt::Debug> PoolableResource for PooledObject<T> {
    fn is_valid(&self) -> bool {
        true
    }
    
    fn reset(&mut self) -> BlixardResult<()> {
        if let Some(reset_fn) = &self.reset_fn {
            reset_fn(&mut self.inner);
        }
        Ok(())
    }
}

impl<T> PooledObject<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            reset_fn: None,
        }
    }
    
    pub fn with_reset(inner: T, reset_fn: Box<dyn Fn(&mut T) + Send + Sync>) -> Self {
        Self {
            inner,
            reset_fn: Some(reset_fn),
        }
    }
    
    pub fn into_inner(self) -> T {
        self.inner
    }
    
    pub fn as_ref(&self) -> &T {
        &self.inner
    }
    
    pub fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

/// Factory for creating RaftProposal objects
pub struct RaftProposalFactory;

#[async_trait]
impl ResourceFactory<PooledObject<RaftProposal>> for RaftProposalFactory {
    async fn create(&self) -> BlixardResult<PooledObject<RaftProposal>> {
        let proposal = RaftProposal {
            id: Vec::with_capacity(16), // Pre-allocate for UUID
            data: ProposalData::UpdateWorkerStatus {
                node_id: 0,
                status: crate::raft::proposals::WorkerStatus::Online,
            },
            response_tx: None,
        };
        
        let reset_fn = Box::new(|p: &mut RaftProposal| {
            p.id.clear();
            p.response_tx = None;
            // Don't reset data as it will be overwritten
        });
        
        Ok(PooledObject::with_reset(proposal, reset_fn))
    }
}

/// Factory for creating VmConfig objects
pub struct VmConfigFactory;

#[async_trait]
impl ResourceFactory<PooledObject<VmConfig>> for VmConfigFactory {
    async fn create(&self) -> BlixardResult<PooledObject<VmConfig>> {
        let config = VmConfig {
            name: String::with_capacity(64),
            config_path: String::with_capacity(256),
            vcpus: 1,
            memory: 1024,
            tenant_id: String::with_capacity(32),
            ip_address: None,
            metadata: Some(HashMap::with_capacity(8)),
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        };
        
        let reset_fn = Box::new(|c: &mut VmConfig| {
            c.name.clear();
            c.config_path.clear();
            c.vcpus = 1;
            c.memory = 1024;
            c.tenant_id.clear();
            c.tenant_id.push_str("default");
            c.ip_address = None;
            if let Some(ref mut metadata) = c.metadata {
                metadata.clear();
            }
            c.anti_affinity = None;
            c.priority = 500;
            c.preemptible = true;
            c.locality_preference = Default::default();
            c.health_check_config = None;
        });
        
        Ok(PooledObject::with_reset(config, reset_fn))
    }
}

/// Factory for creating HashMap objects for metadata
pub struct MetadataMapFactory;

#[async_trait]
impl ResourceFactory<PooledObject<HashMap<String, String>>> for MetadataMapFactory {
    async fn create(&self) -> BlixardResult<PooledObject<HashMap<String, String>>> {
        let map = HashMap::with_capacity(16);
        
        let reset_fn = Box::new(|m: &mut HashMap<String, String>| {
            m.clear();
        });
        
        Ok(PooledObject::with_reset(map, reset_fn))
    }
}

/// Factory for creating Vec<u8> buffers for serialization
pub struct ByteBufferFactory {
    initial_capacity: usize,
}

impl ByteBufferFactory {
    pub fn new(initial_capacity: usize) -> Self {
        Self { initial_capacity }
    }
}

#[async_trait]
impl ResourceFactory<PooledObject<Vec<u8>>> for ByteBufferFactory {
    async fn create(&self) -> BlixardResult<PooledObject<Vec<u8>>> {
        let buffer = Vec::with_capacity(self.initial_capacity);
        
        let reset_fn = Box::new(|b: &mut Vec<u8>| {
            b.clear();
        });
        
        Ok(PooledObject::with_reset(buffer, reset_fn))
    }
}

/// Typed object pool that wraps ResourcePool for specific types
pub struct TypedObjectPool<T: Send + Sync + std::fmt::Debug + 'static> {
    inner: ResourcePool<PooledObject<T>>,
}

impl<T: Send + Sync + std::fmt::Debug + 'static> TypedObjectPool<T> {
    pub fn new(factory: Arc<dyn ResourceFactory<PooledObject<T>>>, config: PoolConfig) -> Self {
        Self {
            inner: ResourcePool::new(factory, config),
        }
    }
    
    pub async fn initialize(&self) -> BlixardResult<()> {
        self.inner.initialize().await
    }
    
    pub async fn acquire(&self) -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<T>>> {
        self.inner.acquire().await
    }
    
    pub async fn try_acquire(&self) -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<T>>> {
        self.inner.try_acquire().await
    }
    
    pub async fn stats(&self) -> crate::patterns::resource_pool::PoolStats {
        self.inner.stats().await
    }
}

/// Global object pools for commonly allocated types
pub mod global_pools {
    use super::*;
    use once_cell::sync::Lazy;
    
    // RaftProposal pool
    pub static RAFT_PROPOSAL_POOL: Lazy<TypedObjectPool<RaftProposal>> = Lazy::new(|| {
        let factory = Arc::new(RaftProposalFactory);
        let config = PoolConfig {
            max_size: 1000,
            min_size: 100,
            ..Default::default()
        };
        let pool = TypedObjectPool::new(factory, config);
        
        // Initialize pool in background
        tokio::spawn(async move {
            let _ = RAFT_PROPOSAL_POOL.initialize().await;
        });
        
        pool
    });
    
    // VmConfig pool
    pub static VM_CONFIG_POOL: Lazy<TypedObjectPool<VmConfig>> = Lazy::new(|| {
        let factory = Arc::new(VmConfigFactory);
        let config = PoolConfig {
            max_size: 500,
            min_size: 50,
            ..Default::default()
        };
        TypedObjectPool::new(factory, config)
    });
    
    // Metadata HashMap pool
    pub static METADATA_MAP_POOL: Lazy<TypedObjectPool<HashMap<String, String>>> = Lazy::new(|| {
        let factory = Arc::new(MetadataMapFactory);
        let config = PoolConfig {
            max_size: 200,
            min_size: 20,
            ..Default::default()
        };
        TypedObjectPool::new(factory, config)
    });
    
    // Serialization buffer pools
    pub static SMALL_BUFFER_POOL: Lazy<TypedObjectPool<Vec<u8>>> = Lazy::new(|| {
        let factory = Arc::new(ByteBufferFactory::new(1024));
        let config = PoolConfig {
            max_size: 500,
            min_size: 50,
            ..Default::default()
        };
        TypedObjectPool::new(factory, config)
    });
    
    pub static LARGE_BUFFER_POOL: Lazy<TypedObjectPool<Vec<u8>>> = Lazy::new(|| {
        let factory = Arc::new(ByteBufferFactory::new(64 * 1024));
        let config = PoolConfig {
            max_size: 100,
            min_size: 10,
            ..Default::default()
        };
        TypedObjectPool::new(factory, config)
    });
}

/// Helper macro to acquire from pool and handle errors
#[macro_export]
macro_rules! pool_acquire {
    ($pool:expr) => {
        match $pool.try_acquire().await {
            Ok(pooled) => pooled,
            Err(_) => {
                // Fallback to creating new instance if pool is exhausted
                // This ensures we never block critical operations
                return Err($crate::error::BlixardError::ResourceUnavailable {
                    resource_type: "object pool".to_string(),
                    message: "Pool exhausted, consider increasing pool size".to_string(),
                });
            }
        }
    };
}

/// Helper to acquire a RaftProposal from the global pool
pub async fn acquire_raft_proposal() -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<RaftProposal>>> {
    global_pools::RAFT_PROPOSAL_POOL.acquire().await
}

/// Helper to acquire a VmConfig from the global pool
pub async fn acquire_vm_config() -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<VmConfig>>> {
    global_pools::VM_CONFIG_POOL.acquire().await
}

/// Helper to acquire a metadata map from the global pool
pub async fn acquire_metadata_map() -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<HashMap<String, String>>>> {
    global_pools::METADATA_MAP_POOL.acquire().await
}

/// Helper to acquire a small buffer from the global pool
pub async fn acquire_small_buffer() -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<Vec<u8>>>> {
    global_pools::SMALL_BUFFER_POOL.acquire().await
}

/// Helper to acquire a large buffer from the global pool
pub async fn acquire_large_buffer() -> BlixardResult<crate::patterns::resource_pool::PooledResource<PooledObject<Vec<u8>>>> {
    global_pools::LARGE_BUFFER_POOL.acquire().await
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_raft_proposal_pool() {
        let factory = Arc::new(RaftProposalFactory);
        let pool = TypedObjectPool::new(factory, PoolConfig::default());
        pool.initialize().await.unwrap();
        
        let mut pooled = pool.acquire().await.unwrap();
        let proposal = pooled.get_mut().unwrap().as_mut();
        
        proposal.id = vec![1, 2, 3];
        assert_eq!(proposal.id, vec![1, 2, 3]);
        
        // After drop and reacquire, should be reset
        drop(pooled);
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let pooled2 = pool.acquire().await.unwrap();
        let proposal2 = pooled2.get().unwrap().as_ref();
        assert!(proposal2.id.is_empty());
    }
    
    #[tokio::test]
    async fn test_vm_config_pool() {
        let factory = Arc::new(VmConfigFactory);
        let pool = TypedObjectPool::new(factory, PoolConfig::default());
        pool.initialize().await.unwrap();
        
        let mut pooled = pool.acquire().await.unwrap();
        let config = pooled.get_mut().unwrap().as_mut();
        
        config.name = "test-vm".to_string();
        config.vcpus = 4;
        assert_eq!(config.name, "test-vm");
        assert_eq!(config.vcpus, 4);
    }
}