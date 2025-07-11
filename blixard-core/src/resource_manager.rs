//! Unified Resource Manager trait abstraction
//!
//! This module provides a unified interface for all resource management operations,
//! eliminating code duplication across quota management, resource monitoring,
//! admission control, and collection modules.
//!
//! ## Design Goals
//!
//! 1. **Unified Interface**: Common operations for all resource types
//! 2. **Type Safety**: Generic resource types with proper bounds
//! 3. **Lifecycle Management**: Consistent initialization, start/stop, health checks
//! 4. **Observability**: Built-in metrics and telemetry export
//! 5. **Factory Pattern**: Easy creation of concrete implementations
//! 6. **Composition**: Support for managing multiple resource types
//!
//! ## Architecture
//!
//! ```text
//! ResourceManager Trait
//! ├── Core Operations: allocate, deallocate, check_availability, get_usage
//! ├── Lifecycle: initialize, start, stop, health_check
//! ├── Monitoring: get_metrics, get_status, export_telemetry
//! ├── Configuration: update_limits, get_limits, validate_config
//! └── Utilities: Factory, Builder, Composite patterns
//! ```

use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Generic resource identifier type
pub type ResourceId = String;

/// Generic tenant identifier type
pub type TenantId = String;

/// Resource type identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Memory,
    Cpu,
    Disk,
    Network,
    Gpu,
    Custom(String),
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Memory => write!(f, "memory"),
            ResourceType::Cpu => write!(f, "cpu"),
            ResourceType::Disk => write!(f, "disk"),
            ResourceType::Network => write!(f, "network"),
            ResourceType::Gpu => write!(f, "gpu"),
            ResourceType::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Generic resource allocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocationRequest {
    /// Unique identifier for this request
    pub request_id: ResourceId,
    /// Tenant making the request
    pub tenant_id: TenantId,
    /// Type of resource being requested
    pub resource_type: ResourceType,
    /// Amount of resource requested (in resource-specific units)
    pub amount: u64,
    /// Priority level (higher = more important)
    pub priority: u32,
    /// Whether this is a temporary or permanent allocation
    pub is_temporary: bool,
    /// Optional expiration time for temporary allocations
    pub expires_at: Option<SystemTime>,
    /// Additional metadata for the request
    pub metadata: HashMap<String, String>,
}

/// Resource allocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Unique identifier for this allocation
    pub allocation_id: ResourceId,
    /// The original request
    pub request: ResourceAllocationRequest,
    /// Actual amount allocated (may differ from requested)
    pub allocated_amount: u64,
    /// Node/location where resource was allocated
    pub allocated_on: Option<String>,
    /// Timestamp when allocation was created
    pub allocated_at: SystemTime,
    /// Current status of the allocation
    pub status: AllocationStatus,
}

/// Status of a resource allocation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AllocationStatus {
    /// Allocation is active and in use
    Active,
    /// Allocation is reserved but not yet active
    Reserved,
    /// Allocation is being released
    Releasing,
    /// Allocation has been released
    Released,
    /// Allocation failed or is invalid
    Failed(String),
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Type of resource
    pub resource_type: ResourceType,
    /// Total capacity available
    pub total_capacity: u64,
    /// Currently allocated amount
    pub allocated_amount: u64,
    /// Actually used amount (may differ from allocated)
    pub used_amount: u64,
    /// Number of active allocations
    pub allocation_count: usize,
    /// Last updated timestamp
    pub updated_at: SystemTime,
}

impl ResourceUsage {
    /// Calculate utilization percentage (0.0 - 100.0)
    pub fn utilization_percent(&self) -> f64 {
        if self.total_capacity == 0 {
            0.0
        } else {
            (self.used_amount as f64 / self.total_capacity as f64) * 100.0
        }
    }

    /// Calculate allocation percentage (0.0 - 100.0)
    pub fn allocation_percent(&self) -> f64 {
        if self.total_capacity == 0 {
            0.0
        } else {
            (self.allocated_amount as f64 / self.total_capacity as f64) * 100.0
        }
    }

