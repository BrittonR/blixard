//! Quota Manager - Central component for resource quota enforcement
//!
//! This module provides the QuotaManager which:
//! - Tracks tenant resource usage in real-time
//! - Enforces quotas before resource allocation
//! - Provides rate limiting for API operations
//! - Manages per-tenant and per-node limits

use crate::error::{BlixardError, BlixardResult};
use crate::resource_quotas::*;
use crate::storage::Storage;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as AsyncRwLock;
use tokio::time::{interval, Interval};
use tracing::{debug, warn, error, info};
use serde::{Deserialize, Serialize};

/// Time window for rate limiting calculations
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_CLEANUP_INTERVAL_SECS: u64 = 30;

/// In-memory quota and usage tracking
#[derive(Debug)]
pub struct QuotaManager {
    /// Tenant quotas configuration
    quotas: Arc<RwLock<HashMap<TenantId, TenantQuota>>>,
    
    /// Current tenant usage
    usage: Arc<RwLock<HashMap<TenantId, TenantUsage>>>,
    
    /// Rate limiting state
    rate_limits: Arc<AsyncRwLock<RateLimitState>>,
    
    /// Storage backend for persistence
    storage: Arc<dyn Storage>,
    
    /// Cleanup timer for expired rate limit entries
    cleanup_interval: Option<Interval>,
}

/// Rate limiting tracking state
#[derive(Debug)]
struct RateLimitState {
    /// Per-tenant API request tracking
    tenant_requests: HashMap<TenantId, TenantRequestTracking>,
    
    /// Global rate limiting (if enabled)
    global_requests: GlobalRequestTracking,
}

/// Request tracking for a specific tenant
#[derive(Debug)]
struct TenantRequestTracking {
    /// Recent request timestamps
    request_times: Vec<SystemTime>,
    
    /// Current concurrent requests
    concurrent_requests: u32,
    
    /// Per-operation request tracking
    operation_tracking: HashMap<ApiOperation, Vec<SystemTime>>,
}

/// Global request tracking across all tenants
#[derive(Debug)]
struct GlobalRequestTracking {
    /// Total requests per second globally
    global_rps: u32,
    
    /// Last reset timestamp
    last_reset: SystemTime,
}

impl QuotaManager {
    /// Create a new quota manager
    pub async fn new(storage: Arc<dyn Storage>) -> BlixardResult<Self> {
        let quotas = Arc::new(RwLock::new(HashMap::new()));
        let usage = Arc::new(RwLock::new(HashMap::new()));
        let rate_limits = Arc::new(AsyncRwLock::new(RateLimitState {
            tenant_requests: HashMap::new(),
            global_requests: GlobalRequestTracking {
                global_rps: 0,
                last_reset: SystemTime::now(),
            },
        }));
        
        let mut manager = Self {
            quotas,
            usage,
            rate_limits,
            storage,
            cleanup_interval: None,
        };
        
        // Load existing quotas from storage
        manager.load_quotas_from_storage().await?;
        
        // Start cleanup task
        manager.start_cleanup_task().await;
        
        Ok(manager)
    }
    
    /// Set quota for a tenant
    pub async fn set_tenant_quota(&self, quota: TenantQuota) -> BlixardResult<()> {
        let tenant_id = quota.tenant_id.clone();
        
        // Store in memory
        {
            let mut quotas = self.quotas.write().unwrap();
            quotas.insert(tenant_id.clone(), quota.clone());
        }
        
        // Persist to storage
        self.save_quota_to_storage(&quota).await?;
        
        info!("Set quota for tenant {}: max_vms={}, max_vcpus={}, max_memory_mb={}", 
              tenant_id, quota.vm_limits.max_vms, quota.vm_limits.max_vcpus, quota.vm_limits.max_memory_mb);
        
        Ok(())
    }
    
