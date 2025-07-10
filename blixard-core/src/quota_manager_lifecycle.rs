//! LifecycleManager implementation for QuotaManager - Proof of Concept
//!
//! This module demonstrates how to refactor an existing Manager to use the unified
//! LifecycleManager pattern. This serves as a template for migrating other managers.

use crate::error::{BlixardError, BlixardResult};
use crate::patterns::{LifecycleManager, LifecycleBase, LifecycleState, LifecycleStats, HealthStatus};
use crate::resource_quotas::*;
use crate::raft_storage::Storage;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock as AsyncRwLock;
use tokio::time::{interval, Interval};
use tracing::{debug, info, warn};

/// Configuration for QuotaManager following the unified configuration pattern
#[derive(Clone, Debug)]
pub struct QuotaManagerConfig {
    /// Storage backend for persistence
    pub storage: Arc<dyn Storage>,
    /// Cleanup interval for rate limiting in seconds (default: 30)
    pub cleanup_interval_secs: u64,
    /// Rate limit window in seconds (default: 60)
    pub rate_limit_window_secs: u64,
    /// Whether to enable background cleanup task (default: true)
    pub enable_cleanup_task: bool,
}

impl QuotaManagerConfig {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            cleanup_interval_secs: 30,
            rate_limit_window_secs: 60,
            enable_cleanup_task: true,
        }
    }

    pub fn with_cleanup_interval(mut self, seconds: u64) -> Self {
        self.cleanup_interval_secs = seconds;
        self
    }

    pub fn with_rate_limit_window(mut self, seconds: u64) -> Self {
        self.rate_limit_window_secs = seconds;
        self
    }

    pub fn disable_cleanup_task(mut self) -> Self {
        self.enable_cleanup_task = false;
        self
    }
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

/// Enhanced QuotaManager with LifecycleManager pattern
#[derive(Debug)]
pub struct LifecycleQuotaManager {
    /// Lifecycle management state
    lifecycle: LifecycleBase<QuotaManagerConfig>,

    /// Tenant quotas configuration
    quotas: Arc<RwLock<HashMap<TenantId, TenantQuota>>>,

    /// Current tenant usage
    usage: Arc<RwLock<HashMap<TenantId, TenantUsage>>>,

    /// Rate limiting state
    rate_limits: Arc<AsyncRwLock<RateLimitState>>,