    /// Check if resource is overallocated
    pub fn is_overallocated(&self) -> bool {
        self.allocated_amount > self.total_capacity
    }
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Type of resource these limits apply to
    pub resource_type: ResourceType,
    /// Hard limit (cannot be exceeded)
    pub hard_limit: u64,
    /// Soft limit (warning threshold)
    pub soft_limit: u64,
    /// Whether to allow overcommit beyond hard limit
    pub allow_overcommit: bool,
    /// Overcommit ratio (e.g., 1.5 = 150% of hard limit)
    pub overcommit_ratio: f64,
    /// Minimum reserved amount for system operations
    pub system_reserve: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            resource_type: ResourceType::Memory,
            hard_limit: u64::MAX,
            soft_limit: u64::MAX / 2,
            allow_overcommit: false,
            overcommit_ratio: 1.0,
            system_reserve: 0,
        }
    }
}

/// Resource manager health status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResourceManagerHealth {
    /// Manager is healthy and operational
    Healthy,
    /// Manager is degraded but functional
    Degraded(String),
    /// Manager is unhealthy and may not function properly
    Unhealthy(String),
    /// Manager is stopped or unavailable
    Stopped,
}

/// Resource manager metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerMetrics {
    /// Resource type being managed
    pub resource_type: ResourceType,
    /// Total number of allocations processed
    pub total_allocations: u64,
    /// Total number of deallocations processed
    pub total_deallocations: u64,
    /// Number of failed allocation attempts
    pub failed_allocations: u64,
    /// Average allocation time in milliseconds
    pub avg_allocation_time_ms: f64,
    /// Current health status
    pub health: ResourceManagerHealth,
    /// Last health check timestamp
    pub last_health_check: SystemTime,
    /// Additional metrics specific to resource type
    pub custom_metrics: HashMap<String, f64>,
}

/// Configuration for resource managers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerConfig {
    /// Type of resource being managed
    pub resource_type: ResourceType,
    /// Resource limits
    pub limits: ResourceLimits,
    /// Whether to enable real-time monitoring
    pub enable_monitoring: bool,
    /// Monitoring interval
    pub monitoring_interval: Duration,
    /// Whether to enable automatic health checks
    pub enable_health_checks: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Whether to export telemetry data
    pub enable_telemetry: bool,
    /// Custom configuration options
    pub custom_config: HashMap<String, String>,
}

impl Default for ResourceManagerConfig {
    fn default() -> Self {
        Self {
            resource_type: ResourceType::Memory,
            limits: ResourceLimits::default(),
            enable_monitoring: true,
            monitoring_interval: Duration::from_secs(30),
            enable_health_checks: true,
            health_check_interval: Duration::from_secs(60),
            enable_telemetry: false,
            custom_config: HashMap::new(),
        }
    }
}

/// Main ResourceManager trait - unified interface for all resource management
#[async_trait]
pub trait ResourceManager: Send + Sync + Debug {
    /// Get the type of resource this manager handles
    fn resource_type(&self) -> ResourceType;

    /// Get manager configuration
    fn config(&self) -> &ResourceManagerConfig;

    // === Core Resource Operations ===

    /// Allocate resources for a request
    async fn allocate(&self, request: ResourceAllocationRequest) -> BlixardResult<ResourceAllocation>;

    /// Deallocate resources by allocation ID
    async fn deallocate(&self, allocation_id: &ResourceId) -> BlixardResult<()>;

    /// Check if resources are available for a request
    async fn check_availability(&self, request: &ResourceAllocationRequest) -> BlixardResult<bool>;

    /// Get current resource usage
    async fn get_usage(&self) -> BlixardResult<ResourceUsage>;

    /// Get all active allocations
    async fn get_allocations(&self) -> BlixardResult<Vec<ResourceAllocation>>;

    /// Get allocation by ID
    async fn get_allocation(&self, allocation_id: &ResourceId) -> BlixardResult<Option<ResourceAllocation>>;

    // === Lifecycle Management ===

    /// Initialize the resource manager
    async fn initialize(&self) -> BlixardResult<()>;

    /// Start the resource manager (begin monitoring, health checks)
    async fn start(&self) -> BlixardResult<()>;

    /// Stop the resource manager
    async fn stop(&self) -> BlixardResult<()>;

    /// Perform health check
    async fn health_check(&self) -> BlixardResult<ResourceManagerHealth>;

    /// Check if manager is running
    async fn is_running(&self) -> bool;

    // === Monitoring and Observability ===

    /// Get current metrics
    async fn get_metrics(&self) -> BlixardResult<ResourceManagerMetrics>;

    /// Get detailed status information
    async fn get_status(&self) -> BlixardResult<ResourceManagerStatus>;

    /// Export telemetry data
    async fn export_telemetry(&self) -> BlixardResult<HashMap<String, serde_json::Value>>;

