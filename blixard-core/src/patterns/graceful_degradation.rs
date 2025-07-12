//! Graceful Degradation Pattern for Service Resilience
//!
//! This module provides patterns for gracefully degrading service functionality
//! when subsystems fail, ensuring core operations continue while non-essential
//! features are temporarily disabled.
//!
//! ## Pattern Implementation
//!
//! The graceful degradation pattern allows services to:
//! - Continue operating with reduced functionality during failures
//! - Prioritize critical operations over non-essential ones
//! - Provide fallback mechanisms for failed subsystems
//! - Automatically recover when subsystems come back online
//!
//! ## Service Levels
//!
//! Services are organized into different levels based on criticality:
//! - **Critical**: Core functionality that must always work
//! - **Important**: Important features that enhance user experience
//! - **Optional**: Nice-to-have features that can be disabled
//! - **Experimental**: Beta features that fail gracefully
//!
//! ## Usage Example
//!
//! ```rust
//! use blixard_core::patterns::{GracefulDegradation, ServiceLevel, DegradationLevel};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let degradation = Arc::new(GracefulDegradation::new());
//!
//! // Register a critical service that should always work
//! degradation.register_service("vm_operations", ServiceLevel::Critical).await;
//! 
//! // Register optional services that can be degraded
//! degradation.register_service("metrics_export", ServiceLevel::Optional).await;
//! degradation.register_service("ui_dashboard", ServiceLevel::Optional).await;
//!
//! // Check if a service should be available
//! if degradation.is_service_available("metrics_export").await {
//!     // Export metrics normally
//!     export_detailed_metrics().await;
//! } else {
//!     // Use fallback or skip metrics
//!     export_basic_metrics().await;
//! }
//! # Ok(())
//! # }
//! # async fn export_detailed_metrics() {}
//! # async fn export_basic_metrics() {}
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

use crate::error::{BlixardError, BlixardResult};

/// Service level indicating criticality and degradation behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceLevel {
    /// Core functionality that must always work
    Critical = 0,
    /// Important features that enhance user experience
    Important = 1,
    /// Nice-to-have features that can be disabled
    Optional = 2,
    /// Beta features that fail gracefully
    Experimental = 3,
}

impl ServiceLevel {
    /// Get the display name for the service level
    pub fn name(&self) -> &'static str {
        match self {
            ServiceLevel::Critical => "critical",
            ServiceLevel::Important => "important",
            ServiceLevel::Optional => "optional",
            ServiceLevel::Experimental => "experimental",
        }
    }
}

/// Current degradation level of the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationLevel {
    /// All services operating normally
    Normal,
    /// Optional and experimental services degraded
    Minimal,
    /// Only critical services operating
    Emergency,
    /// System is failing and should be taken offline
    Critical,
}

impl DegradationLevel {
    /// Get the display name for the degradation level
    pub fn name(&self) -> &'static str {
        match self {
            DegradationLevel::Normal => "normal",
            DegradationLevel::Minimal => "minimal",
            DegradationLevel::Emergency => "emergency",
            DegradationLevel::Critical => "critical",
        }
    }

    /// Check if a service level should be available at this degradation level
    pub fn allows_service_level(&self, service_level: ServiceLevel) -> bool {
        match self {
            DegradationLevel::Normal => true,
            DegradationLevel::Minimal => {
                matches!(service_level, ServiceLevel::Critical | ServiceLevel::Important)
            }
            DegradationLevel::Emergency => matches!(service_level, ServiceLevel::Critical),
            DegradationLevel::Critical => false,
        }
    }
}

/// Configuration for a registered service
#[derive(Clone)]
pub struct ServiceConfig {
    /// Name of the service
    pub name: String,
    /// Service level indicating criticality
    pub level: ServiceLevel,
    /// Whether the service is currently healthy
    pub is_healthy: bool,
    /// Timestamp of last health check
    pub last_health_check: Option<Instant>,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Fallback function to call when service is degraded
    pub fallback: Option<Arc<dyn Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync>>,
    /// Recovery function to call when service comes back online
    pub recovery: Option<Arc<dyn Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync>>,
}

