//! Failpoint integration for fault injection testing
//!
//! This module provides macros and utilities for injecting failures in tests
//! while maintaining compatibility with the Blixard error system.

use crate::error::BlixardError;

/// Initialize failpoints for testing
/// Must be called at the start of tests that use failpoints
#[cfg(feature = "failpoints")]
pub fn init() {
    fail::cfg("blixard", "off").ok();
}

/// Macro to inject a failpoint that returns a BlixardError
///
/// Usage:
/// ```ignore
/// fail_point!("node::initialize", |msg| {
///     BlixardError::Internal { message: msg.unwrap_or("failpoint triggered").to_string() }
/// });
/// ```
#[macro_export]
#[cfg(feature = "failpoints")]
macro_rules! fail_point {
    ($name:expr) => {{
        fail::fail_point!($name, |_| {
            return Err($crate::error::BlixardError::Internal {
                message: concat!("Failpoint triggered: ", $name).to_string(),
            });
        });
    }};
    ($name:expr, $err:expr) => {{
        fail::fail_point!($name, |_| {
            return Err($err);
        });
    }};
    ($name:expr, |$msg:ident| $body:expr) => {{
        fail::fail_point!($name, |$msg| {
            return Err($body);
        });
    }};
}

/// Macro for failpoints that don't return errors (e.g., panics, sleeps)
#[macro_export]
#[cfg(feature = "failpoints")]
macro_rules! fail_point_action {
    ($name:expr, $action:expr) => {{
        fail::fail_point!($name, |_| { $action });
    }};
}

/// No-op versions for when failpoints feature is disabled
#[macro_export]
#[cfg(not(feature = "failpoints"))]
macro_rules! fail_point {
    ($name:expr) => {};
    ($name:expr, $err:expr) => {};
    ($name:expr, |$msg:ident| $body:expr) => {};
}

#[macro_export]
#[cfg(not(feature = "failpoints"))]
macro_rules! fail_point_action {
    ($name:expr, $action:expr) => {};
}

/// Common failpoint scenarios for testing
#[cfg(feature = "failpoints")]
pub mod scenarios {
    use super::*;
    use std::time::Duration;

    /// Configure a failpoint to fail with a specific error
    pub fn fail_with_error(name: &str, error: BlixardError) {
        fail::cfg(name, "return").ok();
        // Note: We can't use cfg_callback with BlixardError because it doesn't implement Clone
        // This function is provided for API compatibility but has limited functionality
    }

    /// Configure a failpoint to fail with a percentage chance
    pub fn fail_with_probability(name: &str, probability: f64) {
        let cfg = format!("{}%return", (probability * 100.0) as u32);
        fail::cfg(name, &cfg).ok();
    }

    /// Configure a failpoint to pause execution
    pub fn pause(name: &str, duration: Duration) {
        fail::cfg(name, "pause").ok();
        fail::cfg_callback(name, move || {
            std::thread::sleep(duration);
        })
        .ok();
    }

    /// Configure a failpoint to panic
    pub fn panic(name: &str, message: &str) {
        let msg = message.to_string();
        fail::cfg(name, "panic").ok();
        fail::cfg_callback(name, move || {
            panic!("{}", msg);
        })
        .ok();
    }

    /// Configure a failpoint to fail after N calls
    pub fn fail_after_n(name: &str, n: usize) {
        let cfg = format!("{}-1*off->return", n);
        fail::cfg(name, &cfg).ok();
    }

    /// Disable a failpoint
    pub fn disable(name: &str) {
        fail::cfg(name, "off").ok();
    }

    /// Disable all failpoints
    pub fn disable_all() {
        fail::cfg("blixard/**", "off").ok();
    }
}

/// Test helper to run code with a failpoint enabled
#[cfg(feature = "failpoints")]
pub async fn with_failpoint<F, R>(name: &str, config: &str, f: F) -> R
where
    F: std::future::Future<Output = R>,
{
    fail::cfg(name, config).map_err(|e| {
        tracing::error!("Failed to configure failpoint '{}': {}", name, e);
    }).ok();
    let result = f.await;
    fail::cfg(name, "off").map_err(|e| {
        tracing::error!("Failed to disable failpoint '{}': {}", name, e);
    }).ok();
    result
}

/// Test helper to run code with multiple failpoints
#[cfg(feature = "failpoints")]
pub async fn with_failpoints<F, R>(failpoints: &[(&str, &str)], f: F) -> R
where
    F: std::future::Future<Output = R>,
{
    // Enable all failpoints
    for (name, config) in failpoints {
        fail::cfg(*name, *config).map_err(|e| {
            tracing::error!("Failed to configure failpoint '{}': {}", name, e);
        }).ok();
    }

    let result = f.await;

    // Disable all failpoints
    for (name, _) in failpoints {
        fail::cfg(*name, "off").map_err(|e| {
            tracing::error!("Failed to disable failpoint '{}': {}", name, e);
        }).ok();
    }

    result
}

#[cfg(test)]
#[cfg(feature = "failpoints")]
mod tests {
    use super::*;

    #[test]
    fn test_failpoint_macro() {
        init();

        fn may_fail() -> BlixardResult<()> {
            fail_point!("test::fail");
            Ok(())
        }

        // Should succeed when disabled
        assert!(may_fail().is_ok());

        // Enable failpoint
        fail::cfg("test::fail", "return").unwrap();
        assert!(may_fail().is_err());

        // Disable failpoint
        fail::cfg("test::fail", "off").unwrap();
        assert!(may_fail().is_ok());
    }

    #[test]
    fn test_custom_error() {
        init();

        fn may_fail_custom() -> BlixardResult<()> {
            fail_point!(
                "test::custom",
                BlixardError::Storage {
                    operation: "test".to_string(),
                    source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "injected")),
                }
            );
            Ok(())
        }

        fail::cfg("test::custom", "return").unwrap();
        match may_fail_custom() {
            Err(BlixardError::Storage { operation, .. }) => {
                assert_eq!(operation, "test");
            }
            _ => panic!("Expected storage error"),
        }
    }

    #[tokio::test]
    async fn test_with_failpoint_helper() {
        init();

        let result = with_failpoint("test::helper", "50%return", async {
            let mut successes = 0;
            let mut failures = 0;

            for _ in 0..100 {
                fn may_fail() -> BlixardResult<()> {
                    fail_point!("test::helper");
                    Ok(())
                }

                match may_fail() {
                    Ok(()) => successes += 1,
                    Err(_) => failures += 1,
                }
            }

            (successes, failures)
        })
        .await;

        // With 50% probability, we should see both successes and failures
        assert!(result.0 > 0);
        assert!(result.1 > 0);
    }
}