    // === Configuration Management ===

    /// Update resource limits
    async fn update_limits(&self, limits: ResourceLimits) -> BlixardResult<()>;

    /// Get current limits
    async fn get_limits(&self) -> BlixardResult<ResourceLimits>;

    /// Validate configuration
    async fn validate_config(&self, config: &ResourceManagerConfig) -> BlixardResult<()>;

    /// Update configuration
    async fn update_config(&self, config: ResourceManagerConfig) -> BlixardResult<()>;

    // === Optional Advanced Operations ===

    /// Reserve resources without allocating (default implementation)
    async fn reserve(&self, request: ResourceAllocationRequest) -> BlixardResult<ResourceAllocation> {
        // Default implementation: create allocation with Reserved status
        let mut allocation = self.allocate(request).await?;
        allocation.status = AllocationStatus::Reserved;
        Ok(allocation)
    }

    /// Activate a reserved allocation (default implementation)
    async fn activate_reservation(&self, allocation_id: &ResourceId) -> BlixardResult<()> {
        // Default implementation: no-op (override for specific behavior)
        debug!("Activating reservation {} (default implementation)", allocation_id);
        Ok(())
    }

    /// Get resource efficiency metrics (default implementation)
    async fn get_efficiency_metrics(&self) -> BlixardResult<ResourceEfficiencyMetrics> {
        let usage = self.get_usage().await?;
        Ok(ResourceEfficiencyMetrics {
            resource_type: usage.resource_type.clone(),
            allocation_efficiency: if usage.allocated_amount > 0 {
                usage.used_amount as f64 / usage.allocated_amount as f64
            } else {
                0.0
            },
            utilization_efficiency: usage.utilization_percent() / 100.0,
            fragmentation_ratio: if usage.allocation_count > 0 {
                usage.allocated_amount as f64 / (usage.allocation_count as f64 * 1024.0)
            } else {
                0.0
            },
            last_calculated: SystemTime::now(),
        })
    }
}

/// Resource manager status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerStatus {
    /// Resource type being managed
    pub resource_type: ResourceType,
    /// Current health status
    pub health: ResourceManagerHealth,
    /// Whether manager is running
    pub is_running: bool,
    /// Current resource usage
    pub usage: ResourceUsage,
    /// Current limits
    pub limits: ResourceLimits,
    /// Manager metrics
    pub metrics: ResourceManagerMetrics,
    /// Timestamp when status was generated
    pub status_time: SystemTime,
}

/// Resource efficiency metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEfficiencyMetrics {
    /// Resource type
    pub resource_type: ResourceType,
    /// Allocation efficiency (used / allocated)
    pub allocation_efficiency: f64,
    /// Utilization efficiency (used / total capacity)
    pub utilization_efficiency: f64,
    /// Fragmentation ratio (measure of allocation fragmentation)
    pub fragmentation_ratio: f64,
    /// When these metrics were calculated
    pub last_calculated: SystemTime,
}

/// Factory trait for creating resource managers
#[async_trait]
pub trait ResourceManagerFactory: Send + Sync {
    /// Create a new resource manager instance
    async fn create(
        &self,
        config: ResourceManagerConfig,
    ) -> BlixardResult<Box<dyn ResourceManager>>;

    /// Get supported resource types
    fn supported_types(&self) -> Vec<ResourceType>;

    /// Validate configuration for a resource type
    async fn validate_config(
        &self,
        resource_type: &ResourceType,
        config: &ResourceManagerConfig,
    ) -> BlixardResult<()>;
}

/// Builder for resource manager configurations
#[derive(Debug)]
pub struct ResourceManagerConfigBuilder {
    config: ResourceManagerConfig,
}

impl ResourceManagerConfigBuilder {
    /// Create a new builder for the specified resource type
    pub fn new(resource_type: ResourceType) -> Self {
        Self {
            config: ResourceManagerConfig {
                resource_type,
                ..Default::default()
            },
        }
    }