impl std::fmt::Debug for ServiceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceConfig")
            .field("name", &self.name)
            .field("level", &self.level)
            .field("is_healthy", &self.is_healthy)
            .field("last_health_check", &self.last_health_check)
            .field("consecutive_failures", &self.consecutive_failures)
            .field("fallback", &self.fallback.as_ref().map(|_| "<closure>"))
            .field("recovery", &self.recovery.as_ref().map(|_| "<closure>"))
            .finish()
    }
}

impl ServiceConfig {
    /// Create a new service configuration
    pub fn new(name: String, level: ServiceLevel) -> Self {
        Self {
            name,
            level,
            is_healthy: true,
            last_health_check: None,
            consecutive_failures: 0,
            fallback: None,
            recovery: None,
        }
    }

    /// Set fallback function for when service is degraded
    pub fn with_fallback<F>(mut self, fallback: F) -> Self
    where
        F: Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync + 'static,
    {
        self.fallback = Some(Arc::new(fallback));
        self
    }

    /// Set recovery function for when service comes back online
    pub fn with_recovery<F>(mut self, recovery: F) -> Self
    where
        F: Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync + 'static,
    {
        self.recovery = Some(Arc::new(recovery));
        self
    }
}

/// Statistics for graceful degradation system
#[derive(Debug, Clone)]
pub struct DegradationStats {
    /// Current system degradation level
    pub current_level: DegradationLevel,
    /// Total number of registered services
    pub total_services: usize,
    /// Number of healthy services
    pub healthy_services: usize,
    /// Number of degraded services
    pub degraded_services: usize,
    /// Services by level
    pub services_by_level: HashMap<ServiceLevel, usize>,
    /// Last degradation change timestamp
    pub last_change: Option<Instant>,
    /// Number of degradation events
    pub degradation_events: u64,
    /// Number of recovery events
    pub recovery_events: u64,
}

/// Trait for health checking services
#[async_trait]
pub trait HealthChecker: Send + Sync {
    /// Check if a service is healthy
    async fn check_health(&self, service_name: &str) -> BlixardResult<bool>;
    
    /// Get service-specific failure threshold
    fn failure_threshold(&self, _service_name: &str) -> u32 {
        3 // Default threshold
    }
}

/// Default health checker that always returns healthy
pub struct DefaultHealthChecker;

#[async_trait]
impl HealthChecker for DefaultHealthChecker {
    async fn check_health(&self, _service_name: &str) -> BlixardResult<bool> {
        Ok(true)
    }
}

/// Graceful degradation manager
pub struct GracefulDegradation {
    services: Arc<RwLock<HashMap<String, ServiceConfig>>>,
    current_level: Arc<RwLock<DegradationLevel>>,
    health_checker: Arc<dyn HealthChecker>,
    stats: Arc<RwLock<DegradationStats>>,
    config: DegradationConfig,
}

/// Configuration for graceful degradation behavior
#[derive(Debug, Clone)]
pub struct DegradationConfig {
    /// How often to check service health
    pub health_check_interval: Duration,
    /// Threshold for triggering degradation
    pub degradation_threshold: f64, // Percentage of failed services
    /// Time to wait before attempting recovery
    pub recovery_delay: Duration,
    /// Enable automatic degradation based on health checks
    pub enable_auto_degradation: bool,
    /// Enable detailed logging of degradation events
    pub enable_logging: bool,
}

impl Default for DegradationConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            degradation_threshold: 0.3, // 30% failure rate triggers degradation
            recovery_delay: Duration::from_secs(60),
            enable_auto_degradation: true,
            enable_logging: true,
        }
    }
}

impl GracefulDegradation {
    /// Create a new graceful degradation manager
    pub fn new() -> Self {
        Self::with_config(DegradationConfig::default())
    }

