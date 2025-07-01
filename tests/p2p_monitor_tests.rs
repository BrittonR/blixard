#[cfg(test)]
mod p2p_monitor_tests {
    use blixard_core::p2p_monitor::{
        ConnectionQuality, ConnectionState, Direction, DiscoveryMethod, P2pErrorType, P2pMonitor,
    };
    use blixard_core::p2p_monitor_otel::OtelP2pMonitor;
    use blixard_core::metrics_otel::Metrics;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    /// Mock monitor for testing that records all calls
    struct MockP2pMonitor {
        connection_attempts: Arc<Mutex<Vec<(String, bool)>>>,
        state_changes: Arc<Mutex<Vec<(String, ConnectionState, ConnectionState)>>>,
        bytes_transferred: Arc<Mutex<Vec<(String, Direction, u64)>>>,
        messages: Arc<Mutex<Vec<(String, String, usize, Option<f64>)>>>,
        rtts: Arc<Mutex<Vec<(String, f64)>>>,
        errors: Arc<Mutex<Vec<(String, P2pErrorType)>>>,
        discoveries: Arc<Mutex<Vec<(String, DiscoveryMethod)>>>,
    }

    impl MockP2pMonitor {
        fn new() -> Self {
            Self {
                connection_attempts: Arc::new(Mutex::new(Vec::new())),
                state_changes: Arc::new(Mutex::new(Vec::new())),
                bytes_transferred: Arc::new(Mutex::new(Vec::new())),
                messages: Arc::new(Mutex::new(Vec::new())),
                rtts: Arc::new(Mutex::new(Vec::new())),
                errors: Arc::new(Mutex::new(Vec::new())),
                discoveries: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl P2pMonitor for MockP2pMonitor {
        async fn record_connection_attempt(&self, peer_id: &str, success: bool) {
            self.connection_attempts
                .lock()
                .await
                .push((peer_id.to_string(), success));
        }

        async fn record_connection_state_change(
            &self,
            peer_id: &str,
            from: ConnectionState,
            to: ConnectionState,
        ) {
            self.state_changes
                .lock()
                .await
                .push((peer_id.to_string(), from, to));
        }

        async fn record_bytes_transferred(
            &self,
            peer_id: &str,
            direction: Direction,
            bytes: u64,
        ) {
            self.bytes_transferred
                .lock()
                .await
                .push((peer_id.to_string(), direction, bytes));
        }

        async fn record_message(
            &self,
            peer_id: &str,
            message_type: &str,
            size: usize,
            latency_ms: Option<f64>,
        ) {
            self.messages.lock().await.push((
                peer_id.to_string(),
                message_type.to_string(),
                size,
                latency_ms,
            ));
        }

        async fn record_rtt(&self, peer_id: &str, rtt_ms: f64) {
            self.rtts
                .lock()
                .await
                .push((peer_id.to_string(), rtt_ms));
        }

        async fn record_error(&self, peer_id: &str, error_type: P2pErrorType) {
            self.errors
                .lock()
                .await
                .push((peer_id.to_string(), error_type));
        }

        async fn record_peer_discovered(&self, peer_id: &str, method: DiscoveryMethod) {
            self.discoveries
                .lock()
                .await
                .push((peer_id.to_string(), method));
        }

        async fn update_connection_quality(&self, _peer_id: &str, _quality: ConnectionQuality) {
            // Not tracked in mock
        }

        async fn record_pool_metrics(&self, _total: usize, _active: usize, _idle: usize) {
            // Not tracked in mock
        }

        async fn record_buffered_messages(&self, _peer_id: &str, _count: usize, _bytes: usize) {
            // Not tracked in mock
        }

        async fn record_resource_sync(
            &self,
            _peer_id: &str,
            _resource_type: &str,
            _bytes: u64,
            _duration_ms: f64,
            _success: bool,
        ) {
            // Not tracked in mock
        }
    }

    #[tokio::test]
    async fn test_connection_state_transitions() {
        let monitor = MockP2pMonitor::new();

        // Test normal connection flow
        monitor
            .record_connection_state_change("peer1", ConnectionState::Disconnected, ConnectionState::Connecting)
            .await;
        monitor
            .record_connection_state_change("peer1", ConnectionState::Connecting, ConnectionState::Connected)
            .await;
        monitor
            .record_connection_attempt("peer1", true)
            .await;

        let state_changes = monitor.state_changes.lock().await;
        assert_eq!(state_changes.len(), 2);
        assert_eq!(state_changes[0].1, ConnectionState::Disconnected);
        assert_eq!(state_changes[0].2, ConnectionState::Connecting);
        assert_eq!(state_changes[1].1, ConnectionState::Connecting);
        assert_eq!(state_changes[1].2, ConnectionState::Connected);

        let attempts = monitor.connection_attempts.lock().await;
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].1, true);
    }