    /// Set resource limits
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.config.limits = limits;
        self
    }

    /// Set hard limit
    pub fn with_hard_limit(mut self, limit: u64) -> Self {
        self.config.limits.hard_limit = limit;
        self
    }

    /// Set soft limit
    pub fn with_soft_limit(mut self, limit: u64) -> Self {
        self.config.limits.soft_limit = limit;
        self
    }

    /// Enable overcommit with ratio
    pub fn with_overcommit(mut self, ratio: f64) -> Self {
        self.config.limits.allow_overcommit = true;
        self.config.limits.overcommit_ratio = ratio;
        self
    }

    /// Set system reserve
    pub fn with_system_reserve(mut self, reserve: u64) -> Self {
        self.config.limits.system_reserve = reserve;
        self
    }

    /// Set monitoring interval
    pub fn with_monitoring_interval(mut self, interval: Duration) -> Self {
        self.config.monitoring_interval = interval;
        self
    }

    /// Set health check interval
    pub fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.config.health_check_interval = interval;
        self
    }

    /// Enable telemetry
    pub fn with_telemetry(mut self, enabled: bool) -> Self {
        self.config.enable_telemetry = enabled;
        self
    }

    /// Add custom configuration
    pub fn with_custom_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.custom_config.insert(key.into(), value.into());
        self
    }

    /// Build the configuration
    pub fn build(self) -> ResourceManagerConfig {
        self.config
    }
}

/// Composite resource manager for managing multiple resource types
#[derive(Debug)]
pub struct CompositeResourceManager {
    managers: Arc<RwLock<HashMap<ResourceType, Box<dyn ResourceManager>>>>,
    config: ResourceManagerConfig,
}

impl CompositeResourceManager {
    /// Create a new composite resource manager
    pub fn new() -> Self {
        Self {
            managers: Arc::new(RwLock::new(HashMap::new())),
            config: ResourceManagerConfigBuilder::new(ResourceType::Custom("composite".to_string()))
                .build(),
        }
    }

    /// Add a resource manager for a specific type
    pub async fn add_manager(
        &self,
        resource_type: ResourceType,
        manager: Box<dyn ResourceManager>,
    ) -> BlixardResult<()> {
        let mut managers = self.managers.write().await;
        managers.insert(resource_type, manager);
        Ok(())
    }

    /// Remove a resource manager
    pub async fn remove_manager(&self, resource_type: &ResourceType) -> BlixardResult<()> {
        let mut managers = self.managers.write().await;
        if let Some(manager) = managers.remove(resource_type) {
            manager.stop().await?;
        }
        Ok(())
    }

    /// Get manager for a specific resource type
    pub async fn get_manager(&self, _resource_type: &ResourceType) -> Option<&dyn ResourceManager> {
        // Note: This doesn't work with async RwLock - need different approach
        // Will be implemented in the concrete implementation
        None
    }

    /// Get all managed resource types
    pub async fn get_resource_types(&self) -> Vec<ResourceType> {
        let managers = self.managers.read().await;
        managers.keys().cloned().collect()
    }
}

#[async_trait]
impl ResourceManager for CompositeResourceManager {
    fn resource_type(&self) -> ResourceType {
        ResourceType::Custom("composite".to_string())
    }

    fn config(&self) -> &ResourceManagerConfig {
        &self.config
    }

    async fn allocate(&self, request: ResourceAllocationRequest) -> BlixardResult<ResourceAllocation> {
        let managers = self.managers.read().await;
        if let Some(manager) = managers.get(&request.resource_type) {
            manager.allocate(request).await
        } else {
            Err(BlixardError::ResourceUnavailable {
                resource_type: request.resource_type.to_string(),
                message: "No manager available for resource type".to_string(),
            })
        }
    }

    async fn deallocate(&self, allocation_id: &ResourceId) -> BlixardResult<()> {
        // For composite manager, we need to track which manager owns which allocation
        // This is simplified - real implementation would maintain an allocation registry
        let managers = self.managers.read().await;
        for manager in managers.values() {
            if let Ok(Some(_)) = manager.get_allocation(allocation_id).await {
                return manager.deallocate(allocation_id).await;
            }
        }
        Err(BlixardError::ResourceUnavailable {
            resource_type: "unknown".to_string(),
            message: format!("Allocation {} not found in any manager", allocation_id),
        })
    }

    async fn check_availability(&self, request: &ResourceAllocationRequest) -> BlixardResult<bool> {
        let managers = self.managers.read().await;
        if let Some(manager) = managers.get(&request.resource_type) {
            manager.check_availability(request).await
        } else {
            Ok(false)
        }
    }