    /// Get quota for a tenant (creates default if not exists)
    pub async fn get_tenant_quota(&self, tenant_id: &str) -> TenantQuota {
        {
            let quotas = self.quotas.read().unwrap();
            if let Some(quota) = quotas.get(tenant_id) {
                return quota.clone();
            }
        }
        
        // Create default quota
        let default_quota = TenantQuota::new(tenant_id.to_string());
        
        // Try to set it (ignore errors for race conditions)
        let _ = self.set_tenant_quota(default_quota.clone()).await;
        
        default_quota
    }
    
    /// Remove tenant quota
    pub async fn remove_tenant_quota(&self, tenant_id: &str) -> BlixardResult<()> {
        // Remove from memory
        {
            let mut quotas = self.quotas.write().unwrap();
            let mut usage = self.usage.write().unwrap();
            quotas.remove(tenant_id);
            usage.remove(tenant_id);
        }
        
        // Remove from storage
        self.delete_quota_from_storage(tenant_id).await?;
        
        info!("Removed quota for tenant {}", tenant_id);
        Ok(())
    }
    
    /// Check if a resource request would exceed quotas
    pub async fn check_resource_quota(&self, request: &ResourceRequest) -> QuotaResult<()> {
        let quota = self.get_tenant_quota(&request.tenant_id).await;
        
        if !quota.enabled {
            return Ok(());
        }
        
        let current_usage = self.get_tenant_usage(&request.tenant_id).await;
        
        // Check VM count limit
        if current_usage.vm_usage.active_vms >= quota.vm_limits.max_vms {
            return Err(QuotaViolation::VmLimitExceeded {
                limit: quota.vm_limits.max_vms,
                current: current_usage.vm_usage.active_vms,
                requested: 1,
            });
        }
        
        // Check CPU limit
        let total_vcpus = current_usage.vm_usage.used_vcpus + request.vcpus;
        if total_vcpus > quota.vm_limits.max_vcpus {
            return Err(QuotaViolation::CpuLimitExceeded {
                limit: quota.vm_limits.max_vcpus,
                current: current_usage.vm_usage.used_vcpus,
                requested: request.vcpus,
            });
        }
        
        // Check memory limit
        let total_memory = current_usage.vm_usage.used_memory_mb + request.memory_mb;
        if total_memory > quota.vm_limits.max_memory_mb {
            return Err(QuotaViolation::MemoryLimitExceeded {
                limit: quota.vm_limits.max_memory_mb,
                current: current_usage.vm_usage.used_memory_mb,
                requested: request.memory_mb,
            });
        }
        
        // Check disk limit
        let total_disk = current_usage.vm_usage.used_disk_gb + request.disk_gb;
        if total_disk > quota.vm_limits.max_disk_gb {
            return Err(QuotaViolation::DiskLimitExceeded {
                limit: quota.vm_limits.max_disk_gb,
                current: current_usage.vm_usage.used_disk_gb,
                requested: request.disk_gb,
            });
        }
        
        // Check per-node limit if node is specified
        if let Some(node_id) = request.node_id {
            let current_vms_on_node = current_usage.vm_usage.vms_per_node.get(&node_id).unwrap_or(&0);
            if *current_vms_on_node >= quota.vm_limits.max_vms_per_node {
                return Err(QuotaViolation::PerNodeVmLimitExceeded {
                    node_id,
                    limit: quota.vm_limits.max_vms_per_node,
                    current: *current_vms_on_node,
                });
            }
        }
        
        Ok(())
    }
    
