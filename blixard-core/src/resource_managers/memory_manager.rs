//! Memory Resource Manager Implementation
//!
//! This module provides a concrete implementation of the ResourceManager trait
//! specifically for memory resource management, unifying the functionality
//! previously scattered across quota_manager.rs and resource_management.rs.

use crate::error::{BlixardError, BlixardResult};
use crate::resource_manager::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Mutex};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// Memory-specific resource manager
#[derive(Debug)]
pub struct MemoryResourceManager {
    /// Manager configuration
    config: ResourceManagerConfig,
    /// Current allocations
    allocations: Arc<RwLock<HashMap<ResourceId, ResourceAllocation>>>,
    /// Current usage tracking
    usage: Arc<RwLock<ResourceUsage>>,
    /// Runtime state
    state: Arc<RwLock<ManagerState>>,
    /// Background monitoring task handle
    monitoring_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Background health check task handle
    health_check_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Manager metrics
    metrics: Arc<RwLock<ResourceManagerMetrics>>,
}

#[derive(Debug)]
struct ManagerState {
    is_running: bool,
    health: ResourceManagerHealth,
    last_health_check: SystemTime,
    total_allocations: u64,
    total_deallocations: u64,
    failed_allocations: u64,
    allocation_times: Vec<Duration>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            is_running: false,
            health: ResourceManagerHealth::Stopped,
            last_health_check: SystemTime::now(),
            total_allocations: 0,
            total_deallocations: 0,
            failed_allocations: 0,
            allocation_times: Vec::new(),
        }
    }
}

impl MemoryResourceManager {
    /// Create a new memory resource manager
    pub fn new(mut config: ResourceManagerConfig) -> Self {
        // Ensure we're managing memory resources
        config.resource_type = ResourceType::Memory;

        let initial_usage = ResourceUsage {
            resource_type: ResourceType::Memory,
            total_capacity: config.limits.hard_limit,
            allocated_amount: 0,
            used_amount: 0,
            allocation_count: 0,
            updated_at: SystemTime::now(),
        };

        let initial_metrics = ResourceManagerMetrics {
            resource_type: ResourceType::Memory,
            total_allocations: 0,
            total_deallocations: 0,
            failed_allocations: 0,
            avg_allocation_time_ms: 0.0,
            health: ResourceManagerHealth::Stopped,
            last_health_check: SystemTime::now(),
            custom_metrics: HashMap::new(),
        };

        Self {
            config,
            allocations: Arc::new(RwLock::new(HashMap::new())),
            usage: Arc::new(RwLock::new(initial_usage)),
            state: Arc::new(RwLock::new(ManagerState::default())),
            monitoring_handle: Arc::new(Mutex::new(None)),
            health_check_handle: Arc::new(Mutex::new(None)),
            metrics: Arc::new(RwLock::new(initial_metrics)),
        }
    }

    /// Create with builder pattern
    pub fn builder() -> MemoryResourceManagerBuilder {
        MemoryResourceManagerBuilder::new()
    }

    /// Calculate effective capacity considering overcommit policy
    fn effective_capacity(&self) -> u64 {
        let limits = &self.config.limits;
        if limits.allow_overcommit {
            ((limits.hard_limit as f64) * limits.overcommit_ratio) as u64
        } else {
            limits.hard_limit
        }
    }

    /// Calculate available memory considering system reserve
    fn available_capacity(&self, current_allocated: u64) -> u64 {
        let effective = self.effective_capacity();
        let reserve = self.config.limits.system_reserve;
        effective.saturating_sub(current_allocated).saturating_sub(reserve)
    }

    /// Check if allocation request would violate limits
    async fn validate_allocation_request(&self, request: &ResourceAllocationRequest) -> BlixardResult<()> {
        // Check resource type
        if request.resource_type != ResourceType::Memory {
            return Err(BlixardError::InvalidInput {
                field: "resource_type".to_string(),
                message: "Memory manager can only handle memory resources".to_string(),
            });
        }

        let usage = self.usage.read().await;
        let available = self.available_capacity(usage.allocated_amount);

        // Check if request fits in available capacity
        if request.amount > available {
            return Err(BlixardError::InsufficientResources {
                requested: format!("{}MB memory", request.amount),
                available: format!("{}MB available", available),
            });
        }

        // Check soft limit
        if usage.allocated_amount + request.amount > self.config.limits.soft_limit {
            warn!(
                "Memory allocation for {} exceeds soft limit: {}MB requested, {}MB soft limit",
                request.tenant_id, request.amount, self.config.limits.soft_limit
            );
        }

        Ok(())
    }

