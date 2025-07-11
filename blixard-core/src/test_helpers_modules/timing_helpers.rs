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
    let multiplier = timeout_multiplier();
    // Use saturating mul to avoid overflow
    base.saturating_mul(multiplier as u32)
}

/// Wait for a condition to become true with exponential backoff
pub async fn wait_for_condition_with_backoff<F, Fut>(
    mut condition: F,
    max_wait: Duration,
    initial_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = Instant::now();
    let scaled_max = scaled_timeout(max_wait);
    let mut interval = initial_interval;
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

/// Robust sleep that accounts for CI environments
pub async fn robust_sleep(base_duration: Duration) {
    // Just sleep for the scaled duration without further manipulation
    // to avoid potential overflow in tokio's deadline calculation
    let scaled = scaled_timeout(base_duration);
    sleep(scaled).await;
}

/// Wait for a condition with simple polling (no backoff)
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

    while start.elapsed() < scaled_max {
        if condition().await {
            return Ok(());
        }

        sleep(check_interval).await;
    }

    Err(format!("Condition not met within {:?}", scaled_max))
}

/// Wait for a condition that might fail with detailed error reporting
pub async fn wait_for_condition_with_result<F, Fut, E>(
    mut condition: F,
    max_wait: Duration,
    initial_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, E>>,
    E: std::fmt::Display,
{
    let start = Instant::now();
    let scaled_max = scaled_timeout(max_wait);
    let mut interval = initial_interval;
    let max_interval = Duration::from_secs(1);
    let mut last_error = None;

    while start.elapsed() < scaled_max {
        match condition().await {
            Ok(true) => return Ok(()),
            Ok(false) => {
                // Continue waiting
            }
            Err(e) => {
                last_error = Some(format!("{}", e));
            }
        }

        sleep(interval).await;

        // Exponential backoff with cap
        interval = (interval * 2).min(max_interval);
    }

    let error_context = if let Some(error) = last_error {
        format!(" (last error: {})", error)
    } else {
        String::new()
    };

    Err(format!(
        "Condition not met within {:?}{}",
        scaled_max, error_context
    ))
}