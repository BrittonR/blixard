# P2P Connection Monitoring Design

## Overview

This document outlines the comprehensive P2P connection monitoring implementation for Blixard's Iroh-based networking layer. The monitoring system provides visibility into connection health, performance, and reliability across the distributed cluster.

## Metrics Schema

### 1. Connection Lifecycle Metrics

#### Connection States
```rust
// Counter metrics for state transitions
p2p_connection_state_transitions_total{from_state, to_state, peer_id}
p2p_connection_attempts_total{peer_id, result}  // result: success|failed|timeout
p2p_connection_duration_seconds{peer_id}  // Histogram

// Gauge metrics for current state
p2p_connections_active{state}  // state: connecting|connected|disconnecting
p2p_connections_total  // Total connections in pool
p2p_connections_idle  // Idle connections ready for reuse
```

#### Connection Errors
```rust
p2p_connection_errors_total{peer_id, error_type}  // error_type: timeout|refused|reset|tls|dns
p2p_circuit_breaker_state{peer_id, state}  // state: closed|open|half_open
p2p_circuit_breaker_failures_total{peer_id}
```

### 2. Performance Metrics

#### Latency Measurements
```rust
p2p_rtt_milliseconds{peer_id}  // Histogram - Round-trip time
p2p_ping_latency_milliseconds{peer_id}  // Gauge - Latest ping
p2p_message_latency_milliseconds{peer_id, message_type}  // Histogram
p2p_handshake_duration_milliseconds{peer_id}  // Histogram
```

#### Bandwidth & Throughput
```rust
p2p_bytes_sent_total{peer_id, direction}  // direction: inbound|outbound
p2p_bytes_received_total{peer_id}
p2p_messages_sent_total{peer_id, message_type}
p2p_messages_received_total{peer_id, message_type}
p2p_bandwidth_bytes_per_second{peer_id, direction}  // Gauge - Moving average
p2p_message_size_bytes{peer_id, message_type}  // Histogram
```

### 3. Discovery & Topology Metrics

#### Peer Discovery
```rust
p2p_discovered_peers_total{discovery_method}  // method: dns|mdns|static|relay
p2p_discovery_duration_milliseconds{method}  // Histogram
p2p_discovery_failures_total{method, reason}
p2p_known_peers_total  // Gauge
```

#### Network Topology
```rust
p2p_peer_distance_hops{peer_id}  // Network distance
p2p_cluster_diameter_hops  // Max distance between any two nodes
p2p_peer_connectivity_score{peer_id}  // 0-1 score based on success rate
```

### 4. Resource Utilization

#### Connection Pool
```rust
p2p_connection_pool_utilization_ratio  // Gauge: active/total
p2p_connection_pool_evictions_total{reason}  // reason: idle|error|capacity
p2p_connection_pool_wait_time_milliseconds  // Histogram - Time waiting for connection
p2p_connection_reuse_count{peer_id}  // Counter - Times connection reused
```

#### Message Buffering
```rust
p2p_buffered_messages_total{peer_id}  // Gauge
p2p_buffered_messages_bytes{peer_id}  // Gauge
p2p_message_buffer_overflows_total{peer_id}
p2p_message_delivery_delay_milliseconds{peer_id}  // Histogram
```

### 5. Protocol-Specific Metrics

#### QUIC/Iroh Metrics
```rust
p2p_quic_stream_count{peer_id, direction}  // Gauge
p2p_quic_retransmissions_total{peer_id}
p2p_quic_packet_loss_ratio{peer_id}  // Gauge
p2p_iroh_protocol_version{peer_id, version}  // Gauge (1 for active version)
```

#### Resource Synchronization (P2pManager)
```rust
p2p_resource_sync_duration_milliseconds{resource_type}  // Histogram
p2p_resource_transfers_active{resource_type}  // Gauge
p2p_resource_transfer_bytes_total{peer_id, resource_type}
p2p_resource_sync_failures_total{peer_id, resource_type, reason}
```

## Implementation Architecture

### 1. Core Components

#### P2pMonitor Trait
```rust
#[async_trait]
pub trait P2pMonitor: Send + Sync {
    async fn record_connection_attempt(&self, peer_id: &str, success: bool);
    async fn record_connection_state_change(&self, peer_id: &str, from: ConnectionState, to: ConnectionState);
    async fn record_bytes_transferred(&self, peer_id: &str, direction: Direction, bytes: u64);
    async fn record_message(&self, peer_id: &str, message_type: &str, size: usize, latency_ms: Option<f64>);
    async fn record_rtt(&self, peer_id: &str, rtt_ms: f64);
    async fn record_error(&self, peer_id: &str, error_type: P2pErrorType);
}
```