    /// Update usage statistics
    async fn update_usage_stats(&self) {
        let allocations = self.allocations.read().await;
        let mut usage = self.usage.write().await;

        let mut total_allocated = 0;
        let mut active_count = 0;

        for allocation in allocations.values() {
            if allocation.status == AllocationStatus::Active || allocation.status == AllocationStatus::Reserved {
                total_allocated += allocation.allocated_amount;
                active_count += 1;
            }
        }

        usage.allocated_amount = total_allocated;
        usage.allocation_count = active_count;
        usage.updated_at = SystemTime::now();

        // Update actual used memory (in production, this would query system metrics)
        usage.used_amount = self.estimate_actual_usage(total_allocated).await;
    }

    /// Estimate actual memory usage (placeholder for real system monitoring)
    async fn estimate_actual_usage(&self, allocated: u64) -> u64 {
        // In production, this would use system APIs to get real memory usage
        // For now, estimate that 70-90% of allocated memory is actually used
        let usage_ratio = 0.75 + (rand::random::<f64>() * 0.15); // 75-90%
        (allocated as f64 * usage_ratio) as u64
    }

    /// Start background monitoring task
    async fn start_monitoring(&self) -> BlixardResult<()> {
        if !self.config.enable_monitoring {
            return Ok(());
        }

        let usage = self.usage.clone();
        let allocations = self.allocations.clone();
        let manager_state = self.state.clone();
        let manager_metrics = self.metrics.clone();
        let interval_duration = self.config.monitoring_interval;

        let handle = tokio::spawn(async move {
            let mut timer = interval(interval_duration);
            
            loop {
                timer.tick().await;
                
                // Update usage statistics
                let allocated = {
                    let allocations_guard = allocations.read().await;
                    allocations_guard.values()
                        .filter(|a| a.status == AllocationStatus::Active || a.status == AllocationStatus::Reserved)
                        .map(|a| a.allocated_amount)
                        .sum::<u64>()
                };

                {
                    let mut usage_guard = usage.write().await;
                    usage_guard.allocated_amount = allocated;
                    usage_guard.allocation_count = allocations.read().await.len();
                    usage_guard.updated_at = SystemTime::now();
                    
                    // Simulate real usage monitoring
                    usage_guard.used_amount = (allocated as f64 * 0.8) as u64;
                }

                // Update metrics
                {
                    let state = manager_state.read().await;
                    let mut metrics = manager_metrics.write().await;
                    metrics.total_allocations = state.total_allocations;
                    metrics.total_deallocations = state.total_deallocations;
                    metrics.failed_allocations = state.failed_allocations;
                    
                    if !state.allocation_times.is_empty() {
                        let total_time: Duration = state.allocation_times.iter().sum();
                        metrics.avg_allocation_time_ms = total_time.as_millis() as f64 / state.allocation_times.len() as f64;
                    }
                }

                debug!("Memory usage updated: {}MB allocated, {}MB used", allocated, allocated * 8 / 10);
            }
        });

        *self.monitoring_handle.lock().await = Some(handle);
        info!("Memory monitoring started with interval: {:?}", interval_duration);
        Ok(())
    }

    /// Start background health check task
    async fn start_health_checks(&self) -> BlixardResult<()> {
        if !self.config.enable_health_checks {
            return Ok(());
        }

        let state = self.state.clone();
        let usage = self.usage.clone();
        let interval_duration = self.config.health_check_interval;

        let handle = tokio::spawn(async move {
            let mut timer = interval(interval_duration);
            
            loop {
                timer.tick().await;
                
                let health_status = {
                    let usage_guard = usage.read().await;
                    let utilization = usage_guard.utilization_percent();
                    
                    if utilization > 95.0 {
                        ResourceManagerHealth::Unhealthy("Memory utilization critically high".to_string())
                    } else if utilization > 85.0 || usage_guard.is_overallocated() {
                        ResourceManagerHealth::Degraded(format!("Memory utilization at {:.1}%", utilization))
                    } else {
                        ResourceManagerHealth::Healthy
                    }
                };

                {
                    let mut state_guard = state.write().await;
                    state_guard.health = health_status;
                    state_guard.last_health_check = SystemTime::now();
                }

                debug!("Memory health check completed");
            }
        });

        *self.health_check_handle.lock().await = Some(handle);
        info!("Memory health checks started with interval: {:?}", interval_duration);
        Ok(())
    }

