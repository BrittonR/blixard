//! Virtual machine configuration

use super::defaults::*;
use super::parse_duration_secs_from_env;
use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Virtual machine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmConfig {
    /// Health check interval
    #[serde(with = "humantime_serde")]
    pub health_check_interval: Duration,

    /// VM startup timeout
    #[serde(with = "humantime_serde")]
    pub startup_timeout: Duration,

    /// VM shutdown timeout
    #[serde(with = "humantime_serde")]
    pub shutdown_timeout: Duration,

    /// Delay before restarting a failed VM
    #[serde(with = "humantime_serde")]
    pub restart_delay: Duration,

    /// Maximum restart attempts
    pub max_restart_attempts: u32,

    /// Console socket path prefix
    pub console_socket_prefix: String,

    /// Default VM resources
    pub default_resources: VmResources,

    /// VM scheduling configuration
    pub scheduling: VmSchedulingConfig,

    /// VM recovery configuration
    pub recovery: VmRecoveryConfig,
}

/// Default VM resource allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmResources {
    /// Default CPU cores
    pub cpu_cores: u32,

    /// Default memory in MB
    pub memory_mb: u32,

    /// Default disk size in GB
    pub disk_gb: u32,

    /// Maximum CPU cores per VM
    pub max_cpu_cores: u32,

    /// Maximum memory per VM in MB
    pub max_memory_mb: u32,

    /// Maximum disk size per VM in GB
    pub max_disk_gb: u32,
}

/// VM scheduling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmSchedulingConfig {
    /// Default scheduling strategy
    pub default_strategy: String,

    /// Enable preemption
    pub enable_preemption: bool,

    /// Overcommit ratio for CPU
    pub cpu_overcommit_ratio: f64,

    /// Overcommit ratio for memory
    pub memory_overcommit_ratio: f64,

    /// Placement timeout
    #[serde(with = "humantime_serde")]
    pub placement_timeout: Duration,
}

/// VM recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VmRecoveryConfig {
    /// Enable automatic recovery
    pub enabled: bool,

    /// Recovery check interval
    #[serde(with = "humantime_serde")]
    pub check_interval: Duration,

    /// Maximum recovery attempts
    pub max_attempts: u32,

    /// Recovery backoff multiplier
    pub backoff_multiplier: f64,

    /// Maximum recovery delay
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            health_check_interval: duration_secs(DEFAULT_HEALTH_CHECK_INTERVAL_SECS),
            startup_timeout: duration_secs(DEFAULT_VM_STARTUP_TIMEOUT_SECS),
            shutdown_timeout: duration_secs(DEFAULT_VM_SHUTDOWN_TIMEOUT_SECS),
            restart_delay: duration_secs(DEFAULT_VM_RESTART_DELAY_SECS),
            max_restart_attempts: DEFAULT_MAX_VM_RESTART_ATTEMPTS,
            console_socket_prefix: DEFAULT_VM_CONSOLE_SOCKET_PREFIX.to_string(),
            default_resources: VmResources::default(),
            scheduling: VmSchedulingConfig::default(),
            recovery: VmRecoveryConfig::default(),
        }
    }
}

impl Default for VmResources {
    fn default() -> Self {
        Self {
            cpu_cores: 1,
            memory_mb: 512,
            disk_gb: 10,
            max_cpu_cores: 32,
            max_memory_mb: 65536, // 64GB
            max_disk_gb: 1000,    // 1TB
        }
    }
}

impl Default for VmSchedulingConfig {
    fn default() -> Self {
        Self {
            default_strategy: "MostAvailable".to_string(),
            enable_preemption: false,
            cpu_overcommit_ratio: 1.0,
            memory_overcommit_ratio: 1.0,
            placement_timeout: Duration::from_secs(30),
        }
    }
}

impl Default for VmRecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(60),
            max_attempts: 3,
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(300),
        }
    }
}

