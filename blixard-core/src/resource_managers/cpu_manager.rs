//! CPU Resource Manager Implementation
//!
//! This module provides a concrete implementation of the ResourceManager trait
//! specifically for CPU resource management with support for:
//! - vCPU allocation and tracking
//! - CPU affinity and pinning
//! - Overcommit policies with different scheduling strategies
//! - Real-time CPU utilization monitoring

use crate::error::{BlixardError, BlixardResult};
use crate::resource_manager::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, info};

/// CPU scheduling policy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CpuSchedulingPolicy {
    /// First-fit allocation
    FirstFit,
    /// Best-fit allocation (minimize fragmentation)
    BestFit,
    /// Round-robin allocation
    RoundRobin,
    /// CPU affinity-aware allocation
    AffinityAware,
}

/// CPU resource allocation with specific core assignments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuAllocation {
    /// Base allocation information
    pub base: ResourceAllocation,
    /// Specific CPU cores assigned
    pub assigned_cores: Vec<u32>,
    /// CPU affinity mask (if applicable)
    pub affinity_mask: Option<u64>,
    /// Scheduling policy used
    pub scheduling_policy: CpuSchedulingPolicy,
    /// Whether cores are exclusively assigned
    pub exclusive: bool,
}

/// CPU-specific resource manager
#[derive(Debug)]
pub struct CpuResourceManager {
    /// Manager configuration
    config: ResourceManagerConfig,
    /// Current CPU allocations
    allocations: Arc<RwLock<HashMap<ResourceId, CpuAllocation>>>,
    /// CPU core availability tracking
    core_usage: Arc<RwLock<HashMap<u32, CoreUsage>>>,
    /// Current usage tracking
    usage: Arc<RwLock<ResourceUsage>>,
    /// Runtime state
    state: Arc<RwLock<ManagerState>>,
    /// Background monitoring task handle
    monitoring_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Background health check task handle
    health_check_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Manager metrics
    metrics: Arc<RwLock<CpuManagerMetrics>>,
    /// Scheduling policy
    scheduling_policy: CpuSchedulingPolicy,
    /// Total number of physical CPU cores
    total_cores: u32,
}

#[derive(Debug, Clone)]
struct CoreUsage {
    /// Core number
    core_id: u32,
    /// Number of vCPUs allocated to this core
    allocated_vcpus: u32,
    /// Current utilization percentage
    utilization_percent: f64,
    /// Whether core is exclusively assigned
    exclusive: bool,
    /// Tenant that has exclusive access (if any)
    exclusive_tenant: Option<TenantId>,
}

#[derive(Debug)]
struct ManagerState {
    is_running: bool,
    health: ResourceManagerHealth,
    last_health_check: SystemTime,
    total_allocations: u64,
    total_deallocations: u64,
    #[allow(dead_code)] // Tracked for future allocation failure analysis
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CpuManagerMetrics {
    /// Base resource manager metrics
    pub base: ResourceManagerMetrics,
    /// Average CPU utilization across all cores
    pub avg_cpu_utilization: f64,
    /// Number of cores with exclusive assignments
    pub exclusive_cores: u32,
    /// CPU scheduling efficiency
    pub scheduling_efficiency: f64,
    /// Core fragmentation ratio
    pub fragmentation_ratio: f64,
}

impl CpuResourceManager {
    /// Create a new CPU resource manager
    pub fn new(mut config: ResourceManagerConfig, total_cores: u32) -> Self {
        // Ensure we're managing CPU resources
        config.resource_type = ResourceType::Cpu;

        let initial_usage = ResourceUsage {
            resource_type: ResourceType::Cpu,
            total_capacity: total_cores as u64,
            allocated_amount: 0,
            used_amount: 0,
            allocation_count: 0,
            updated_at: SystemTime::now(),
        };

        let base_metrics = ResourceManagerMetrics {
            resource_type: ResourceType::Cpu,
            total_allocations: 0,
            total_deallocations: 0,
            failed_allocations: 0,
            avg_allocation_time_ms: 0.0,
            health: ResourceManagerHealth::Stopped,
            last_health_check: SystemTime::now(),
            custom_metrics: HashMap::new(),
        };

        let initial_metrics = CpuManagerMetrics {
            base: base_metrics,
            avg_cpu_utilization: 0.0,
            exclusive_cores: 0,
            scheduling_efficiency: 1.0,
            fragmentation_ratio: 0.0,
        };

        // Initialize core usage tracking
        let mut core_usage = HashMap::new();
        for core_id in 0..total_cores {
            core_usage.insert(
                core_id,
                CoreUsage {
                    core_id,
                    allocated_vcpus: 0,
                    utilization_percent: 0.0,
                    exclusive: false,
                    exclusive_tenant: None,
                },
            );
        }

        Self {
            config,
            allocations: Arc::new(RwLock::new(HashMap::new())),
            core_usage: Arc::new(RwLock::new(core_usage)),
            usage: Arc::new(RwLock::new(initial_usage)),
            state: Arc::new(RwLock::new(ManagerState::default())),
            monitoring_handle: Arc::new(Mutex::new(None)),
            health_check_handle: Arc::new(Mutex::new(None)),
            metrics: Arc::new(RwLock::new(initial_metrics)),
            scheduling_policy: CpuSchedulingPolicy::FirstFit,
            total_cores,
        }
    }