    /// Stop background tasks
    async fn stop_background_tasks(&self) {
        if let Some(handle) = self.monitoring_handle.lock().await.take() {
            handle.abort();
            info!("Memory monitoring stopped");
        }

        if let Some(handle) = self.health_check_handle.lock().await.take() {
            handle.abort();
            info!("Memory health checks stopped");
        }
    }
}

#[async_trait]
impl ResourceManager for MemoryResourceManager {
    fn resource_type(&self) -> ResourceType {
        ResourceType::Memory
    }

    fn config(&self) -> &ResourceManagerConfig {
        &self.config
    }

    async fn allocate(&self, request: ResourceAllocationRequest) -> BlixardResult<ResourceAllocation> {
        let start_time = SystemTime::now();
        
        // Validate the request
        self.validate_allocation_request(&request).await?;

        // Create allocation
        let allocation = ResourceAllocation {
            allocation_id: format!("mem-{}-{}", request.tenant_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            request: request.clone(),
            allocated_amount: request.amount,
            allocated_on: Some("memory".to_string()),
            allocated_at: SystemTime::now(),
            status: AllocationStatus::Active,
        };

        // Store allocation
        {
            let mut allocations = self.allocations.write().await;
            allocations.insert(allocation.allocation_id.clone(), allocation.clone());
        }

        // Update usage and state
        self.update_usage_stats().await;
        
        {
            let mut state = self.state.write().await;
            state.total_allocations += 1;
            
            if let Ok(duration) = start_time.elapsed() {
                state.allocation_times.push(duration);
                // Keep only last 100 allocation times for rolling average
                if state.allocation_times.len() > 100 {
                    state.allocation_times.remove(0);
                }
            }
        }

        info!(
            "Memory allocated: {}MB for tenant {} (allocation_id: {})",
            request.amount, request.tenant_id, allocation.allocation_id
        );

        Ok(allocation)
    }

    async fn deallocate(&self, allocation_id: &ResourceId) -> BlixardResult<()> {
        let allocation = {
            let mut allocations = self.allocations.write().await;
            allocations.remove(allocation_id)
        };

        if let Some(mut allocation) = allocation {
            allocation.status = AllocationStatus::Released;
            
            // Update usage statistics
            self.update_usage_stats().await;
            
            // Update state
            {
                let mut state = self.state.write().await;
                state.total_deallocations += 1;
            }

            info!(
                "Memory deallocated: {}MB for tenant {} (allocation_id: {})",
                allocation.allocated_amount, allocation.request.tenant_id, allocation_id
            );

            Ok(())
        } else {
            Err(BlixardError::ResourceUnavailable {
                resource_type: "memory".to_string(),
                message: format!("Allocation {} not found", allocation_id),
            })
        }
    }

    async fn check_availability(&self, request: &ResourceAllocationRequest) -> BlixardResult<bool> {
        match self.validate_allocation_request(request).await {
            Ok(_) => Ok(true),
            Err(BlixardError::InsufficientResources { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn get_usage(&self) -> BlixardResult<ResourceUsage> {
        Ok(self.usage.read().await.clone())
    }

    async fn get_allocations(&self) -> BlixardResult<Vec<ResourceAllocation>> {
        let allocations = self.allocations.read().await;
        Ok(allocations.values().cloned().collect())
    }

    async fn get_allocation(&self, allocation_id: &ResourceId) -> BlixardResult<Option<ResourceAllocation>> {
        let allocations = self.allocations.read().await;
        Ok(allocations.get(allocation_id).cloned())
    }

    async fn initialize(&self) -> BlixardResult<()> {
        info!("Initializing memory resource manager");
        
        // Initialize usage tracking
        let mut usage = self.usage.write().await;
        usage.total_capacity = self.config.limits.hard_limit;
        usage.updated_at = SystemTime::now();
        drop(usage);

        // Initialize state
        let mut state = self.state.write().await;
        state.health = ResourceManagerHealth::Healthy;
        state.last_health_check = SystemTime::now();
        drop(state);

        info!("Memory resource manager initialized with capacity: {}MB", self.config.limits.hard_limit);
        Ok(())
    }

    async fn start(&self) -> BlixardResult<()> {
        info!("Starting memory resource manager");
        
        // Start background tasks
        self.start_monitoring().await?;
        self.start_health_checks().await?;
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.health = ResourceManagerHealth::Healthy;
        }

        info!("Memory resource manager started successfully");
        Ok(())
    }

    async fn stop(&self) -> BlixardResult<()> {
        info!("Stopping memory resource manager");
        
        // Stop background tasks
        self.stop_background_tasks().await;
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.is_running = false;
            state.health = ResourceManagerHealth::Stopped;
        }

        info!("Memory resource manager stopped");
        Ok(())
    }

    async fn health_check(&self) -> BlixardResult<ResourceManagerHealth> {
        let state = self.state.read().await;
        Ok(state.health.clone())
    }

    async fn is_running(&self) -> bool {
        let state = self.state.read().await;
        state.is_running
    }

    async fn get_metrics(&self) -> BlixardResult<ResourceManagerMetrics> {
        let metrics = self.metrics.read().await;
        Ok(metrics.clone())
    }

    async fn get_status(&self) -> BlixardResult<ResourceManagerStatus> {
        Ok(ResourceManagerStatus {
            resource_type: self.resource_type(),
            health: self.health_check().await?,
            is_running: self.is_running().await,
            usage: self.get_usage().await?,
            limits: self.get_limits().await?,
            metrics: self.get_metrics().await?,
            status_time: SystemTime::now(),
        })
    }

    async fn export_telemetry(&self) -> BlixardResult<HashMap<String, serde_json::Value>> {
        let mut telemetry = HashMap::new();
        
        let usage = self.get_usage().await?;
        let metrics = self.get_metrics().await?;
        let state = self.state.read().await;

        telemetry.insert("resource_type".to_string(), serde_json::json!("memory"));
        telemetry.insert("total_capacity_mb".to_string(), serde_json::json!(usage.total_capacity));
        telemetry.insert("allocated_mb".to_string(), serde_json::json!(usage.allocated_amount));
        telemetry.insert("used_mb".to_string(), serde_json::json!(usage.used_amount));
        telemetry.insert("utilization_percent".to_string(), serde_json::json!(usage.utilization_percent()));
        telemetry.insert("allocation_count".to_string(), serde_json::json!(usage.allocation_count));
        telemetry.insert("total_allocations".to_string(), serde_json::json!(metrics.total_allocations));
        telemetry.insert("total_deallocations".to_string(), serde_json::json!(metrics.total_deallocations));
        telemetry.insert("failed_allocations".to_string(), serde_json::json!(metrics.failed_allocations));
        telemetry.insert("avg_allocation_time_ms".to_string(), serde_json::json!(metrics.avg_allocation_time_ms));
        telemetry.insert("health_status".to_string(), serde_json::json!(format!("{:?}", state.health)));
        telemetry.insert("is_running".to_string(), serde_json::json!(state.is_running));

        Ok(telemetry)
    }

    async fn update_limits(&self, limits: ResourceLimits) -> BlixardResult<()> {
        if limits.resource_type != ResourceType::Memory {
            return Err(BlixardError::InvalidInput {
                field: "resource_type".to_string(),
                message: "Limits must be for memory resource type".to_string(),
            });
        }

        // Note: In a real implementation, we'd need to handle config mutability differently
        info!("Memory limits update requested: hard={}, soft={}, overcommit={}",
            limits.hard_limit, limits.soft_limit, limits.overcommit_ratio);
        
        // Update usage capacity
        {
            let mut usage = self.usage.write().await;
            usage.total_capacity = limits.hard_limit;
            usage.updated_at = SystemTime::now();
        }

        Ok(())
    }

    async fn get_limits(&self) -> BlixardResult<ResourceLimits> {
        Ok(self.config.limits.clone())
    }

    async fn validate_config(&self, config: &ResourceManagerConfig) -> BlixardResult<()> {
        if config.resource_type != ResourceType::Memory {
            return Err(BlixardError::InvalidInput {
                field: "resource_type".to_string(),
                message: "Configuration must be for memory resource type".to_string(),
            });
        }

        if config.limits.hard_limit == 0 {
            return Err(BlixardError::InvalidInput {
                field: "hard_limit".to_string(),
                message: "Hard limit must be greater than 0".to_string(),
            });
        }

        if config.limits.soft_limit > config.limits.hard_limit {
            return Err(BlixardError::InvalidInput {
                field: "soft_limit".to_string(),
                message: "Soft limit cannot exceed hard limit".to_string(),
            });
        }

        if config.limits.overcommit_ratio < 1.0 {
            return Err(BlixardError::InvalidInput {
                field: "overcommit_ratio".to_string(),
                message: "Overcommit ratio must be at least 1.0".to_string(),
            });
        }

        Ok(())
    }

    async fn update_config(&self, config: ResourceManagerConfig) -> BlixardResult<()> {
        self.validate_config(&config).await?;
        
        // Note: In a real implementation, this would properly update the config
        info!("Memory manager configuration update validated");
        Ok(())
    }
}

/// Builder for MemoryResourceManager
pub struct MemoryResourceManagerBuilder {
    config: ResourceManagerConfig,
}

impl MemoryResourceManagerBuilder {
    pub fn new() -> Self {
        Self {
            config: ResourceManagerConfigBuilder::new(ResourceType::Memory).build(),
        }
    }

    pub fn with_capacity(mut self, capacity_mb: u64) -> Self {
        self.config.limits.hard_limit = capacity_mb;
        self.config.limits.soft_limit = (capacity_mb as f64 * 0.8) as u64; // 80% soft limit
        self
    }

    pub fn with_overcommit(mut self, ratio: f64) -> Self {
        self.config.limits.allow_overcommit = true;
        self.config.limits.overcommit_ratio = ratio;
        self
    }

    pub fn with_monitoring(mut self, enabled: bool, interval: Duration) -> Self {
        self.config.enable_monitoring = enabled;
        self.config.monitoring_interval = interval;
        self
    }

    pub fn with_health_checks(mut self, enabled: bool, interval: Duration) -> Self {
        self.config.enable_health_checks = enabled;
        self.config.health_check_interval = interval;
        self
    }

    pub fn build(self) -> MemoryResourceManager {
        MemoryResourceManager::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_manager_creation() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(8192)
            .with_overcommit(1.5)
            .build();

        assert_eq!(manager.resource_type(), ResourceType::Memory);
        assert_eq!(manager.config.limits.hard_limit, 8192);
        assert_eq!(manager.config.limits.overcommit_ratio, 1.5);
    }

    #[tokio::test]
    async fn test_memory_allocation_and_deallocation() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(1024)
            .build();

        manager.initialize().await.unwrap();

        let request = ResourceAllocationRequest {
            request_id: "test-req-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            resource_type: ResourceType::Memory,
            amount: 512,
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: HashMap::new(),
        };

        // Test allocation
        let allocation = manager.allocate(request.clone()).await.unwrap();
        assert_eq!(allocation.allocated_amount, 512);
        assert_eq!(allocation.status, AllocationStatus::Active);

        // Check usage
        let usage = manager.get_usage().await.unwrap();
        assert_eq!(usage.allocated_amount, 512);
        assert_eq!(usage.allocation_count, 1);

        // Test deallocation
        manager.deallocate(&allocation.allocation_id).await.unwrap();
        
        let usage_after = manager.get_usage().await.unwrap();
        assert_eq!(usage_after.allocated_amount, 0);
        assert_eq!(usage_after.allocation_count, 0);
    }

    #[tokio::test]
    async fn test_memory_overallocation_rejection() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(1024)
            .build();

        manager.initialize().await.unwrap();

        let large_request = ResourceAllocationRequest {
            request_id: "test-req-large".to_string(),
            tenant_id: "tenant-1".to_string(),
            resource_type: ResourceType::Memory,
            amount: 2048, // More than capacity
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: HashMap::new(),
        };

        let result = manager.allocate(large_request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BlixardError::InsufficientResources { .. }));
    }

