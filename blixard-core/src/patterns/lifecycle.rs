//! Unified lifecycle management trait for all blixard managers
//!
//! This module provides a consistent interface for manager lifecycle operations
//! while allowing for flexibility in implementation details.

use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use std::fmt::Debug;
use tokio::sync::oneshot;

/// Lifecycle states for managers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    /// Manager is newly created but not initialized
    Created,
    /// Manager is initializing
    Initializing,
    /// Manager is fully initialized and ready
    Ready,
    /// Manager is starting background operations
    Starting,
    /// Manager is running normally
    Running,
    /// Manager is stopping gracefully
    Stopping,
    /// Manager has stopped
    Stopped,
    /// Manager encountered an error and failed
    Failed(String),
}

/// Health status for managers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

/// Lifecycle statistics
#[derive(Debug, Clone)]
pub struct LifecycleStats {
    pub start_time: Option<std::time::SystemTime>,
    pub uptime: Option<std::time::Duration>,
    pub restart_count: u32,
    pub last_error: Option<String>,
}

impl LifecycleStats {
    pub fn new() -> Self {
        Self {
            start_time: None,
            uptime: None,
            restart_count: 0,
            last_error: None,
        }
    }

    pub fn record_start(&mut self) {
        self.start_time = Some(std::time::SystemTime::now());
        self.uptime = None;
    }

    pub fn record_stop(&mut self) {
        if let Some(start_time) = self.start_time {
            self.uptime = start_time.elapsed().ok();
        }
        self.start_time = None;
    }

    pub fn record_restart(&mut self) {
        self.restart_count += 1;
        self.record_start();
    }

    pub fn record_error(&mut self, error: &str) {
        self.last_error = Some(error.to_string());
    }

    pub fn current_uptime(&self) -> Option<std::time::Duration> {
        self.start_time
            .and_then(|start| start.elapsed().ok())
    }
}

impl Default for LifecycleStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Unified lifecycle management trait for all blixard managers
#[async_trait]
pub trait LifecycleManager: Send + Sync + Debug {
    /// Associated configuration type
    type Config: Clone + Debug + Send + Sync;
    
    /// Associated state type for manager-specific state
    type State: Send + Sync;
    
    /// Associated error type (typically BlixardError)
    type Error: std::error::Error + Send + Sync + 'static;

    /// Create a new manager instance with the given configuration
    /// 
    /// This is a factory method that creates but does not start the manager.
    /// The manager will be in Created state after this call.
    async fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Initialize the manager with any required resources
    /// 
    /// This method should:
    /// - Validate configuration
    /// - Initialize internal state
    /// - Prepare resources but not start background tasks
    /// - Transition from Created → Initializing → Ready
    async fn initialize(&mut self) -> Result<(), Self::Error> {
        // Default implementation - managers can override if needed
        Ok(())
    }

    /// Start the manager and begin normal operations
    /// 
    /// This method should:
    /// - Start background tasks
    /// - Establish connections
    /// - Begin processing
    /// - Transition from Ready → Starting → Running
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Stop the manager gracefully
    /// 
    /// This method should:
    /// - Signal background tasks to stop
    /// - Close connections gracefully
    /// - Clean up resources
    /// - Transition from Running → Stopping → Stopped
    async fn stop(&mut self) -> Result<(), Self::Error>;