    /// Create with builder pattern
    pub fn builder(total_cores: u32) -> CpuResourceManagerBuilder {
        CpuResourceManagerBuilder::new(total_cores)
    }

    /// Find available cores for allocation using the configured scheduling policy
    async fn find_available_cores(
        &self,
        vcpus_needed: u32,
        exclusive: bool,
    ) -> BlixardResult<Vec<u32>> {
        let core_usage = self.core_usage.read().await;
        let mut available_cores = Vec::new();

        match self.scheduling_policy {
            CpuSchedulingPolicy::FirstFit => {
                for core_id in 0..self.total_cores {
                    if let Some(core) = core_usage.get(&core_id) {
                        if exclusive && core.allocated_vcpus > 0 {
                            continue; // Core already in use
                        }
                        if !exclusive && !core.exclusive {
                            available_cores.push(core_id);
                            if available_cores.len() >= vcpus_needed as usize {
                                break;
                            }
                        } else if exclusive && core.allocated_vcpus == 0 {
                            available_cores.push(core_id);
                            if available_cores.len() >= vcpus_needed as usize {
                                break;
                            }
                        }
                    }
                }
            }
            CpuSchedulingPolicy::BestFit => {
                // Find cores with least fragmentation
                let mut cores_with_usage: Vec<_> = core_usage
                    .iter()
                    .filter(|(_, core)| {
                        if exclusive {
                            core.allocated_vcpus == 0
                        } else {
                            !core.exclusive
                        }
                    })
                    .map(|(id, core)| (*id, core.allocated_vcpus))
                    .collect();

                cores_with_usage.sort_by_key(|(_, usage)| *usage);

                for (core_id, _) in cores_with_usage.iter().take(vcpus_needed as usize) {
                    available_cores.push(*core_id);
                }
            }
            CpuSchedulingPolicy::RoundRobin => {
                // Distribute across cores evenly
                let mut core_ids: Vec<_> = (0..self.total_cores).collect();
                core_ids
                    .sort_by_key(|id| core_usage.get(id).map(|c| c.allocated_vcpus).unwrap_or(0));

                for core_id in core_ids.iter().take(vcpus_needed as usize) {
                    if let Some(core) = core_usage.get(core_id) {
                        if exclusive && core.allocated_vcpus == 0 {
                            available_cores.push(*core_id);
                        } else if !exclusive && !core.exclusive {
                            available_cores.push(*core_id);
                        }
                    }
                }
            }
            CpuSchedulingPolicy::AffinityAware => {
                // Try to keep allocations together for better cache locality
                let mut consecutive_cores = Vec::new();
                let mut current_run = Vec::new();

                for core_id in 0..self.total_cores {
                    if let Some(core) = core_usage.get(&core_id) {
                        let can_use = if exclusive {
                            core.allocated_vcpus == 0
                        } else {
                            !core.exclusive
                        };

                        if can_use {
                            current_run.push(core_id);
                        } else {
                            if current_run.len() >= vcpus_needed as usize {
                                consecutive_cores = current_run.clone();
                                break;
                            }
                            current_run.clear();
                        }
                    }
                }

                if consecutive_cores.len() >= vcpus_needed as usize {
                    available_cores = consecutive_cores
                        .into_iter()
                        .take(vcpus_needed as usize)
                        .collect();
                } else {
                    // Fallback to first-fit
                    return self
                        .find_available_cores_first_fit(vcpus_needed, exclusive)
                        .await;
                }
            }
        }

        if available_cores.len() < vcpus_needed as usize {
            Err(BlixardError::InsufficientResources {
                requested: format!("{} vCPUs", vcpus_needed),
                available: format!("{} cores available", available_cores.len()),
            })
        } else {
            Ok(available_cores)
        }
    }