    /// Create a new graceful degradation manager with custom config
    pub fn with_config(config: DegradationConfig) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            current_level: Arc::new(RwLock::new(DegradationLevel::Normal)),
            health_checker: Arc::new(DefaultHealthChecker),
            stats: Arc::new(RwLock::new(DegradationStats {
                current_level: DegradationLevel::Normal,
                total_services: 0,
                healthy_services: 0,
                degraded_services: 0,
                services_by_level: HashMap::new(),
                last_change: None,
                degradation_events: 0,
                recovery_events: 0,
            })),
            config,
        }
    }

    /// Set a custom health checker
    pub fn with_health_checker(mut self, health_checker: Arc<dyn HealthChecker>) -> Self {
        self.health_checker = health_checker;
        self
    }

    /// Register a service for degradation management
    pub async fn register_service(&self, name: &str, level: ServiceLevel) -> BlixardResult<()> {
        let config = ServiceConfig::new(name.to_string(), level);
        
        {
            let mut services = self.services.write().await;
            services.insert(name.to_string(), config);
        }

        self.update_stats().await;
        
        if self.config.enable_logging {
            info!("Registered {} service: {}", level.name(), name);
        }
        
        Ok(())
    }

    /// Register a service with fallback and recovery functions
    pub async fn register_service_with_handlers<F, R>(
        &self,
        name: &str,
        level: ServiceLevel,
        fallback: Option<F>,
        recovery: Option<R>,
    ) -> BlixardResult<()>
    where
        F: Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync + 'static,
        R: Fn() -> Box<dyn std::future::Future<Output = BlixardResult<()>> + Send + Unpin> + Send + Sync + 'static,
    {
        let mut config = ServiceConfig::new(name.to_string(), level);
        
        if let Some(fallback_fn) = fallback {
            config.fallback = Some(Arc::new(fallback_fn));
        }
        
        if let Some(recovery_fn) = recovery {
            config.recovery = Some(Arc::new(recovery_fn));
        }
        
        {
            let mut services = self.services.write().await;
            services.insert(name.to_string(), config);
        }

        self.update_stats().await;
        
        if self.config.enable_logging {
            info!("Registered {} service with handlers: {}", level.name(), name);
        }
        
        Ok(())
    }

    /// Check if a service should be available based on current degradation level
    pub async fn is_service_available(&self, service_name: &str) -> bool {
        let services = self.services.read().await;
        let current_level = *self.current_level.read().await;
        
        if let Some(service) = services.get(service_name) {
            service.is_healthy && current_level.allows_service_level(service.level)
        } else {
            false
        }
    }

    /// Check if a service is healthy (regardless of degradation level)
    pub async fn is_service_healthy(&self, service_name: &str) -> bool {
        let services = self.services.read().await;
        services.get(service_name).map(|s| s.is_healthy).unwrap_or(false)
    }

    /// Manually mark a service as unhealthy
    pub async fn mark_service_unhealthy(&self, service_name: &str, reason: &str) -> BlixardResult<()> {
        let mut services = self.services.write().await;
        
        if let Some(service) = services.get_mut(service_name) {
            if service.is_healthy {
                service.is_healthy = false;
                service.consecutive_failures += 1;
                
                if self.config.enable_logging {
                    warn!("Marking service {} as unhealthy: {}", service_name, reason);
                }

                // Execute fallback if available
                if let Some(ref fallback) = service.fallback {
                    let fallback_fn = Arc::clone(fallback);
                    drop(services); // Release lock before async call
                    
                    if let Err(e) = fallback_fn().await {
                        error!("Fallback for service {} failed: {}", service_name, e);
                    }
                } else {
                    drop(services);
                }
            } else {
                drop(services);
            }
        } else {
            return Err(BlixardError::NotFound {
                resource: format!("Service {}", service_name),
            });
        }

        self.update_stats().await;
        self.evaluate_degradation_level().await?;
        
        Ok(())
    }

    /// Manually mark a service as healthy
    pub async fn mark_service_healthy(&self, service_name: &str) -> BlixardResult<()> {
        let mut services = self.services.write().await;
        
        if let Some(service) = services.get_mut(service_name) {
            if !service.is_healthy {
                service.is_healthy = true;
                service.consecutive_failures = 0;
                
                if self.config.enable_logging {
                    info!("Marking service {} as healthy", service_name);
                }

                // Execute recovery if available
                if let Some(ref recovery) = service.recovery {
                    let recovery_fn = Arc::clone(recovery);
                    drop(services); // Release lock before async call
                    
                    if let Err(e) = recovery_fn().await {
                        error!("Recovery for service {} failed: {}", service_name, e);
                    }
                } else {
                    drop(services);
                }
            } else {
                drop(services);
            }
        } else {
            return Err(BlixardError::NotFound {
                resource: format!("Service {}", service_name),
            });
        }

        self.update_stats().await;
        self.evaluate_degradation_level().await?;
        
        Ok(())
    }

    /// Get current degradation level
    pub async fn current_level(&self) -> DegradationLevel {
        *self.current_level.read().await
    }

    /// Force set degradation level (for emergencies)
    pub async fn force_degradation_level(&self, level: DegradationLevel) -> BlixardResult<()> {
        let mut current = self.current_level.write().await;
        let old_level = *current;
        *current = level;
        
        if old_level != level {
            if self.config.enable_logging {
                warn!("Degradation level manually changed from {} to {}", old_level.name(), level.name());
            }
            
            let mut stats = self.stats.write().await;
            stats.current_level = level;
            stats.last_change = Some(Instant::now());
            
            if level > old_level {
                stats.degradation_events += 1;
            } else {
                stats.recovery_events += 1;
            }
        }
        
        Ok(())
    }

    /// Run health checks for all services
    pub async fn run_health_checks(&self) -> BlixardResult<()> {
        let service_names: Vec<String> = {
            let services = self.services.read().await;
            services.keys().cloned().collect()
        };

        for service_name in service_names {
            match self.health_checker.check_health(&service_name).await {
                Ok(is_healthy) => {
                    if is_healthy {
                        // Only mark as healthy if it was previously unhealthy
                        let was_unhealthy = {
                            let services = self.services.read().await;
                            services.get(&service_name).map(|s| !s.is_healthy).unwrap_or(false)
                        };
                        
                        if was_unhealthy {
                            self.mark_service_healthy(&service_name).await?;
                        }
                    } else {
                        self.mark_service_unhealthy(&service_name, "health check failed").await?;
                    }
                }
                Err(e) => {
                    debug!("Health check error for {}: {}", service_name, e);
                    self.mark_service_unhealthy(&service_name, &format!("health check error: {}", e)).await?;
                }
            }
        }

        Ok(())
    }

    /// Get current statistics
    pub async fn stats(&self) -> DegradationStats {
        self.stats.read().await.clone()
    }

    /// Update internal statistics
    async fn update_stats(&self) {
        let services = self.services.read().await;
        let mut stats = self.stats.write().await;
        
        stats.total_services = services.len();
        stats.healthy_services = services.values().filter(|s| s.is_healthy).count();
        stats.degraded_services = stats.total_services - stats.healthy_services;
        stats.current_level = *self.current_level.read().await;
        
        stats.services_by_level.clear();
        for service in services.values() {
            *stats.services_by_level.entry(service.level).or_insert(0) += 1;
        }
    }

    /// Evaluate and potentially change degradation level based on service health
    async fn evaluate_degradation_level(&self) -> BlixardResult<()> {
        if !self.config.enable_auto_degradation {
            return Ok(());
        }

        let stats = self.stats.read().await;
        let current_level = *self.current_level.read().await;
        
        // Calculate failure rates by service level
        let critical_failures = stats.services_by_level.get(&ServiceLevel::Critical).copied().unwrap_or(0) as f64;
        let important_failures = stats.services_by_level.get(&ServiceLevel::Important).copied().unwrap_or(0) as f64;
        let total_services = stats.total_services as f64;
        
        let new_level = if total_services == 0.0 {
            DegradationLevel::Normal
        } else {
            let failure_rate = stats.degraded_services as f64 / total_services;
            
            // If any critical services are failing, go to emergency or critical mode
            if critical_failures > 0.0 {
                if critical_failures / total_services > 0.5 {
                    DegradationLevel::Critical
                } else {
                    DegradationLevel::Emergency
                }
            } else if failure_rate > self.config.degradation_threshold {
                DegradationLevel::Minimal
            } else {
                DegradationLevel::Normal
            }
        };
        
        drop(stats);

        if new_level != current_level {
            self.change_degradation_level(current_level, new_level).await?;
        }

        Ok(())
    }

    /// Change degradation level and update statistics
    async fn change_degradation_level(&self, old_level: DegradationLevel, new_level: DegradationLevel) -> BlixardResult<()> {
        {
            let mut current = self.current_level.write().await;
            *current = new_level;
        }

        {
            let mut stats = self.stats.write().await;
            stats.current_level = new_level;
            stats.last_change = Some(Instant::now());
            
            if new_level > old_level {
                stats.degradation_events += 1;
                if self.config.enable_logging {
                    warn!("System degraded from {} to {} mode", old_level.name(), new_level.name());
                }
            } else {
                stats.recovery_events += 1;
                if self.config.enable_logging {
                    info!("System recovered from {} to {} mode", old_level.name(), new_level.name());
                }
            }
        }

        Ok(())
    }

    /// Start background health checking task
    pub async fn start_health_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let degradation = Arc::new(self.clone());
        let interval = self.config.health_check_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                if let Err(e) = degradation.run_health_checks().await {
                    error!("Health check run failed: {}", e);
                }
            }
        })
    }
}