    /// Check API rate limits for a tenant
    pub async fn check_rate_limit(&self, tenant_id: &str, operation: &ApiOperation) -> QuotaResult<()> {
        let quota = self.get_tenant_quota(tenant_id).await;
        
        if !quota.enabled {
            return Ok(());
        }
        
        let mut rate_limits = self.rate_limits.write().await;
        let now = SystemTime::now();
        
        // Get or create tenant tracking
        let tenant_tracking = rate_limits.tenant_requests
            .entry(tenant_id.to_string())
            .or_insert_with(|| TenantRequestTracking {
                request_times: Vec::new(),
                concurrent_requests: 0,
                operation_tracking: HashMap::new(),
            });
        
        // Clean old requests (older than rate limit window)
        let cutoff_time = now - Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
        tenant_tracking.request_times.retain(|&time| time > cutoff_time);
        
        // Check general rate limits
        let requests_in_window = tenant_tracking.request_times.len() as u32;
        if requests_in_window >= quota.api_limits.requests_per_second * RATE_LIMIT_WINDOW_SECS as u32 {
            return Err(QuotaViolation::RateLimitExceeded {
                operation: "general".to_string(),
                limit: quota.api_limits.requests_per_second,
                current: requests_in_window,
            });
        }
        
        // Check concurrent request limit
        if tenant_tracking.concurrent_requests >= quota.api_limits.max_concurrent_requests {
            return Err(QuotaViolation::RateLimitExceeded {
                operation: "concurrent".to_string(),
                limit: quota.api_limits.max_concurrent_requests,
                current: tenant_tracking.concurrent_requests,
            });
        }
        
        // Check operation-specific limits
        let operation_times = tenant_tracking.operation_tracking
            .entry(operation.clone())
            .or_insert_with(Vec::new);
        
        operation_times.retain(|&time| time > cutoff_time);
        
        let operation_limit = match operation {
            ApiOperation::VmCreate => quota.api_limits.operation_limits.vm_create_per_minute,
            ApiOperation::VmDelete => quota.api_limits.operation_limits.vm_delete_per_minute,
            ApiOperation::ClusterJoin => quota.api_limits.operation_limits.cluster_join_per_hour / 60, // Per minute
            ApiOperation::StatusQuery => quota.api_limits.operation_limits.status_query_per_second * 60, // Per minute
            ApiOperation::ConfigChange => quota.api_limits.operation_limits.config_change_per_hour / 60, // Per minute
            _ => 1000, // High default for other operations
        };
        
        if operation_times.len() as u32 >= operation_limit {
            return Err(QuotaViolation::RateLimitExceeded {
                operation: operation.to_string(),
                limit: operation_limit,
                current: operation_times.len() as u32,
            });
        }
        
        Ok(())
    }
    
    /// Record an API request for rate limiting
    pub async fn record_api_request(&self, tenant_id: &str, operation: &ApiOperation) {
        let mut rate_limits = self.rate_limits.write().await;
        let now = SystemTime::now();
        
        let tenant_tracking = rate_limits.tenant_requests
            .entry(tenant_id.to_string())
            .or_insert_with(|| TenantRequestTracking {
                request_times: Vec::new(),
                concurrent_requests: 0,
                operation_tracking: HashMap::new(),
            });
        
        // Record general request
        tenant_tracking.request_times.push(now);
        tenant_tracking.concurrent_requests += 1;
        
        // Record operation-specific request
        tenant_tracking.operation_tracking
            .entry(operation.clone())
            .or_insert_with(Vec::new)
            .push(now);
    }
    
    /// Record end of API request (for concurrent tracking)
    pub async fn record_api_request_end(&self, tenant_id: &str) {
        let mut rate_limits = self.rate_limits.write().await;
        
        if let Some(tenant_tracking) = rate_limits.tenant_requests.get_mut(tenant_id) {
            tenant_tracking.concurrent_requests = tenant_tracking.concurrent_requests.saturating_sub(1);
        }
    }
    
    /// Update tenant resource usage
    pub async fn update_resource_usage(
        &self, 
        tenant_id: &str, 
        vcpus: i32, 
        memory_mb: i64, 
        disk_gb: i64, 
        node_id: u64
    ) -> BlixardResult<()> {
        let mut usage_guard = self.usage.write().unwrap();
        let tenant_usage = usage_guard
            .entry(tenant_id.to_string())
            .or_insert_with(|| TenantUsage::new(tenant_id.to_string()));
        
        tenant_usage.update_vm_usage(vcpus, memory_mb, disk_gb, node_id);
        
        debug!("Updated usage for tenant {}: vcpus={}, memory_mb={}, disk_gb={}, node_id={}", 
               tenant_id, vcpus, memory_mb, disk_gb, node_id);
        
        Ok(())
    }
    