    /// Restart the manager (stop then start)
    async fn restart(&mut self) -> Result<(), Self::Error> {
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    /// Get current lifecycle state
    fn state(&self) -> LifecycleState;

    /// Get health status
    async fn health(&self) -> HealthStatus {
        match self.state() {
            LifecycleState::Running => HealthStatus::Healthy,
            LifecycleState::Failed(err) => HealthStatus::Unhealthy(err),
            LifecycleState::Starting | LifecycleState::Stopping => {
                HealthStatus::Degraded("State transition in progress".to_string())
            }
            _ => HealthStatus::Degraded("Not running".to_string()),
        }
    }

    /// Get lifecycle statistics
    fn stats(&self) -> LifecycleStats;

    /// Get manager name for logging/monitoring
    fn name(&self) -> &'static str;

    /// Get current configuration
    fn config(&self) -> &Self::Config;

    /// Update configuration (if supported)
    /// 
    /// Default implementation returns error - managers that support
    /// dynamic reconfiguration should override this.
    async fn update_config(&mut self, _config: Self::Config) -> Result<(), Self::Error> {
        Err(Self::Error::from(BlixardError::NotSupported {
            operation: "update_config".to_string(),
            reason: format!("{} does not support dynamic reconfiguration", self.name()),
        }))
    }

    /// Wait for manager to reach a specific state
    async fn wait_for_state(&self, target_state: LifecycleState, timeout: std::time::Duration) -> Result<(), Self::Error> {
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            if self.state() == target_state {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        
        Err(Self::Error::from(BlixardError::Timeout {
            operation: format!("wait_for_state({:?})", target_state),
            duration: timeout,
        }))
    }

    /// Check if the manager is running
    fn is_running(&self) -> bool {
        matches!(self.state(), LifecycleState::Running)
    }

    /// Check if the manager is healthy
    async fn is_healthy(&self) -> bool {
        matches!(self.health().await, HealthStatus::Healthy)
    }
}

/// Helper trait for managers that support background task management
#[async_trait]
pub trait BackgroundTaskManager: LifecycleManager {
    /// Get names of background tasks
    fn task_names(&self) -> Vec<String>;
    
    /// Check if a specific task is running
    fn is_task_running(&self, task_name: &str) -> bool;
    
    /// Restart a specific background task
    async fn restart_task(&mut self, task_name: &str) -> Result<(), Self::Error>;
    
    /// Get status of all background tasks
    fn task_status(&self) -> std::collections::HashMap<String, bool> {
        self.task_names()
            .into_iter()
            .map(|name| {
                let running = self.is_task_running(&name);
                (name, running)
            })
            .collect()
    }
}

/// Base implementation helper for common lifecycle patterns
pub struct LifecycleBase<C> {
    pub config: C,
    pub state: LifecycleState,
    pub stats: LifecycleStats,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub shutdown_rx: Option<oneshot::Receiver<()>>,
}

impl<C: Clone + Debug> LifecycleBase<C> {
    pub fn new(config: C) -> Self {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        Self {
            config,
            state: LifecycleState::Created,
            stats: LifecycleStats::new(),
            shutdown_tx: Some(shutdown_tx),
            shutdown_rx: Some(shutdown_rx),
        }
    }

    pub fn transition_state(&mut self, new_state: LifecycleState) {
        match (&self.state, &new_state) {
            (LifecycleState::Running, LifecycleState::Starting) => {
                self.stats.record_restart();
            }
            (_, LifecycleState::Starting) => {
                self.stats.record_start();
            }
            (_, LifecycleState::Stopped) => {
                self.stats.record_stop();
            }
            _ => {}
        }
        
        self.state = new_state;
    }

    pub fn record_error(&mut self, error: &str) {
        self.stats.record_error(error);
        self.state = LifecycleState::Failed(error.to_string());
    }

    pub fn trigger_shutdown(&mut self) -> Result<(), ()> {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).map_err(|_| ())
        } else {
            Err(())
        }
    }

    pub fn is_shutdown_signaled(&self) -> bool {
        self.shutdown_tx.is_none()
    }
}

impl<C: Clone + Debug> Debug for LifecycleBase<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleBase")
            .field("config", &self.config)
            .field("state", &self.state)
            .field("stats", &self.stats)
            .field("shutdown_signaled", &self.is_shutdown_signaled())
            .finish()
    }
}

/// Macro to help implement LifecycleManager for existing managers
#[macro_export]
macro_rules! impl_lifecycle_manager {
    ($manager:ty, $config:ty, $state:ty, $error:ty, $name:expr) => {
        #[async_trait::async_trait]
        impl $crate::patterns::LifecycleManager for $manager {
            type Config = $config;
            type State = $state;
            type Error = $error;

            async fn new(config: Self::Config) -> Result<Self, Self::Error>
            where
                Self: Sized
            {
                <$manager>::new(config).await
            }

            async fn start(&mut self) -> Result<(), Self::Error> {
                self.start().await
            }

            async fn stop(&mut self) -> Result<(), Self::Error> {
                // Default stop implementation - override if needed
                Ok(())
            }

            fn state(&self) -> $crate::patterns::LifecycleState {
                self.lifecycle.state.clone()
            }

            fn stats(&self) -> $crate::patterns::LifecycleStats {
                self.lifecycle.stats.clone()
            }

            fn name(&self) -> &'static str {
                $name
            }

            fn config(&self) -> &Self::Config {
                &self.lifecycle.config
            }
        }
    };
}

/// Trait for objects that can be converted to a LifecycleManager
pub trait IntoLifecycleManager<T: LifecycleManager> {
    fn into_lifecycle_manager(self) -> T;
}

/// Collection of lifecycle managers for coordinated management
pub struct LifecycleCoordinator {
    managers: Vec<Box<dyn LifecycleManager<Config = (), State = (), Error = BlixardError>>>,
}

impl LifecycleCoordinator {
    pub fn new() -> Self {
        Self {
            managers: Vec::new(),
        }
    }

    pub fn add_manager<M>(&mut self, manager: M)
    where
        M: LifecycleManager<Config = (), State = (), Error = BlixardError> + 'static,
    {
        self.managers.push(Box::new(manager));
    }