impl VmConfig {
    /// Load VM configuration from environment variables
    pub fn from_env() -> BlixardResult<Self> {
        let mut config = Self::default();

        config.health_check_interval = parse_duration_secs_from_env(
            "BLIXARD_VM_HEALTH_CHECK_INTERVAL_SECS",
            config.health_check_interval,
        );

        config.startup_timeout =
            parse_duration_secs_from_env("BLIXARD_VM_STARTUP_TIMEOUT_SECS", config.startup_timeout);

        config.shutdown_timeout = parse_duration_secs_from_env(
            "BLIXARD_VM_SHUTDOWN_TIMEOUT_SECS",
            config.shutdown_timeout,
        );

        config.restart_delay =
            parse_duration_secs_from_env("BLIXARD_VM_RESTART_DELAY_SECS", config.restart_delay);

        if let Ok(val) = std::env::var("BLIXARD_VM_MAX_RESTART_ATTEMPTS") {
            config.max_restart_attempts = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_VM_MAX_RESTART_ATTEMPTS".to_string())
            })?;
        }

        if let Ok(val) = std::env::var("BLIXARD_VM_CONSOLE_SOCKET_PREFIX") {
            config.console_socket_prefix = val;
        }

        // Load resource defaults
        if let Ok(val) = std::env::var("BLIXARD_VM_DEFAULT_CPU_CORES") {
            config.default_resources.cpu_cores = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_VM_DEFAULT_CPU_CORES".to_string())
            })?;
        }

        if let Ok(val) = std::env::var("BLIXARD_VM_DEFAULT_MEMORY_MB") {
            config.default_resources.memory_mb = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_VM_DEFAULT_MEMORY_MB".to_string())
            })?;
        }

        // Load scheduling config
        if let Ok(val) = std::env::var("BLIXARD_VM_SCHEDULING_STRATEGY") {
            config.scheduling.default_strategy = val;
        }

        if let Ok(val) = std::env::var("BLIXARD_VM_ENABLE_PREEMPTION") {
            config.scheduling.enable_preemption = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_VM_ENABLE_PREEMPTION".to_string())
            })?;
        }

        // Load recovery config
        if let Ok(val) = std::env::var("BLIXARD_VM_RECOVERY_ENABLED") {
            config.recovery.enabled = val.parse().map_err(|_| {
                BlixardError::ConfigError("Invalid BLIXARD_VM_RECOVERY_ENABLED".to_string())
            })?;
        }

        Ok(config)
    }

    /// Validate VM configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Validate intervals
        if self.health_check_interval < Duration::from_secs(5) {
            return Err(BlixardError::ConfigError(
                "health_check_interval too small (min 5s)".to_string(),
            ));
        }

        if self.startup_timeout < Duration::from_secs(30) {
            return Err(BlixardError::ConfigError(
                "startup_timeout too small (min 30s)".to_string(),
            ));
        }

        // Validate resources
        if self.default_resources.cpu_cores == 0 {
            return Err(BlixardError::ConfigError(
                "default_resources.cpu_cores must be at least 1".to_string(),
            ));
        }

        if self.default_resources.memory_mb < 128 {
            return Err(BlixardError::ConfigError(
                "default_resources.memory_mb too small (min 128MB)".to_string(),
            ));
        }

        if self.default_resources.cpu_cores > self.default_resources.max_cpu_cores {
            return Err(BlixardError::ConfigError(
                "default cpu_cores exceeds max_cpu_cores".to_string(),
            ));
        }

        // Validate scheduling
        let valid_strategies = ["MostAvailable", "LeastAvailable", "RoundRobin", "Random"];
        if !valid_strategies.contains(&self.scheduling.default_strategy.as_str()) {
            return Err(BlixardError::ConfigError(format!(
                "Invalid scheduling strategy: {}",
                self.scheduling.default_strategy
            )));
        }

        if self.scheduling.cpu_overcommit_ratio < 0.5 || self.scheduling.cpu_overcommit_ratio > 10.0
        {
            return Err(BlixardError::ConfigError(
                "cpu_overcommit_ratio must be between 0.5 and 10.0".to_string(),
            ));
        }

        // Validate recovery
        if self.recovery.backoff_multiplier < 1.0 || self.recovery.backoff_multiplier > 10.0 {
            return Err(BlixardError::ConfigError(
                "recovery.backoff_multiplier must be between 1.0 and 10.0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_vm_config_is_valid() {
        let config = VmConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_resources() {
        let mut config = VmConfig::default();
        config.default_resources.cpu_cores = 0;
        assert!(config.validate().is_err());

        config.default_resources.cpu_cores = 100;
        config.default_resources.max_cpu_cores = 10;
        assert!(config.validate().is_err());
    }
}
