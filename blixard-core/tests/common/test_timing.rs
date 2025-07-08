//! Robust timing utilities for distributed system tests
//!
//! This module provides timing utilities that help reduce test flakiness
//! by implementing condition-based waiting, exponential backoff, and
//! environment-aware timeout multipliers.

use std::env;
use std::future::Future;
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// Get a timeout multiplier based on the environment
/// CI environments often need longer timeouts due to resource constraints
pub fn timeout_multiplier() -> u64 {
    // Check for CI environment variables
    if env::var("CI").is_ok() || env::var("GITHUB_ACTIONS").is_ok() {
        return 3; // 3x slower in CI
    }

    // Allow manual override
    if let Ok(multiplier) = env::var("TEST_TIMEOUT_MULTIPLIER") {
        if let Ok(m) = multiplier.parse::<u64>() {
            return m;
        }
    }

    1 // Normal speed for local development
}

/// Apply the timeout multiplier to a duration
pub fn scaled_timeout(base: Duration) -> Duration {
    base * timeout_multiplier() as u32
}

/// Wait for a condition to become true with exponential backoff
///
/// This is more robust than fixed sleep durations as it:
/// - Returns as soon as the condition is met
/// - Increases wait time between checks to reduce CPU usage
/// - Has a maximum timeout to prevent hanging
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    max_wait: Duration,
    check_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = Instant::now();
    let scaled_max = scaled_timeout(max_wait);
    let mut interval = check_interval;
    let max_interval = Duration::from_secs(1);

    while start.elapsed() < scaled_max {
        if condition().await {
            return Ok(());
        }

        sleep(interval).await;

        // Exponential backoff with cap
        interval = (interval * 2).min(max_interval);
    }

    Err(format!("Condition not met within {:?}", scaled_max))
}

/// Wait for a async condition with retry logic
pub async fn wait_for_async<F, Fut, T, E>(
    mut operation: F,
    max_attempts: u32,
    initial_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = initial_delay;
    let max_delay = Duration::from_secs(5);

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt == max_attempts => return Err(e),
            Err(e) => {
                tracing::debug!(
                    "Attempt {}/{} failed: {}. Retrying in {:?}",
                    attempt,
                    max_attempts,
                    e,
                    delay
                );
                sleep(delay).await;
                delay = (delay * 2).min(max_delay);
            }
        }
    }

    unreachable!()
}

/// Connect to a service with exponential backoff retry
pub async fn connect_with_retry<F, Fut, T, E>(
    connect_fn: F,
    service_name: &str,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    wait_for_async(
        connect_fn,
        10 * timeout_multiplier() as u32, // More attempts in CI
        Duration::from_millis(100),
    )
    .await
    .map_err(|e| format!("Failed to connect to {}: {}", service_name, e))
}

/// Wait for a service to be ready by checking health
pub async fn wait_for_service_ready<F, Fut>(
    mut health_check: F,
    service_name: &str,
    startup_timeout: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, Box<dyn std::error::Error>>>,
{
    let scaled_timeout = scaled_timeout(startup_timeout);
    let start = Instant::now();
    let mut interval = Duration::from_millis(50);
    let max_interval = Duration::from_secs(1);

    while start.elapsed() < scaled_timeout {
        match health_check().await {
            Ok(true) => return Ok(()),
            _ => {}
        }

        sleep(interval).await;
        interval = (interval * 2).min(max_interval);
    }

    Err(format!(
        "{} failed to become ready within {:?}",
        service_name, scaled_timeout
    ))
}

/// Robust sleep that accounts for CI environments
pub async fn robust_sleep(base_duration: Duration) {
    sleep(scaled_timeout(base_duration)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_wait_for_condition_immediate() {
        let result = wait_for_condition(
            || async { true },
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_wait_for_condition_delayed() {
        let start = Instant::now();
        let count = Arc::new(Mutex::new(0));
        let count_clone = count.clone();

        let result = wait_for_condition(
            move || {
                let count = count_clone.clone();
                async move {
                    let mut c = count.lock().unwrap();
                    *c += 1;
                    *c >= 3
                }
            },
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_wait_for_condition_timeout() {
        let result = wait_for_condition(
            || async { false },
            Duration::from_millis(100),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_exponential_backoff() {
        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = wait_for_async(
            move || {
                let attempts = attempts_clone.clone();
                async move {
                    let mut a = attempts.lock().unwrap();
                    *a += 1;
                    if *a < 3 {
                        Err("Not ready")
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            Duration::from_millis(10),
        )
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(*attempts.lock().unwrap(), 3);
    }
}