    async fn get_usage(&self) -> BlixardResult<ResourceUsage> {
        // Return aggregate usage across all managers
        let managers = self.managers.read().await;
        let mut total_capacity = 0;
        let mut total_allocated = 0;
        let mut total_used = 0;
        let mut total_allocations = 0;

        for manager in managers.values() {
            if let Ok(usage) = manager.get_usage().await {
                total_capacity += usage.total_capacity;
                total_allocated += usage.allocated_amount;
                total_used += usage.used_amount;
                total_allocations += usage.allocation_count;
            }
        }

        Ok(ResourceUsage {
            resource_type: self.resource_type(),
            total_capacity,
            allocated_amount: total_allocated,
            used_amount: total_used,
            allocation_count: total_allocations,
            updated_at: SystemTime::now(),
        })
    }

    async fn get_allocations(&self) -> BlixardResult<Vec<ResourceAllocation>> {
        let managers = self.managers.read().await;
        let mut all_allocations = Vec::new();

        for manager in managers.values() {
            if let Ok(allocations) = manager.get_allocations().await {
                all_allocations.extend(allocations);
            }
        }

        Ok(all_allocations)
    }

    async fn get_allocation(&self, allocation_id: &ResourceId) -> BlixardResult<Option<ResourceAllocation>> {
        let managers = self.managers.read().await;
        for manager in managers.values() {
            if let Ok(Some(allocation)) = manager.get_allocation(allocation_id).await {
                return Ok(Some(allocation));
            }
        }
        Ok(None)
    }

    async fn initialize(&self) -> BlixardResult<()> {
        let managers = self.managers.read().await;
        for (resource_type, manager) in managers.iter() {
            if let Err(e) = manager.initialize().await {
                warn!("Failed to initialize {} manager: {}", resource_type, e);
            }
        }
        Ok(())
    }

    async fn start(&self) -> BlixardResult<()> {
        let managers = self.managers.read().await;
        for (resource_type, manager) in managers.iter() {
            if let Err(e) = manager.start().await {
                warn!("Failed to start {} manager: {}", resource_type, e);
            }
        }
        Ok(())
    }

    async fn stop(&self) -> BlixardResult<()> {
        let managers = self.managers.read().await;
        for (resource_type, manager) in managers.iter() {
            if let Err(e) = manager.stop().await {
                warn!("Failed to stop {} manager: {}", resource_type, e);
            }
        }
        Ok(())
    }

    async fn health_check(&self) -> BlixardResult<ResourceManagerHealth> {
        let managers = self.managers.read().await;
        let mut unhealthy_count = 0;
        let mut degraded_count = 0;
        let total_count = managers.len();

        for manager in managers.values() {
            match manager.health_check().await {
                Ok(ResourceManagerHealth::Healthy) => {}
                Ok(ResourceManagerHealth::Degraded(_)) => degraded_count += 1,
                Ok(ResourceManagerHealth::Unhealthy(_)) | Ok(ResourceManagerHealth::Stopped) => unhealthy_count += 1,
                Err(_) => unhealthy_count += 1,
            }
        }

        if unhealthy_count > 0 {
            Ok(ResourceManagerHealth::Unhealthy(format!(
                "{} of {} managers unhealthy",
                unhealthy_count, total_count
            )))
        } else if degraded_count > 0 {
            Ok(ResourceManagerHealth::Degraded(format!(
                "{} of {} managers degraded",
                degraded_count, total_count
            )))
        } else {
            Ok(ResourceManagerHealth::Healthy)
        }
    }

    async fn is_running(&self) -> bool {
        let managers = self.managers.read().await;
        if managers.is_empty() {
            return false;
        }

        for manager in managers.values() {
            if !manager.is_running().await {
                return false;
            }
        }
        true
    }

    async fn get_metrics(&self) -> BlixardResult<ResourceManagerMetrics> {
        let managers = self.managers.read().await;
        let mut total_allocations = 0;
        let mut total_deallocations = 0;
        let mut total_failed = 0;
        let mut avg_times = Vec::new();

        for manager in managers.values() {
            if let Ok(metrics) = manager.get_metrics().await {
                total_allocations += metrics.total_allocations;
                total_deallocations += metrics.total_deallocations;
                total_failed += metrics.failed_allocations;
                avg_times.push(metrics.avg_allocation_time_ms);
            }
        }

        let avg_allocation_time_ms = if avg_times.is_empty() {
            0.0
        } else {
            avg_times.iter().sum::<f64>() / avg_times.len() as f64
        };

        Ok(ResourceManagerMetrics {
            resource_type: self.resource_type(),
            total_allocations,
            total_deallocations,
            failed_allocations: total_failed,
            avg_allocation_time_ms,
            health: self.health_check().await.unwrap_or(ResourceManagerHealth::Unhealthy("Health check failed".to_string())),
            last_health_check: SystemTime::now(),
            custom_metrics: HashMap::new(),
        })
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
        let managers = self.managers.read().await;
        let mut telemetry = HashMap::new();

        for (resource_type, manager) in managers.iter() {
            if let Ok(manager_telemetry) = manager.export_telemetry().await {
                telemetry.insert(resource_type.to_string(), serde_json::json!(manager_telemetry));
            }
        }

        telemetry.insert("composite_manager".to_string(), serde_json::json!({
            "manager_count": managers.len(),
            "resource_types": managers.keys().collect::<Vec<_>>(),
        }));

        Ok(telemetry)
    }