    /// First-fit allocation fallback
    async fn find_available_cores_first_fit(
        &self,
        vcpus_needed: u32,
        exclusive: bool,
    ) -> BlixardResult<Vec<u32>> {
        let core_usage = self.core_usage.read().await;
        let mut available_cores = Vec::new();

        for core_id in 0..self.total_cores {
            if let Some(core) = core_usage.get(&core_id) {
                let can_use = if exclusive {
                    core.allocated_vcpus == 0
                } else {
                    !core.exclusive && core.allocated_vcpus < 4 // Allow up to 4 vCPUs per core
                };

                if can_use {
                    available_cores.push(core_id);
                    if available_cores.len() >= vcpus_needed as usize {
                        break;
                    }
                }
            }
        }

        if available_cores.len() < vcpus_needed as usize {
            Err(BlixardError::InsufficientResources {
                requested: format!("{} vCPUs", vcpus_needed),
                available: format!("{} cores available", available_cores.len()),
            })
        } else {
            Ok(available_cores)
        }
    }

    /// Allocate CPU cores to an allocation
    async fn allocate_cores(
        &self,
        allocation_id: &ResourceId,
        cores: Vec<u32>,
        exclusive: bool,
        tenant_id: &TenantId,
    ) -> BlixardResult<()> {
        let mut core_usage = self.core_usage.write().await;

        for &core_id in &cores {
            if let Some(core) = core_usage.get_mut(&core_id) {
                if exclusive {
                    core.exclusive = true;
                    core.exclusive_tenant = Some(tenant_id.clone());
                    core.allocated_vcpus = 1; // One vCPU per exclusive core
                } else {
                    core.allocated_vcpus += 1;
                }
            }
        }

        debug!(
            "Allocated {} cores to {}: {:?}",
            cores.len(),
            allocation_id,
            cores
        );
        Ok(())
    }

    /// Deallocate CPU cores from an allocation
    async fn deallocate_cores(&self, cores: &[u32], exclusive: bool) -> BlixardResult<()> {
        let mut core_usage = self.core_usage.write().await;

        for &core_id in cores {
            if let Some(core) = core_usage.get_mut(&core_id) {
                if exclusive {
                    core.exclusive = false;
                    core.exclusive_tenant = None;
                    core.allocated_vcpus = 0;
                } else {
                    core.allocated_vcpus = core.allocated_vcpus.saturating_sub(1);
                }
            }
        }

        debug!("Deallocated {} cores: {:?}", cores.len(), cores);
        Ok(())
    }

    /// Update CPU usage statistics
    async fn update_usage_stats(&self) {
        let allocations = self.allocations.read().await;
        let core_usage = self.core_usage.read().await;
        let mut usage = self.usage.write().await;

        let mut total_allocated = 0;
        let mut total_used = 0.0;
        let mut active_count = 0;

        for allocation in allocations.values() {
            if allocation.base.status == AllocationStatus::Active
                || allocation.base.status == AllocationStatus::Reserved
            {
                total_allocated += allocation.assigned_cores.len() as u64;
                active_count += 1;
            }
        }

        // Calculate actual usage from core utilization
        for core in core_usage.values() {
            if core.allocated_vcpus > 0 {
                total_used += core.utilization_percent;
            }
        }

        usage.allocated_amount = total_allocated;
        usage.used_amount = (total_used / 100.0) as u64; // Convert percentage to "cores used"
        usage.allocation_count = active_count;
        usage.updated_at = SystemTime::now();
    }