    /// Get current tenant usage
    pub async fn get_tenant_usage(&self, tenant_id: &str) -> TenantUsage {
        let usage = self.usage.read().unwrap();
        usage.get(tenant_id)
            .cloned()
            .unwrap_or_else(|| TenantUsage::new(tenant_id.to_string()))
    }
    
    /// Get usage for all tenants
    pub async fn get_all_usage(&self) -> HashMap<TenantId, TenantUsage> {
        let usage = self.usage.read().unwrap();
        usage.clone()
    }
    
    /// Get quota for all tenants
    pub async fn get_all_quotas(&self) -> HashMap<TenantId, TenantQuota> {
        let quotas = self.quotas.read().unwrap();
        quotas.clone()
    }
    
    /// Reset usage statistics (for testing or maintenance)
    pub async fn reset_tenant_usage(&self, tenant_id: &str) -> BlixardResult<()> {
        let mut usage = self.usage.write().unwrap();
        usage.insert(tenant_id.to_string(), TenantUsage::new(tenant_id.to_string()));
        
        let mut rate_limits = self.rate_limits.write().await;
        rate_limits.tenant_requests.remove(tenant_id);
        
        info!("Reset usage statistics for tenant {}", tenant_id);
        Ok(())
    }
    
    /// Load quotas from storage backend
    async fn load_quotas_from_storage(&self) -> BlixardResult<()> {
        // Implementation depends on storage backend
        // For now, we'll load from a "quotas" table/collection
        match self.storage.get_all_quotas().await {
            Ok(stored_quotas) => {
                let mut quotas = self.quotas.write().unwrap();
                for quota in stored_quotas {
                    quotas.insert(quota.tenant_id.clone(), quota);
                }
                info!("Loaded {} quotas from storage", quotas.len());
            }
            Err(e) => {
                warn!("Failed to load quotas from storage: {}", e);
                // Continue with empty quotas - they'll be created as needed
            }
        }
        Ok(())
    }
    
    /// Save quota to storage backend
    async fn save_quota_to_storage(&self, quota: &TenantQuota) -> BlixardResult<()> {
        self.storage.save_tenant_quota(quota).await
    }
    
    /// Delete quota from storage backend
    async fn delete_quota_from_storage(&self, tenant_id: &str) -> BlixardResult<()> {
        self.storage.delete_tenant_quota(tenant_id).await
    }
    
    /// Start background cleanup task for expired rate limit entries
    async fn start_cleanup_task(&mut self) {
        let mut interval = interval(Duration::from_secs(RATE_LIMIT_CLEANUP_INTERVAL_SECS));
        let rate_limits = Arc::clone(&self.rate_limits);
        
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                Self::cleanup_expired_rate_limits(&rate_limits).await;
            }
        });
    }
    
    /// Clean up expired rate limit entries
    async fn cleanup_expired_rate_limits(rate_limits: &Arc<AsyncRwLock<RateLimitState>>) {
        let mut state = rate_limits.write().await;
        let cutoff_time = SystemTime::now() - Duration::from_secs(RATE_LIMIT_WINDOW_SECS * 2);
        
        for (tenant_id, tracking) in state.tenant_requests.iter_mut() {
            // Clean up old request times
            tracking.request_times.retain(|&time| time > cutoff_time);
            
            // Clean up operation tracking
            for (_, operation_times) in tracking.operation_tracking.iter_mut() {
                operation_times.retain(|&time| time > cutoff_time);
            }
            
            // Remove empty operation tracking
            tracking.operation_tracking.retain(|_, times| !times.is_empty());
        }
        
        // Remove empty tenant tracking
        state.tenant_requests.retain(|_, tracking| {
            !tracking.request_times.is_empty() || 
            tracking.concurrent_requests > 0 || 
            !tracking.operation_tracking.is_empty()
        });
        
        debug!("Cleaned up rate limit state, {} tenants remaining", state.tenant_requests.len());
    }
}