    #[tokio::test]
    async fn test_connection_failure_tracking() {
        let monitor = MockP2pMonitor::new();

        // Test connection failure
        monitor
            .record_connection_state_change("peer2", ConnectionState::Disconnected, ConnectionState::Connecting)
            .await;
        monitor
            .record_connection_attempt("peer2", false)
            .await;
        monitor
            .record_error("peer2", P2pErrorType::Timeout)
            .await;
        monitor
            .record_connection_state_change("peer2", ConnectionState::Connecting, ConnectionState::Failed)
            .await;

        let state_changes = monitor.state_changes.lock().await;
        assert_eq!(state_changes.len(), 2);
        assert_eq!(state_changes[1].2, ConnectionState::Failed);

        let attempts = monitor.connection_attempts.lock().await;
        assert_eq!(attempts[0].1, false);

        let errors = monitor.errors.lock().await;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, P2pErrorType::Timeout);
    }

    #[tokio::test]
    async fn test_data_transfer_tracking() {
        let monitor = MockP2pMonitor::new();

        // Track outbound transfer
        monitor
            .record_bytes_transferred("peer3", Direction::Outbound, 1024)
            .await;
        monitor
            .record_message("peer3", "CreateVm", 512, Some(25.5))
            .await;

        // Track inbound transfer
        monitor
            .record_bytes_transferred("peer3", Direction::Inbound, 2048)
            .await;
        monitor
            .record_message("peer3", "VmStatus", 256, Some(15.3))
            .await;

        let bytes = monitor.bytes_transferred.lock().await;
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0].1, Direction::Outbound);
        assert_eq!(bytes[0].2, 1024);
        assert_eq!(bytes[1].1, Direction::Inbound);
        assert_eq!(bytes[1].2, 2048);

        let messages = monitor.messages.lock().await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].1, "CreateVm");
        assert_eq!(messages[0].3, Some(25.5));
        assert_eq!(messages[1].1, "VmStatus");
        assert_eq!(messages[1].3, Some(15.3));
    }

    #[tokio::test]
    async fn test_rtt_measurements() {
        let monitor = MockP2pMonitor::new();

        // Record multiple RTT measurements
        let rtts = vec![10.5, 12.3, 11.1, 13.7, 10.9];
        for rtt in &rtts {
            monitor.record_rtt("peer4", *rtt).await;
        }

        let recorded_rtts = monitor.rtts.lock().await;
        assert_eq!(recorded_rtts.len(), 5);
        for (i, (peer, rtt)) in recorded_rtts.iter().enumerate() {
            assert_eq!(peer, "peer4");
            assert_eq!(*rtt, rtts[i]);
        }
    }

    #[tokio::test]
    async fn test_peer_discovery() {
        let monitor = MockP2pMonitor::new();

        // Test different discovery methods
        monitor
            .record_peer_discovered("peer5", DiscoveryMethod::Dns)
            .await;
        monitor
            .record_peer_discovered("peer6", DiscoveryMethod::Mdns)
            .await;
        monitor
            .record_peer_discovered("peer7", DiscoveryMethod::Static)
            .await;

        let discoveries = monitor.discoveries.lock().await;
        assert_eq!(discoveries.len(), 3);
        assert_eq!(discoveries[0].1, DiscoveryMethod::Dns);
        assert_eq!(discoveries[1].1, DiscoveryMethod::Mdns);
        assert_eq!(discoveries[2].1, DiscoveryMethod::Static);
    }

    #[tokio::test]
    async fn test_connection_quality_score() {
        // Test connection quality scoring
        let quality = ConnectionQuality {
            success_rate: 0.95,
            avg_rtt: 25.0,
            packet_loss: 0.02,
            bandwidth: 5_000_000.0, // 5 MB/s
        };

        let score = quality.score();
        assert!(score > 0.8); // Should be high quality
        assert!(score <= 1.0);

        // Test poor quality
        let poor_quality = ConnectionQuality {
            success_rate: 0.5,
            avg_rtt: 500.0,
            packet_loss: 0.15,
            bandwidth: 100_000.0, // 100 KB/s
        };

        let poor_score = poor_quality.score();
        assert!(poor_score < 0.5); // Should be low quality
        assert!(poor_score >= 0.0);
    }

    #[tokio::test]
    async fn test_error_type_coverage() {
        let monitor = MockP2pMonitor::new();

        // Test all error types
        let error_types = vec![
            P2pErrorType::Timeout,
            P2pErrorType::ConnectionRefused,
            P2pErrorType::ConnectionReset,
            P2pErrorType::TlsError,
            P2pErrorType::DnsResolution,
            P2pErrorType::ProtocolError,
            P2pErrorType::Unknown,
        ];

        for (i, error_type) in error_types.iter().enumerate() {
            monitor
                .record_error(&format!("peer{}", i), *error_type)
                .await;
        }

        let errors = monitor.errors.lock().await;
        assert_eq!(errors.len(), error_types.len());
        for (i, (_, error_type)) in errors.iter().enumerate() {
            assert_eq!(*error_type, error_types[i]);
        }
    }

    #[tokio::test]
    async fn test_connection_state_as_str() {
        assert_eq!(ConnectionState::Disconnected.as_str(), "disconnected");
        assert_eq!(ConnectionState::Connecting.as_str(), "connecting");
        assert_eq!(ConnectionState::Connected.as_str(), "connected");
        assert_eq!(ConnectionState::Disconnecting.as_str(), "disconnecting");
        assert_eq!(ConnectionState::Failed.as_str(), "failed");
    }

    #[tokio::test]
    async fn test_direction_as_str() {
        assert_eq!(Direction::Inbound.as_str(), "inbound");
        assert_eq!(Direction::Outbound.as_str(), "outbound");
    }

    #[tokio::test]
    async fn test_discovery_method_as_str() {
        assert_eq!(DiscoveryMethod::Dns.as_str(), "dns");
        assert_eq!(DiscoveryMethod::Mdns.as_str(), "mdns");
        assert_eq!(DiscoveryMethod::Static.as_str(), "static");
        assert_eq!(DiscoveryMethod::Relay.as_str(), "relay");
        assert_eq!(DiscoveryMethod::Manual.as_str(), "manual");
    }

    #[tokio::test]
    async fn test_p2p_error_type_as_str() {
        assert_eq!(P2pErrorType::Timeout.as_str(), "timeout");
        assert_eq!(P2pErrorType::ConnectionRefused.as_str(), "refused");
        assert_eq!(P2pErrorType::ConnectionReset.as_str(), "reset");
        assert_eq!(P2pErrorType::TlsError.as_str(), "tls");
        assert_eq!(P2pErrorType::DnsResolution.as_str(), "dns");
        assert_eq!(P2pErrorType::ProtocolError.as_str(), "protocol");
        assert_eq!(P2pErrorType::Unknown.as_str(), "unknown");
    }

    #[tokio::test]
    #[ignore] // Requires actual metrics setup
    async fn test_otel_p2p_monitor_integration() {
        // This would test the actual OtelP2pMonitor with a real metrics instance
        // Marked as ignore since it requires full OpenTelemetry setup
        
        // Example of how it would work:
        // let metrics = Arc::new(Metrics::new(...));
        // let monitor = OtelP2pMonitor::new(metrics);
        // 
        // monitor.record_connection_attempt("peer1", true).await;
        // monitor.record_rtt("peer1", 25.5).await;
        // 
        // // Verify metrics were recorded properly
    }
}