    /// Start background monitoring task
    async fn start_monitoring(&self) -> BlixardResult<()> {
        if !self.config.enable_monitoring {
            return Ok(());
        }

        let core_usage = self.core_usage.clone();
        let metrics = self.metrics.clone();
        let interval_duration = self.config.monitoring_interval;

        let handle = tokio::spawn(async move {
            let mut timer = interval(interval_duration);

            loop {
                timer.tick().await;

                // Simulate CPU utilization monitoring
                let mut total_utilization = 0.0;
                let mut exclusive_cores = 0;
                let mut allocated_cores = 0;

                {
                    let mut core_usage_guard = core_usage.write().await;
                    for core in core_usage_guard.values_mut() {
                        if core.allocated_vcpus > 0 {
                            // Simulate varying CPU utilization
                            core.utilization_percent = 20.0 + (rand::random::<f64>() * 60.0);
                            total_utilization += core.utilization_percent;
                            allocated_cores += 1;
                        }

                        if core.exclusive {
                            exclusive_cores += 1;
                        }
                    }
                }

                let avg_utilization = if allocated_cores > 0 {
                    total_utilization / allocated_cores as f64
                } else {
                    0.0
                };

                // Update metrics
                {
                    let mut metrics_guard = metrics.write().await;
                    metrics_guard.avg_cpu_utilization = avg_utilization;
                    metrics_guard.exclusive_cores = exclusive_cores;

                    // Calculate scheduling efficiency (simplified)
                    metrics_guard.scheduling_efficiency = if allocated_cores > 0 {
                        (avg_utilization / 100.0).min(1.0)
                    } else {
                        1.0
                    };
                }

                debug!(
                    "CPU monitoring: avg utilization {:.1}%, {} cores allocated",
                    avg_utilization, allocated_cores
                );
            }
        });

        *self.monitoring_handle.lock().await = Some(handle);
        info!(
            "CPU monitoring started with interval: {:?}",
            interval_duration
        );
        Ok(())
    }

    /// Start background health check task
    async fn start_health_checks(&self) -> BlixardResult<()> {
        if !self.config.enable_health_checks {
            return Ok(());
        }

        let state = self.state.clone();
        let metrics = self.metrics.clone();
        let interval_duration = self.config.health_check_interval;

        let handle = tokio::spawn(async move {
            let mut timer = interval(interval_duration);

            loop {
                timer.tick().await;

                let health_status = {
                    let metrics_guard = metrics.read().await;
                    let avg_util = metrics_guard.avg_cpu_utilization;

                    if avg_util > 95.0 {
                        ResourceManagerHealth::Unhealthy(
                            "CPU utilization critically high".to_string(),
                        )
                    } else if avg_util > 85.0 {
                        ResourceManagerHealth::Degraded(format!(
                            "CPU utilization at {:.1}%",
                            avg_util
                        ))
                    } else {
                        ResourceManagerHealth::Healthy
                    }
                };

                {
                    let mut state_guard = state.write().await;
                    state_guard.health = health_status;
                    state_guard.last_health_check = SystemTime::now();
                }

                debug!("CPU health check completed");
            }
        });

        *self.health_check_handle.lock().await = Some(handle);
        info!(
            "CPU health checks started with interval: {:?}",
            interval_duration
        );
        Ok(())
    }

    /// Stop background tasks
    async fn stop_background_tasks(&self) {
        if let Some(handle) = self.monitoring_handle.lock().await.take() {
            handle.abort();
            info!("CPU monitoring stopped");
        }

        if let Some(handle) = self.health_check_handle.lock().await.take() {
            handle.abort();
            info!("CPU health checks stopped");
        }
    }
}

#[async_trait]
impl ResourceManager for CpuResourceManager {
    fn resource_type(&self) -> ResourceType {
        ResourceType::Cpu
    }

    fn config(&self) -> &ResourceManagerConfig {
        &self.config
    }