    pub async fn start_all(&mut self) -> BlixardResult<()> {
        for manager in &mut self.managers {
            manager.start().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to start manager {}: {}", manager.name(), e),
            })?;
        }
        Ok(())
    }

    pub async fn stop_all(&mut self) -> BlixardResult<()> {
        for manager in &mut self.managers {
            manager.stop().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to stop manager {}: {}", manager.name(), e),
            })?;
        }
        Ok(())
    }

    pub async fn health_check_all(&self) -> Vec<(String, HealthStatus)> {
        let mut results = Vec::new();
        for manager in &self.managers {
            let health = manager.health().await;
            results.push((manager.name().to_string(), health));
        }
        results
    }
}

impl Default for LifecycleCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    // Mock manager for testing
    #[derive(Debug)]
    struct MockManager {
        lifecycle: LifecycleBase<()>,
        fail_on_start: bool,
    }

    impl MockManager {
        pub fn new_success() -> Self {
            Self {
                lifecycle: LifecycleBase::new(()),
                fail_on_start: false,
            }
        }

        pub fn new_failing() -> Self {
            Self {
                lifecycle: LifecycleBase::new(()),
                fail_on_start: true,
            }
        }
    }

    #[async_trait]
    impl LifecycleManager for MockManager {
        type Config = ();
        type State = ();
        type Error = BlixardError;

        async fn new(_config: Self::Config) -> Result<Self, Self::Error> {
            Ok(MockManager::new_success())
        }

        async fn start(&mut self) -> Result<(), Self::Error> {
            if self.fail_on_start {
                self.lifecycle.record_error("Simulated start failure");
                return Err(BlixardError::Internal {
                    message: "Simulated start failure".to_string(),
                });
            }

            self.lifecycle.transition_state(LifecycleState::Starting);
            sleep(Duration::from_millis(10)).await; // Simulate startup time
            self.lifecycle.transition_state(LifecycleState::Running);
            Ok(())
        }

        async fn stop(&mut self) -> Result<(), Self::Error> {
            self.lifecycle.transition_state(LifecycleState::Stopping);
            sleep(Duration::from_millis(5)).await; // Simulate shutdown time
            self.lifecycle.transition_state(LifecycleState::Stopped);
            Ok(())
        }

        fn state(&self) -> LifecycleState {
            self.lifecycle.state.clone()
        }

        fn stats(&self) -> LifecycleStats {
            self.lifecycle.stats.clone()
        }

        fn name(&self) -> &'static str {
            "MockManager"
        }

        fn config(&self) -> &Self::Config {
            &self.lifecycle.config
        }
    }

    #[tokio::test]
    async fn test_lifecycle_states() {
        let mut manager = MockManager::new_success();
        assert_eq!(manager.state(), LifecycleState::Created);

        manager.start().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);
        assert!(manager.is_running());

        manager.stop().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Stopped);
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_health_status() {
        let mut manager = MockManager::new_success();
        
        // Initially not healthy (not running)
        let health = manager.health().await;
        assert!(matches!(health, HealthStatus::Degraded(_)));

        // Healthy when running
        manager.start().await.unwrap();
        let health = manager.health().await;
        assert_eq!(health, HealthStatus::Healthy);
        assert!(manager.is_healthy().await);
    }

    #[tokio::test]
    async fn test_restart() {
        let mut manager = MockManager::new_success();
        
        manager.start().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);
        
        manager.restart().await.unwrap();
        assert_eq!(manager.state(), LifecycleState::Running);
        
        // Should have recorded a restart
        let stats = manager.stats();
        assert_eq!(stats.restart_count, 1);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let mut manager = MockManager::new_failing();
        
        let result = manager.start().await;
        assert!(result.is_err());
        
        // Should be in failed state
        assert!(matches!(manager.state(), LifecycleState::Failed(_)));
        
        // Health should be unhealthy
        let health = manager.health().await;
        assert!(matches!(health, HealthStatus::Unhealthy(_)));
    }

    #[tokio::test]
    async fn test_wait_for_state() {
        let mut manager = MockManager::new_success();
        
        // Start manager in background
        let manager_ref = &mut manager;
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            manager_ref.start().await.unwrap();
        });
        
        // Wait for running state
        let result = manager.wait_for_state(
            LifecycleState::Running,
            Duration::from_millis(100)
        ).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_state_timeout() {
        let manager = MockManager::new_success();
        
        // Wait for a state that will never be reached
        let result = manager.wait_for_state(
            LifecycleState::Running,
            Duration::from_millis(50)
        ).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lifecycle_stats() {
        let mut manager = MockManager::new_success();
        let mut stats = manager.stats();
        
        // Initially no start time
        assert!(stats.start_time.is_none());
        assert_eq!(stats.restart_count, 0);
        
        manager.start().await.unwrap();
        stats = manager.stats();
        
        // Should have start time after starting
        assert!(stats.start_time.is_some());
        assert!(stats.current_uptime().is_some());
    }
}