#### Connection State Enum
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Failed,
}
```

### 2. Integration Points

#### IrohPeerConnector Enhancement
- Hook into `connect_to_peer()` for connection attempts
- Monitor circuit breaker state changes
- Track connection pool metrics
- Periodic RTT measurements

#### IrohTransportV2 Enhancement
- Instrument `send_to_peer()` for outbound metrics
- Monitor incoming stream handlers
- Track QUIC-level statistics

#### P2pManager Integration
- Subscribe to P2pEvent stream
- Track resource synchronization metrics
- Monitor discovery events

### 3. Monitoring Features

#### Automatic Health Checks
```rust
pub struct P2pHealthChecker {
    monitor: Arc<dyn P2pMonitor>,
    interval: Duration,
}

impl P2pHealthChecker {
    async fn run_health_check(&self, peer_id: &str) {
        // Measure RTT with ping
        // Check connection quality
        // Update connectivity score
    }
}
```

#### Connection Quality Scoring
```rust
pub struct ConnectionQuality {
    success_rate: f64,      // Recent success percentage
    avg_rtt: f64,          // Moving average RTT
    packet_loss: f64,      // QUIC packet loss rate
    bandwidth: f64,        // Estimated bandwidth
}

impl ConnectionQuality {
    pub fn score(&self) -> f64 {
        // Weighted score 0-1 based on multiple factors
    }
}
```

## Grafana Dashboard Panels

### Connection Overview
1. Active Connections by State (stacked area chart)
2. Connection Success Rate (percentage gauge)
3. Average Connection Duration (histogram)
4. Connection Attempts per Minute (line chart)

### Performance Metrics
1. P2P Latency Heatmap (peer x time)
2. Bandwidth Usage per Peer (stacked bar)
3. Message Volume by Type (pie chart)
4. Top 10 Peers by Traffic (table)

### Error Analysis
1. Connection Errors by Type (time series)
2. Circuit Breaker States (state timeline)
3. Failed Connection Attempts (heatmap)
4. Error Rate by Peer (sorted bar chart)

### Network Topology
1. Peer Connectivity Graph (network visualization)
2. Discovery Success Rate by Method (gauges)
3. Cluster Diameter Over Time (line chart)
4. Peer Churn Rate (additions/removals per hour)

## Prometheus Alert Rules

### Critical Alerts
```yaml
- alert: P2pHighConnectionFailureRate
  expr: rate(p2p_connection_attempts_total{result="failed"}[5m]) > 0.5
  annotations:
    summary: "High P2P connection failure rate"

- alert: P2pPeerDisconnected
  expr: p2p_connections_active{state="connected"} < 2
  for: 5m
  annotations:
    summary: "Cluster has fewer than 2 connected peers"

- alert: P2pCircuitBreakerOpen
  expr: p2p_circuit_breaker_state{state="open"} == 1
  for: 2m
  annotations:
    summary: "Circuit breaker open for peer {{ $labels.peer_id }}"
```

### Warning Alerts
```yaml
- alert: P2pHighLatency
  expr: histogram_quantile(0.95, p2p_rtt_milliseconds) > 100
  annotations:
    summary: "P95 P2P latency exceeds 100ms"

- alert: P2pMessageBufferHigh
  expr: p2p_buffered_messages_total > 1000
  annotations:
    summary: "High message buffer for peer {{ $labels.peer_id }}"
```

## Testing Strategy

### Unit Tests
- Mock P2pMonitor implementation for testing
- Verify metric recording accuracy
- Test connection state transitions

### Integration Tests
- Use MadSim for deterministic testing
- Simulate network conditions (latency, packet loss)
- Verify monitoring under failure scenarios

### Performance Tests
- Benchmark monitoring overhead
- Test metric cardinality limits
- Verify monitoring doesn't impact P2P performance

## Migration Plan

1. **Phase 1**: Implement core P2pMonitor trait and basic metrics
2. **Phase 2**: Integrate with IrohPeerConnector and IrohTransportV2
3. **Phase 3**: Add advanced metrics (RTT, bandwidth, quality scores)
4. **Phase 4**: Implement Grafana dashboards and alerts
5. **Phase 5**: Production rollout with gradual enablement

## Security Considerations

- Avoid exposing sensitive peer information in metrics
- Rate limit metric collection to prevent DoS
- Use secure metric endpoints with authentication
- Sanitize peer IDs in public dashboards