    async fn allocate(
        &self,
        request: ResourceAllocationRequest,
    ) -> BlixardResult<ResourceAllocation> {
        let start_time = SystemTime::now();

        // Validate the request
        if request.resource_type != ResourceType::Cpu {
            return Err(BlixardError::ConfigurationError {
                component: "cpu_manager.allocation.resource_type".to_string(),
                message: "CPU manager can only handle CPU resources".to_string(),
            });
        }

        // Determine if exclusive allocation is requested
        let exclusive = request
            .metadata
            .get("exclusive")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Find available cores
        let cores = self
            .find_available_cores(request.amount as u32, exclusive)
            .await?;

        // Create base allocation
        let base_allocation = ResourceAllocation {
            allocation_id: format!(
                "cpu-{}-{}",
                request.tenant_id,
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            request: request.clone(),
            allocated_amount: cores.len() as u64,
            allocated_on: Some("cpu".to_string()),
            allocated_at: SystemTime::now(),
            status: AllocationStatus::Active,
        };

        // Create CPU-specific allocation
        let cpu_allocation = CpuAllocation {
            base: base_allocation.clone(),
            assigned_cores: cores.clone(),
            affinity_mask: None, // Could be calculated based on cores
            scheduling_policy: self.scheduling_policy.clone(),
            exclusive,
        };

        // Allocate the cores
        self.allocate_cores(
            &base_allocation.allocation_id,
            cores,
            exclusive,
            &request.tenant_id,
        )
        .await?;

        // Store allocation
        {
            let mut allocations = self.allocations.write().await;
            allocations.insert(base_allocation.allocation_id.clone(), cpu_allocation);
        }

        // Update usage and state
        self.update_usage_stats().await;

        {
            let mut state = self.state.write().await;
            state.total_allocations += 1;

            if let Ok(duration) = start_time.elapsed() {
                state.allocation_times.push(duration);
                if state.allocation_times.len() > 100 {
                    state.allocation_times.remove(0);
                }
            }
        }

        info!(
            "CPU allocated: {} vCPUs for tenant {} (allocation_id: {}, exclusive: {})",
            request.amount, request.tenant_id, base_allocation.allocation_id, exclusive
        );

        Ok(base_allocation)
    }

    async fn deallocate(&self, allocation_id: &ResourceId) -> BlixardResult<()> {
        let allocation = {
            let mut allocations = self.allocations.write().await;
            allocations.remove(allocation_id)
        };

        if let Some(cpu_allocation) = allocation {
            // Deallocate the cores
            self.deallocate_cores(&cpu_allocation.assigned_cores, cpu_allocation.exclusive)
                .await?;

            // Update usage statistics
            self.update_usage_stats().await;

            // Update state
            {
                let mut state = self.state.write().await;
                state.total_deallocations += 1;
            }

            info!(
                "CPU deallocated: {} vCPUs for tenant {} (allocation_id: {})",
                cpu_allocation.assigned_cores.len(),
                cpu_allocation.base.request.tenant_id,
                allocation_id
            );

            Ok(())
        } else {
            Err(BlixardError::ResourceUnavailable {
                resource_type: "cpu".to_string(),
                message: format!("Allocation {} not found", allocation_id),
            })
        }
    }

    async fn check_availability(&self, request: &ResourceAllocationRequest) -> BlixardResult<bool> {
        if request.resource_type != ResourceType::Cpu {
            return Ok(false);
        }

        let exclusive = request
            .metadata
            .get("exclusive")
            .map(|v| v == "true")
            .unwrap_or(false);

        match self
            .find_available_cores(request.amount as u32, exclusive)
            .await
        {
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
        Ok(allocations
            .values()
            .map(|cpu_alloc| cpu_alloc.base.clone())
            .collect())
    }

    async fn get_allocation(
        &self,
        allocation_id: &ResourceId,
    ) -> BlixardResult<Option<ResourceAllocation>> {
        let allocations = self.allocations.read().await;
        Ok(allocations
            .get(allocation_id)
            .map(|cpu_alloc| cpu_alloc.base.clone()))
    }

    async fn initialize(&self) -> BlixardResult<()> {
        info!(
            "Initializing CPU resource manager with {} cores",
            self.total_cores
        );

        // Initialize usage tracking
        let mut usage = self.usage.write().await;
        usage.total_capacity = self.total_cores as u64;
        usage.updated_at = SystemTime::now();
        drop(usage);

        // Initialize state
        let mut state = self.state.write().await;
        state.health = ResourceManagerHealth::Healthy;
        state.last_health_check = SystemTime::now();
        drop(state);

        info!(
            "CPU resource manager initialized with {} cores",
            self.total_cores
        );
        Ok(())
    }

    async fn start(&self) -> BlixardResult<()> {
        info!("Starting CPU resource manager");

        // Start background tasks
        self.start_monitoring().await?;
        self.start_health_checks().await?;

        // Update state
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.health = ResourceManagerHealth::Healthy;
        }

        info!("CPU resource manager started successfully");
        Ok(())
    }

    async fn stop(&self) -> BlixardResult<()> {
        info!("Stopping CPU resource manager");

        // Stop background tasks
        self.stop_background_tasks().await;

        // Update state
        {
            let mut state = self.state.write().await;
            state.is_running = false;
            state.health = ResourceManagerHealth::Stopped;
        }

        info!("CPU resource manager stopped");
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
        Ok(metrics.base.clone())
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
        let metrics = self.metrics.read().await;
        let core_usage = self.core_usage.read().await;

        telemetry.insert("resource_type".to_string(), serde_json::json!("cpu"));
        telemetry.insert(
            "total_cores".to_string(),
            serde_json::json!(self.total_cores),
        );
        telemetry.insert(
            "allocated_vcpus".to_string(),
            serde_json::json!(usage.allocated_amount),
        );
        telemetry.insert(
            "utilization_percent".to_string(),
            serde_json::json!(usage.utilization_percent()),
        );
        telemetry.insert(
            "allocation_count".to_string(),
            serde_json::json!(usage.allocation_count),
        );
        telemetry.insert(
            "avg_cpu_utilization".to_string(),
            serde_json::json!(metrics.avg_cpu_utilization),
        );
        telemetry.insert(
            "exclusive_cores".to_string(),
            serde_json::json!(metrics.exclusive_cores),
        );
        telemetry.insert(
            "scheduling_policy".to_string(),
            serde_json::json!(format!("{:?}", self.scheduling_policy)),
        );

        // Core-specific metrics
        let core_metrics: Vec<serde_json::Value> = core_usage
            .values()
            .map(|core| {
                serde_json::json!({
                    "core_id": core.core_id,
                    "allocated_vcpus": core.allocated_vcpus,
                    "utilization_percent": core.utilization_percent,
                    "exclusive": core.exclusive,
                    "exclusive_tenant": core.exclusive_tenant
                })
            })
            .collect();
        telemetry.insert("core_usage".to_string(), serde_json::json!(core_metrics));

        Ok(telemetry)
    }

    async fn update_limits(&self, limits: ResourceLimits) -> BlixardResult<()> {
        if limits.resource_type != ResourceType::Cpu {
            return Err(BlixardError::ConfigurationError {
                component: "cpu_manager.limits.resource_type".to_string(),
                message: "Limits must be for CPU resource type".to_string(),
            });
        }

        info!(
            "CPU limits update requested: hard={}, soft={}, overcommit={}",
            limits.hard_limit, limits.soft_limit, limits.overcommit_ratio
        );

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
        if config.resource_type != ResourceType::Cpu {
            return Err(BlixardError::ConfigurationError {
                component: "cpu_manager.config.resource_type".to_string(),
                message: "Configuration must be for CPU resource type".to_string(),
            });
        }

        if config.limits.hard_limit == 0 {
            return Err(BlixardError::ConfigurationError {
                component: "cpu_manager.config.hard_limit".to_string(),
                message: "Hard limit must be greater than 0".to_string(),
            });
        }

        Ok(())
    }

    async fn update_config(&self, config: ResourceManagerConfig) -> BlixardResult<()> {
        self.validate_config(&config).await?;

        info!("CPU manager configuration update validated");
        Ok(())
    }
}

/// Builder for CpuResourceManager
pub struct CpuResourceManagerBuilder {
    config: ResourceManagerConfig,
    total_cores: u32,
    scheduling_policy: CpuSchedulingPolicy,
}

impl CpuResourceManagerBuilder {
    pub fn new(total_cores: u32) -> Self {
        Self {
            config: ResourceManagerConfigBuilder::new(ResourceType::Cpu).build(),
            total_cores,
            scheduling_policy: CpuSchedulingPolicy::FirstFit,
        }
    }