    async fn update_limits(&self, limits: ResourceLimits) -> BlixardResult<()> {
        let managers = self.managers.read().await;
        if let Some(manager) = managers.get(&limits.resource_type) {
            manager.update_limits(limits).await
        } else {
            Err(BlixardError::ResourceUnavailable {
                resource_type: limits.resource_type.to_string(),
                message: "No manager available for resource type".to_string(),
            })
        }
    }

    async fn get_limits(&self) -> BlixardResult<ResourceLimits> {
        // Return default limits for composite manager
        Ok(self.config.limits.clone())
    }

    async fn validate_config(&self, _config: &ResourceManagerConfig) -> BlixardResult<()> {
        // Basic validation for composite manager
        Ok(())
    }

    async fn update_config(&self, config: ResourceManagerConfig) -> BlixardResult<()> {
        // Update config (simplified - real implementation would handle this properly)
        info!("Updating composite resource manager config: {:?}", config);
        Ok(())
    }
}

impl Default for CompositeResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resource_manager_config_builder() {
        let config = ResourceManagerConfigBuilder::new(ResourceType::Memory)
            .with_hard_limit(8192)
            .with_soft_limit(6144)
            .with_overcommit(1.5)
            .with_system_reserve(512)
            .with_monitoring_interval(Duration::from_secs(30))
            .with_telemetry(true)
            .with_custom_config("test_key".to_string(), "test_value".to_string())
            .build();

        assert_eq!(config.resource_type, ResourceType::Memory);
        assert_eq!(config.limits.hard_limit, 8192);
        assert_eq!(config.limits.soft_limit, 6144);
        assert!(config.limits.allow_overcommit);
        assert_eq!(config.limits.overcommit_ratio, 1.5);
        assert_eq!(config.limits.system_reserve, 512);
        assert_eq!(config.monitoring_interval, Duration::from_secs(30));
        assert!(config.enable_telemetry);
        assert_eq!(config.custom_config.get("test_key"), Some(&"test_value".to_string()));
    }

    #[tokio::test]
    async fn test_resource_usage_calculations() {
        let usage = ResourceUsage {
            resource_type: ResourceType::Memory,
            total_capacity: 1000,
            allocated_amount: 800,
            used_amount: 600,
            allocation_count: 5,
            updated_at: SystemTime::now(),
        };

        assert_eq!(usage.utilization_percent(), 60.0);
        assert_eq!(usage.allocation_percent(), 80.0);
        assert!(!usage.is_overallocated());

        let overallocated = ResourceUsage {
            total_capacity: 1000,
            allocated_amount: 1200,
            ..usage
        };
        assert!(overallocated.is_overallocated());
    }

    #[tokio::test]
    async fn test_composite_resource_manager_creation() {
        let composite = CompositeResourceManager::new();
        assert_eq!(composite.resource_type(), ResourceType::Custom("composite".to_string()));
        assert_eq!(composite.get_resource_types().await.len(), 0);
    }

    #[test]
    fn test_resource_type_display() {
        assert_eq!(ResourceType::Memory.to_string(), "memory");
        assert_eq!(ResourceType::Cpu.to_string(), "cpu");
        assert_eq!(ResourceType::Custom("test".to_string()).to_string(), "test");
    }

    #[test]
    fn test_allocation_status() {
        assert_eq!(AllocationStatus::Active, AllocationStatus::Active);
        assert_ne!(AllocationStatus::Active, AllocationStatus::Reserved);
        
        match AllocationStatus::Failed("test error".to_string()) {
            AllocationStatus::Failed(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Failed status"),
        }
    }
}