    #[tokio::test]
    async fn test_memory_manager_lifecycle() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(1024)
            .with_monitoring(true, Duration::from_millis(100))
            .build();

        // Test initialization
        manager.initialize().await.unwrap();
        
        // Test starting
        manager.start().await.unwrap();
        assert!(manager.is_running().await);

        // Test health check
        let health = manager.health_check().await.unwrap();
        assert_eq!(health, ResourceManagerHealth::Healthy);

        // Test stopping
        manager.stop().await.unwrap();
        assert!(!manager.is_running().await);
    }

    #[tokio::test]
    async fn test_memory_usage_calculations() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(1000)
            .build();

        manager.initialize().await.unwrap();

        let usage = manager.get_usage().await.unwrap();
        assert_eq!(usage.utilization_percent(), 0.0);
        assert_eq!(usage.allocation_percent(), 0.0);
        assert!(!usage.is_overallocated());
    }

    #[tokio::test]
    async fn test_memory_manager_metrics() {
        let manager = MemoryResourceManager::builder()
            .with_capacity(1024)
            .build();

        manager.initialize().await.unwrap();

        let metrics = manager.get_metrics().await.unwrap();
        assert_eq!(metrics.resource_type, ResourceType::Memory);
        assert_eq!(metrics.total_allocations, 0);
        assert_eq!(metrics.failed_allocations, 0);
    }
}