    pub fn with_overcommit(mut self, ratio: f64) -> Self {
        self.config.limits.allow_overcommit = true;
        self.config.limits.overcommit_ratio = ratio;
        self
    }

    pub fn with_scheduling_policy(mut self, policy: CpuSchedulingPolicy) -> Self {
        self.scheduling_policy = policy;
        self
    }

    pub fn with_monitoring(mut self, enabled: bool, interval: Duration) -> Self {
        self.config.enable_monitoring = enabled;
        self.config.monitoring_interval = interval;
        self
    }

    pub fn build(self) -> CpuResourceManager {
        let mut manager = CpuResourceManager::new(self.config, self.total_cores);
        manager.scheduling_policy = self.scheduling_policy;
        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cpu_manager_creation() {
        let manager = CpuResourceManager::builder(8)
            .with_overcommit(2.0)
            .with_scheduling_policy(CpuSchedulingPolicy::BestFit)
            .build();

        assert_eq!(manager.resource_type(), ResourceType::Cpu);
        assert_eq!(manager.total_cores, 8);
        assert_eq!(manager.scheduling_policy, CpuSchedulingPolicy::BestFit);
    }

    #[tokio::test]
    async fn test_cpu_allocation_and_deallocation() {
        let manager = CpuResourceManager::builder(4).build();
        manager.initialize().await.unwrap();

        let request = ResourceAllocationRequest {
            request_id: "test-req-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            resource_type: ResourceType::Cpu,
            amount: 2,
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: HashMap::new(),
        };

        // Test allocation
        let allocation = manager.allocate(request.clone()).await.unwrap();
        assert_eq!(allocation.allocated_amount, 2);
        assert_eq!(allocation.status, AllocationStatus::Active);

        // Check usage
        let usage = manager.get_usage().await.unwrap();
        assert_eq!(usage.allocated_amount, 2);
        assert_eq!(usage.allocation_count, 1);

        // Test deallocation
        manager.deallocate(&allocation.allocation_id).await.unwrap();

        let usage_after = manager.get_usage().await.unwrap();
        assert_eq!(usage_after.allocated_amount, 0);
        assert_eq!(usage_after.allocation_count, 0);
    }

    #[tokio::test]
    async fn test_exclusive_cpu_allocation() {
        let manager = CpuResourceManager::builder(4).build();
        manager.initialize().await.unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("exclusive".to_string(), "true".to_string());

        let request = ResourceAllocationRequest {
            request_id: "test-req-exclusive".to_string(),
            tenant_id: "tenant-1".to_string(),
            resource_type: ResourceType::Cpu,
            amount: 2,
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata,
        };

        let allocation = manager.allocate(request).await.unwrap();
        assert_eq!(allocation.allocated_amount, 2);

        // Verify cores are marked as exclusive
        let core_usage = manager.core_usage.read().await;
        let exclusive_count = core_usage.values().filter(|c| c.exclusive).count();
        assert_eq!(exclusive_count, 2);
    }

    #[tokio::test]
    async fn test_cpu_overallocation_rejection() {
        let manager = CpuResourceManager::builder(2).build();
        manager.initialize().await.unwrap();

        let large_request = ResourceAllocationRequest {
            request_id: "test-req-large".to_string(),
            tenant_id: "tenant-1".to_string(),
            resource_type: ResourceType::Cpu,
            amount: 5, // More than available cores
            priority: 100,
            is_temporary: false,
            expires_at: None,
            metadata: HashMap::new(),
        };

        let result = manager.allocate(large_request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlixardError::InsufficientResources { .. }
        ));
    }
}
