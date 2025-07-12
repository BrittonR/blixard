//! P2P Health Check Service
//!
//! This module provides health checking capabilities for P2P connections.
//!
//! ## Migration to Clock Abstraction
//!
//! This module has been migrated from direct time operations to use the `Clock` trait.
//! This enables:
//!
//! - **Deterministic testing**: Use `MockClock` to control time in tests
//! - **Fast tests**: No need to wait for real timeouts
//! - **Predictable behavior**: Time-dependent logic is fully testable
//!
//! ### Usage Examples
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use blixard_core::p2p_health_check::P2pHealthChecker;
//! use blixard_core::abstractions::time::{SystemClock, MockClock};
//!
//! // Method 1: With custom clock (recommended for testing)
//! let clock = Arc::new(MockClock::new());
//! let checker = P2pHealthChecker::new(monitor, clock.clone());
//!
//! // Method 2: With default system clock (for production)
//! let checker = P2pHealthChecker::with_default_clock(monitor);
//! ```

use crate::abstractions::time::{Clock, ClockExt};
use crate::error::{BlixardError, BlixardResult};
use crate::iroh_types::HealthCheckRequest;
use crate::p2p_monitor::{ConnectionQuality, P2pMonitor};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

/// P2P health check service for measuring connection quality
pub struct P2pHealthChecker {
    monitor: Arc<dyn P2pMonitor>,
    timeout_duration: Duration,
    clock: Arc<dyn Clock>,
}

impl P2pHealthChecker {
    /// Create a new health checker with custom clock
    pub fn new(monitor: Arc<dyn P2pMonitor>, clock: Arc<dyn Clock>) -> Self {
        Self {
            monitor,
            timeout_duration: Duration::from_secs(5),
            clock,
        }
    }

    /// Create a new health checker with default system clock
    pub fn with_default_clock(monitor: Arc<dyn P2pMonitor>) -> Self {
        use crate::abstractions::time::SystemClock;
        Self::new(monitor, Arc::new(SystemClock::new()))
    }

    /// Perform a health check and RTT measurement for a peer
    pub async fn check_peer_health(
        &self,
        peer_id: &str,
        _client: Arc<crate::transport::iroh_client::IrohClusterServiceClient>,
    ) -> BlixardResult<f64> {
        let start = self.clock.now();

        // Send health check request using Iroh client
        let request = HealthCheckRequest {};

        match self
            .clock
            .timeout(self.timeout_duration, _client.health_check(request))
            .await
        {
            Ok(Ok(response)) => {
                let now = self.clock.now();
                let rtt_ms = start.elapsed(now).as_secs_f64() * 1000.0;
                let response = response.into_inner();

                debug!(
                    peer_id = peer_id,
                    rtt_ms = rtt_ms,
                    "Health check successful"
                );

                // Record RTT measurement
                self.monitor.record_rtt(peer_id, rtt_ms).await;

                // Verify response indicates health
                if !response.healthy {
                    warn!(
                        peer_id = peer_id,
                        message = response.message,
                        "Health check indicates unhealthy peer"
                    );
                }

                Ok(rtt_ms)
            }
            Ok(Err(e)) => {
                error!(
                    peer_id = peer_id,
                    error = %e,
                    "Health check RPC failed"
                );
                Err(BlixardError::NetworkError(format!(
                    "Health check failed for peer {}: {}",
                    peer_id, e
                )))
            }
            Err(_) => {
                error!(
                    peer_id = peer_id,
                    timeout_secs = self.timeout_duration.as_secs(),
                    "Health check timeout"
                );
                Err(BlixardError::NetworkError(format!(
                    "Health check timeout for peer {} after {:?}",
                    peer_id, self.timeout_duration
                )))
            }
        }
    }

    /// Calculate connection quality based on recent measurements
    pub async fn calculate_connection_quality(
        &self,
        peer_id: &str,
        recent_rtts: &[f64],
        success_count: usize,
        total_attempts: usize,
    ) -> ConnectionQuality {
        let success_rate = if total_attempts > 0 {
            success_count as f64 / total_attempts as f64
        } else {
            0.0
        };

        let avg_rtt = if !recent_rtts.is_empty() {
            recent_rtts.iter().sum::<f64>() / recent_rtts.len() as f64
        } else {
            999.0 // High default if no measurements
        };

        // TODO: Get actual QUIC stats for packet loss and bandwidth
        let packet_loss = 0.0; // Placeholder
        let bandwidth = 1_000_000.0; // 1 MB/s placeholder

        let quality = ConnectionQuality {
            success_rate,
            avg_rtt,
            packet_loss,
            bandwidth,
        };

        // Update connection quality in monitor
        self.monitor
            .update_connection_quality(peer_id, quality.clone())
            .await;

        quality
    }
}

/// Trait for performing P2P health checks
#[async_trait]
pub trait HealthChecker: Send + Sync {
    /// Check if a peer is healthy and measure RTT
    async fn check_health(&self, peer_id: &str) -> BlixardResult<f64>;

    /// Get connection quality metrics for a peer
    async fn get_connection_quality(&self, peer_id: &str) -> BlixardResult<ConnectionQuality>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p_monitor::NoOpMonitor;

    #[tokio::test]
    async fn test_connection_quality_calculation() {
        let monitor = Arc::new(NoOpMonitor);
        let checker = P2pHealthChecker::with_default_clock(monitor);

        // Test with good metrics
        let rtts = vec![10.0, 12.0, 11.0, 13.0];
        let quality = checker
            .calculate_connection_quality("peer1", &rtts, 95, 100)
            .await;

        assert_eq!(quality.success_rate, 0.95);
        assert_eq!(quality.avg_rtt, 11.5);
        assert!(quality.score() > 0.8); // Should have high score

        // Test with poor metrics
        let rtts = vec![500.0, 600.0, 550.0];
        let quality = checker
            .calculate_connection_quality("peer2", &rtts, 50, 100)
            .await;

        assert_eq!(quality.success_rate, 0.5);
        assert_eq!(quality.avg_rtt, 550.0);
        assert!(quality.score() < 0.5); // Should have low score
    }

    #[tokio::test]
    async fn test_with_mock_clock() {
        use crate::abstractions::time::MockClock;

        let monitor = Arc::new(NoOpMonitor);
        let mock_clock = Arc::new(MockClock::new());
        let checker = P2pHealthChecker::new(monitor, mock_clock.clone());

        // Test that we can control time in tests
        let start_time = mock_clock.current_time().await;
        assert_eq!(start_time, 0);

        // Advance time and verify it changes
        mock_clock.advance(Duration::from_millis(100)).await;
        let new_time = mock_clock.current_time().await;
        assert_eq!(new_time, 100_000); // 100ms = 100,000 microseconds

        // This demonstrates how MockClock enables deterministic testing
        // In real tests, we could simulate timeout scenarios by not advancing time
    }
}