impl Clone for GracefulDegradation {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            current_level: Arc::clone(&self.current_level),
            health_checker: Arc::clone(&self.health_checker),
            stats: Arc::clone(&self.stats),
            config: self.config.clone(),
        }
    }
}

impl Default for GracefulDegradation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_service_registration() {
        let degradation = GracefulDegradation::new();
        
        degradation.register_service("critical_service", ServiceLevel::Critical).await.unwrap();
        degradation.register_service("optional_service", ServiceLevel::Optional).await.unwrap();
        
        assert!(degradation.is_service_available("critical_service").await);
        assert!(degradation.is_service_available("optional_service").await);
        
        let stats = degradation.stats().await;
        assert_eq!(stats.total_services, 2);
        assert_eq!(stats.healthy_services, 2);
    }

    #[tokio::test]
    async fn test_service_degradation() {
        let degradation = GracefulDegradation::new();
        
        degradation.register_service("critical_service", ServiceLevel::Critical).await.unwrap();
        degradation.register_service("optional_service", ServiceLevel::Optional).await.unwrap();
        
        // Mark optional service as unhealthy
        degradation.mark_service_unhealthy("optional_service", "test failure").await.unwrap();
        
        // Critical should still be available
        assert!(degradation.is_service_available("critical_service").await);
        assert!(!degradation.is_service_available("optional_service").await);
    }

    #[tokio::test]
    async fn test_degradation_levels() {
        let degradation = GracefulDegradation::new();
        
        // Normal level should allow all services
        assert!(DegradationLevel::Normal.allows_service_level(ServiceLevel::Critical));
        assert!(DegradationLevel::Normal.allows_service_level(ServiceLevel::Optional));
        
        // Emergency level should only allow critical services
        assert!(DegradationLevel::Emergency.allows_service_level(ServiceLevel::Critical));
        assert!(!DegradationLevel::Emergency.allows_service_level(ServiceLevel::Optional));
        
        // Critical level should allow no services
        assert!(!DegradationLevel::Critical.allows_service_level(ServiceLevel::Critical));
        assert!(!DegradationLevel::Critical.allows_service_level(ServiceLevel::Optional));
    }

    #[tokio::test]
    async fn test_manual_degradation() {
        let degradation = GracefulDegradation::new();
        
        degradation.register_service("test_service", ServiceLevel::Optional).await.unwrap();
        
        // Force emergency mode
        degradation.force_degradation_level(DegradationLevel::Emergency).await.unwrap();
        assert_eq!(degradation.current_level().await, DegradationLevel::Emergency);
        
        // Optional service should not be available
        assert!(!degradation.is_service_available("test_service").await);
        
        // Recover to normal
        degradation.force_degradation_level(DegradationLevel::Normal).await.unwrap();
        assert!(degradation.is_service_available("test_service").await);
    }

    #[tokio::test]
    async fn test_service_recovery() {
        let degradation = GracefulDegradation::new();
        
        degradation.register_service("test_service", ServiceLevel::Important).await.unwrap();
        
        // Mark as unhealthy
        degradation.mark_service_unhealthy("test_service", "test").await.unwrap();
        assert!(!degradation.is_service_healthy("test_service").await);
        
        // Mark as healthy again
        degradation.mark_service_healthy("test_service").await.unwrap();
        assert!(degradation.is_service_healthy("test_service").await);
        assert!(degradation.is_service_available("test_service").await);
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let degradation = GracefulDegradation::new();
        
        degradation.register_service("critical", ServiceLevel::Critical).await.unwrap();
        degradation.register_service("optional", ServiceLevel::Optional).await.unwrap();
        
        let stats = degradation.stats().await;
        assert_eq!(stats.total_services, 2);
        assert_eq!(stats.healthy_services, 2);
        assert_eq!(stats.degraded_services, 0);
        
        degradation.mark_service_unhealthy("optional", "test").await.unwrap();
        
        let stats = degradation.stats().await;
        assert_eq!(stats.healthy_services, 1);
        assert_eq!(stats.degraded_services, 1);
    }
}