    /// Background cleanup task handle
    cleanup_task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl LifecycleQuotaManager {
    /// Helper to safely acquire read lock for quotas
    fn read_quotas(&self) -> BlixardResult<RwLockReadGuard<HashMap<TenantId, TenantQuota>>> {
        self.quotas.read().map_err(|_| BlixardError::Internal {
            message: "Failed to acquire quota read lock - lock poisoned".to_string(),
        })
    }

    /// Helper to safely acquire write lock for quotas
    fn write_quotas(&self) -> BlixardResult<RwLockWriteGuard<HashMap<TenantId, TenantQuota>>> {
        self.quotas.write().map_err(|_| BlixardError::Internal {
            message: "Failed to acquire quota write lock - lock poisoned".to_string(),
        })
    }

    /// Helper to safely acquire usage read lock
    fn read_usage(&self) -> BlixardResult<RwLockReadGuard<HashMap<TenantId, TenantUsage>>> {
        self.usage.read().map_err(|_| BlixardError::Internal {
            message: "Failed to acquire usage read lock - lock poisoned".to_string(),
        })
    }

    /// Helper to safely acquire usage write lock
    fn write_usage(&self) -> BlixardResult<RwLockWriteGuard<HashMap<TenantId, TenantUsage>>> {
        self.usage.write().map_err(|_| BlixardError::Internal {
            message: "Failed to acquire usage write lock - lock poisoned".to_string(),
        })
    }

    /// Load quotas from storage backend
    async fn load_quotas_from_storage(&self) -> BlixardResult<()> {
        info!("Loading quotas from storage backend");
        
        match self.lifecycle.config.storage.get_all_quotas().await {
            Ok(stored_quotas) => {
                let mut quotas = self.write_quotas()?;
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

    /// Start background cleanup task for expired rate limit entries
    async fn start_cleanup_task(&mut self) -> BlixardResult<()> {
        if !self.lifecycle.config.enable_cleanup_task {
            info!("Cleanup task disabled in configuration");
            return Ok(());
        }

        if self.cleanup_task_handle.is_some() {
            warn!("Cleanup task already running");
            return Ok(());
        }

        info!("Starting quota cleanup task with interval of {} seconds", 
              self.lifecycle.config.cleanup_interval_secs);

        let cleanup_interval = Duration::from_secs(self.lifecycle.config.cleanup_interval_secs);
        let rate_limit_window = Duration::from_secs(self.lifecycle.config.rate_limit_window_secs);
        let rate_limits = Arc::clone(&self.rate_limits);

        let handle = tokio::spawn(async move {
            let mut interval = interval(cleanup_interval);
            
            loop {
                interval.tick().await;
                Self::cleanup_expired_rate_limits(&rate_limits, rate_limit_window).await;
            }
        });

        self.cleanup_task_handle = Some(handle);
        info!("Quota cleanup task started successfully");
        Ok(())
    }

    /// Stop the background cleanup task
    async fn stop_cleanup_task(&mut self) -> BlixardResult<()> {
        if let Some(handle) = self.cleanup_task_handle.take() {
            info!("Stopping quota cleanup task");
            handle.abort();
            
            // Wait for task to finish (it should abort quickly)
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(result) => {
                    match result {
                        Ok(_) => info!("Cleanup task stopped gracefully"),
                        Err(_) => info!("Cleanup task was aborted as expected"),
                    }
                }
                Err(_) => {
                    warn!("Cleanup task did not stop within timeout, continuing anyway");
                }
            }
        } else {
            debug!("No cleanup task to stop");
        }
        Ok(())
    }

    /// Clean up expired rate limit entries
    async fn cleanup_expired_rate_limits(
        rate_limits: &Arc<AsyncRwLock<RateLimitState>>,
        rate_limit_window: Duration,
    ) {
        let mut state = rate_limits.write().await;
        let cutoff_time = SystemTime::now() - (rate_limit_window * 2);

        for (_, tracking) in state.tenant_requests.iter_mut() {
            // Clean up old request times
            tracking.request_times.retain(|&time| time > cutoff_time);

            // Clean up operation tracking
            for (_, operation_times) in tracking.operation_tracking.iter_mut() {
                operation_times.retain(|&time| time > cutoff_time);
            }

            // Remove empty operation tracking
            tracking
                .operation_tracking
                .retain(|_, times| !times.is_empty());
        }

        // Remove empty tenant tracking
        state.tenant_requests.retain(|_, tracking| {
            !tracking.request_times.is_empty()
                || tracking.concurrent_requests > 0
                || !tracking.operation_tracking.is_empty()
        });

        debug!(
            "Cleaned up rate limit state, {} tenants remaining",
            state.tenant_requests.len()
        );
    }

    /// Business logic methods (unchanged from original QuotaManager)
    
    /// Set quota for a tenant
    pub async fn set_tenant_quota(&self, quota: TenantQuota) -> BlixardResult<()> {
        let tenant_id = quota.tenant_id.clone();

        // Store in memory
        {
            let mut quotas = self.write_quotas()?;
            quotas.insert(tenant_id.clone(), quota.clone());
        }

        // Persist to storage
        self.lifecycle.config.storage.save_tenant_quota(&quota).await?;

        info!(
            "Set quota for tenant {}: max_vms={}, max_vcpus={}, max_memory_mb={}",
            tenant_id,
            quota.vm_limits.max_vms,
            quota.vm_limits.max_vcpus,
            quota.vm_limits.max_memory_mb
        );

        Ok(())
    }

    /// Get quota for a tenant (creates default if not exists)
    pub async fn get_tenant_quota(&self, tenant_id: &str) -> TenantQuota {
        {
            if let Ok(quotas) = self.read_quotas() {
                if let Some(quota) = quotas.get(tenant_id) {
                    return quota.clone();
                }
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
            let mut quotas = self.write_quotas()?;
            let mut usage = self.write_usage()?;
            quotas.remove(tenant_id);
            usage.remove(tenant_id);
        }

        // Remove from storage
        self.lifecycle.config.storage.delete_tenant_quota(tenant_id).await?;

        info!("Removed quota for tenant {}", tenant_id);
        Ok(())
    }

    /// Check if a resource request would exceed quotas
    pub async fn check_resource_quota(&self, request: &ResourceRequest) -> QuotaResult<()> {
        let quota = self.get_tenant_quota(&request.tenant_id).await;

        if !quota.enabled {
            return Ok(());
        }

        // Get current usage for tenant
        let current_usage = {
            if let Ok(usage_map) = self.read_usage() {
                usage_map
                    .get(&request.tenant_id)
                    .cloned()
                    .unwrap_or_else(|| TenantUsage::new(request.tenant_id.clone()))
            } else {
                return Err(QuotaError::InternalError(
                    "Failed to read usage data".to_string(),
                ));
            }
        };

        // Check VM limits
        if let Some(vm_request) = &request.vm_request {
            let vm_limits = &quota.vm_limits;
            let vm_usage = &current_usage.vm_usage;

            if vm_usage.active_vms >= vm_limits.max_vms {
                return Err(QuotaError::VmQuotaExceeded {
                    current: vm_usage.active_vms,
                    limit: vm_limits.max_vms,
                    resource: "vm_count".to_string(),
                });
            }

            if vm_usage.total_vcpus + vm_request.vcpus > vm_limits.max_vcpus {
                return Err(QuotaError::VmQuotaExceeded {
                    current: vm_usage.total_vcpus,
                    limit: vm_limits.max_vcpus,
                    resource: "vcpus".to_string(),
                });
            }

            if vm_usage.total_memory_mb + vm_request.memory_mb > vm_limits.max_memory_mb {
                return Err(QuotaError::VmQuotaExceeded {
                    current: vm_usage.total_memory_mb,
                    limit: vm_limits.max_memory_mb,
                    resource: "memory".to_string(),
                });
            }
        }

        Ok(())
    }
}

/// Implement the LifecycleManager trait for QuotaManager
#[async_trait]
impl LifecycleManager for LifecycleQuotaManager {
    type Config = QuotaManagerConfig;
    type State = (); // QuotaManager doesn't need complex shared state
    type Error = BlixardError;

    /// Create a new quota manager instance (factory method)
    async fn new(config: Self::Config) -> Result<Self, Self::Error> {
        info!("Creating new LifecycleQuotaManager");

        let lifecycle = LifecycleBase::new(config);
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
            lifecycle,
            quotas,
            usage,
            rate_limits,
            cleanup_task_handle: None,
        };

        manager.lifecycle.transition_state(LifecycleState::Created);
        info!("LifecycleQuotaManager created successfully");
        Ok(manager)
    }

    /// Initialize the manager by loading data from storage
    async fn initialize(&mut self) -> Result<(), Self::Error> {
        if self.lifecycle.state != LifecycleState::Created {
            return Err(BlixardError::Internal {
                message: format!("Cannot initialize QuotaManager from state {:?}", self.lifecycle.state),
            });
        }

        info!("Initializing QuotaManager");
        self.lifecycle.transition_state(LifecycleState::Initializing);

        // Load quotas from storage
        self.load_quotas_from_storage().await.map_err(|e| {
            self.lifecycle.record_error(&format!("Failed to load quotas: {}", e));
            e
        })?;

        self.lifecycle.transition_state(LifecycleState::Ready);
        info!("QuotaManager initialized successfully");
        Ok(())
    }

    /// Start the manager and begin background operations
    async fn start(&mut self) -> Result<(), Self::Error> {
        if !matches!(self.lifecycle.state, LifecycleState::Ready | LifecycleState::Stopped) {
            return Err(BlixardError::Internal {
                message: format!("Cannot start QuotaManager from state {:?}", self.lifecycle.state),
            });
        }

        info!("Starting QuotaManager");
        self.lifecycle.transition_state(LifecycleState::Starting);

        // Start background cleanup task
        self.start_cleanup_task().await.map_err(|e| {
            self.lifecycle.record_error(&format!("Failed to start cleanup task: {}", e));
            e
        })?;

        self.lifecycle.transition_state(LifecycleState::Running);
        info!("QuotaManager started successfully");
        Ok(())
    }

    /// Stop the manager gracefully
    async fn stop(&mut self) -> Result<(), Self::Error> {
        if !matches!(self.lifecycle.state, LifecycleState::Running | LifecycleState::Starting) {
            return Err(BlixardError::Internal {
                message: format!("Cannot stop QuotaManager from state {:?}", self.lifecycle.state),
            });
        }

        info!("Stopping QuotaManager");
        self.lifecycle.transition_state(LifecycleState::Stopping);

        // Stop background cleanup task
        self.stop_cleanup_task().await.map_err(|e| {
            self.lifecycle.record_error(&format!("Failed to stop cleanup task: {}", e));
            e
        })?;

        self.lifecycle.transition_state(LifecycleState::Stopped);
        info!("QuotaManager stopped successfully");
        Ok(())
    }

    /// Get current lifecycle state
    fn state(&self) -> LifecycleState {
        self.lifecycle.state.clone()
    }

    /// Get lifecycle statistics
    fn stats(&self) -> LifecycleStats {
        self.lifecycle.stats.clone()
    }

    /// Get manager name
    fn name(&self) -> &'static str {
        "QuotaManager"
    }

    /// Get current configuration
    fn config(&self) -> &Self::Config {
        &self.lifecycle.config
    }

    /// Get health status with domain-specific checks
    async fn health(&self) -> HealthStatus {
        match self.state() {
            LifecycleState::Running => {
                // Additional health checks for quota manager
                let quota_count = self.read_quotas()
                    .map(|quotas| quotas.len())
                    .unwrap_or(0);

                let usage_count = self.read_usage()
                    .map(|usage| usage.len())
                    .unwrap_or(0);

                // Check if cleanup task is still running
                let cleanup_healthy = self.cleanup_task_handle
                    .as_ref()
                    .map(|handle| !handle.is_finished())
                    .unwrap_or(!self.lifecycle.config.enable_cleanup_task);

                if !cleanup_healthy {
                    HealthStatus::Degraded("Cleanup task has stopped".to_string())
                } else {
                    HealthStatus::Healthy
                }
            }
            LifecycleState::Failed(ref err) => HealthStatus::Unhealthy(err.clone()),
            _ => HealthStatus::Degraded("Not running".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_quota_manager_lifecycle() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage);

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Created);

        // Initialize
        manager.initialize().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Ready);

        // Start
        manager.start().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);
        assert!(manager.is_running());
        assert!(manager.is_healthy().await);

        // Stop
        manager.stop().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Stopped);
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_quota_manager_restart() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage);

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();

        // Restart
        manager.restart().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);

        // Should have recorded a restart
        let stats = manager.stats();
        assert_eq!(stats.restart_count, 1);
    }

    #[tokio::test]
    async fn test_quota_manager_without_cleanup_task() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage).disable_cleanup_task();

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();

        assert_eq!(manager.state(), LifecycleState::Running);
        assert!(manager.cleanup_task_handle.is_none());
    }

    #[tokio::test]
    async fn test_quota_manager_business_logic() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage).disable_cleanup_task();

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();

        // Test quota operations
        let tenant_id = "test-tenant";
        let quota = TenantQuota::new(tenant_id.to_string());
        
        manager.set_tenant_quota(quota.clone()).await.unwrap();
        let retrieved_quota = manager.get_tenant_quota(tenant_id).await;
        assert_eq!(retrieved_quota.tenant_id, tenant_id);

        manager.remove_tenant_quota(tenant_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_quota_manager_state_transitions() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage);

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();

        // Can't start without initializing
        assert!(manager.start().await.is_err());

        // Initialize then start
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();

        // Can't initialize when running
        assert!(manager.initialize().await.is_err());

        // Can stop when running
        manager.stop().await.unwrap();

        // Can start again after stopping
        manager.start().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);
    }

    #[tokio::test]
    async fn test_quota_manager_cleanup_task_lifecycle() {
        let (storage, _temp_dir) = test_helpers::create_test_storage().await;
        let config = QuotaManagerConfig::new(storage)
            .with_cleanup_interval(1); // 1 second for fast testing

        let mut manager = LifecycleQuotaManager::new(config).await.unwrap();
        manager.initialize().await.unwrap();
        manager.start().await.unwrap();

        // Cleanup task should be running
        assert!(manager.cleanup_task_handle.is_some());
        let handle = manager.cleanup_task_handle.as_ref().unwrap();
        assert!(!handle.is_finished());

        // Stop should gracefully shut down the task
        manager.stop().await.unwrap();
        assert!(manager.cleanup_task_handle.is_none());
    }
}