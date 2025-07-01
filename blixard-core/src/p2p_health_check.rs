use crate::error::{BlixardError, BlixardResult};
use crate::p2p_monitor::{ConnectionQuality, P2pMonitor};
use async_trait::async_trait;
use redb::proto::cluster_service_client::ClusterServiceClient;
use redb::proto::{HealthCheckRequest, HealthCheckResponse};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tonic::transport::Channel;
use tracing::{debug, error, warn};

/// P2P health check service for measuring connection quality
pub struct P2pHealthChecker {
    monitor: Arc<dyn P2pMonitor>,
    timeout_duration: Duration,
}

impl P2pHealthChecker {
    pub fn new(monitor: Arc<dyn P2pMonitor>) -> Self {
        Self {
            monitor,
            timeout_duration: Duration::from_secs(5),
        }
    }
    
    /// Perform a health check and RTT measurement for a peer
    pub async fn check_peer_health(
        &self,
        peer_id: &str,
        channel: Channel,
    ) -> BlixardResult<f64> {
        let start = Instant::now();
        
        // Create gRPC client
        let mut client = ClusterServiceClient::new(channel);
        
        // Send health check request
        let request = tonic::Request::new(HealthCheckRequest {
            timestamp: chrono::Utc::now().timestamp_millis(),
            echo_data: format!("ping-{}", peer_id),
        });
        
        match timeout(self.timeout_duration, client.health_check(request)).await {
            Ok(Ok(response)) => {
                let rtt_ms = start.elapsed().as_secs_f64() * 1000.0;
                let response = response.into_inner();
                
                debug!(
                    peer_id = peer_id,
                    rtt_ms = rtt_ms,
                    "Health check successful"
                );
                
                // Record RTT measurement
                self.monitor.record_rtt(peer_id, rtt_ms).await;
                
                // Verify echo data matches
                if response.echo_data != format!("ping-{}", peer_id) {
                    warn!(
                        peer_id = peer_id,
                        expected = format!("ping-{}", peer_id),
                        actual = response.echo_data,
                        "Health check echo data mismatch"
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
                Err(BlixardError::Timeout {
                    operation: format!("health check for peer {}", peer_id),
                    duration: self.timeout_duration,
                })
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
        self.monitor.update_connection_quality(peer_id, quality.clone()).await;
        
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
        let checker = P2pHealthChecker::new(monitor);
        
        // Test with good metrics
        let rtts = vec![10.0, 12.0, 11.0, 13.0];
        let quality = checker.calculate_connection_quality("peer1", &rtts, 95, 100).await;
        
        assert_eq!(quality.success_rate, 0.95);
        assert_eq!(quality.avg_rtt, 11.5);
        assert!(quality.score() > 0.8); // Should have high score
        
        // Test with poor metrics
        let rtts = vec![500.0, 600.0, 550.0];
        let quality = checker.calculate_connection_quality("peer2", &rtts, 50, 100).await;
        
        assert_eq!(quality.success_rate, 0.5);
        assert_eq!(quality.avg_rtt, 550.0);
        assert!(quality.score() < 0.5); // Should have low score
    }
}