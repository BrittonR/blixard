# PeerConnector Testing & Implementation Plan

## Overview
The `PeerConnector` is a critical component managing gRPC connections to cluster peers. It handles connection pooling, automatic reconnection, and message buffering. Currently, it has no dedicated tests despite being essential for cluster communication.

## Critical Test Areas for PeerConnector

### 1. Connection Lifecycle Testing
- **Happy path**: Connect → Send messages → Disconnect
- **Connection failures**: Invalid addresses, unreachable hosts, network timeouts
- **Duplicate connection attempts**: Multiple concurrent calls to `connect_to_peer` for same peer
- **Connection state tracking**: Verify `is_connected` status updates correctly
- **Clean disconnect**: Ensure resources are freed when disconnecting

### 2. Message Buffering Edge Cases
- **Buffer overflow**: Test the 100-message limit per peer
- **Message ordering**: Ensure FIFO order is maintained
- **Buffer cleanup**: Messages cleared after successful connection
- **Memory pressure**: Large messages or many peers with full buffers
- **Orphaned buffers**: Messages for peers that never connect

### 3. Concurrent Access Scenarios
- **Race conditions**: Multiple threads sending to same peer simultaneously
- **Connection state races**: Connect/disconnect happening concurrently
- **Buffer access**: Concurrent buffering and sending
- **Connection reuse**: Multiple senders using same connection

### 4. Reconnection Behavior
- **Automatic reconnection**: Failed sends trigger reconnection
- **Reconnection backoff**: Currently missing - should add exponential backoff
- **Connection flapping**: Rapid connect/disconnect cycles
- **Stale connection detection**: Currently missing - connections might go stale

### 5. Error Handling & Resilience
- **Partial failures**: Some peers reachable, others not
- **Network partitions**: Simulate split-brain scenarios
- **Resource exhaustion**: Too many connections, file descriptor limits
- **Malformed messages**: Invalid Raft message serialization

## Recommended Implementation Additions

### 1. Connection Health Checking
```rust
// Add periodic health checks to detect stale connections
async fn check_connection_health(&self, peer_id: u64) -> bool {
    // Send a lightweight ping/health check
    // Mark connection as unhealthy if it fails
}
```

### 2. Exponential Backoff for Reconnections
```rust
struct ConnectionState {
    last_attempt: Instant,
    attempt_count: u32,
    backoff_ms: u64,
}

// Implement exponential backoff to avoid overwhelming failed peers
fn should_attempt_reconnection(&self, peer_id: u64) -> bool {
    // Check if enough time has passed based on backoff
}
```

### 3. Connection Pool Limits
```rust
const MAX_CONNECTIONS: usize = 100;
// Prevent resource exhaustion by limiting total connections
```

### 4. Message Priority & Expiration
```rust
struct BufferedMessage {
    to: u64,
    message: raft::prelude::Message,
    attempts: u32,
    priority: MessagePriority,  // Add priority for critical messages
    expires_at: Instant,        // Add TTL to prevent stale messages
}
```

### 5. Metrics & Observability
- Connection success/failure rates per peer
- Message buffer depths
- Reconnection attempt counts
- Message send latencies

### 6. Circuit Breaker Pattern
```rust
// After N consecutive failures, stop attempting connections
// for a cooldown period to prevent resource waste
struct CircuitBreaker {
    failure_count: u32,
    last_failure: Instant,
    state: CircuitState,
}
```

## Test Implementation Strategy

### Unit Tests
- Mock the `SharedNodeState` and `ClusterServiceClient`
- Test each method in isolation
- Use `tokio::test` for async testing
- Inject controllable failures

### Integration Tests
- Create real gRPC endpoints for testing
- Simulate network conditions (delays, failures)
- Test multi-peer scenarios
- Verify message delivery guarantees

### Property-Based Tests
- Connection state invariants (can't be both connecting and connected)
- Buffer size limits are respected
- Message ordering is preserved
- No resource leaks under random operations

### Stress Tests
- Many concurrent connections
- High message throughput
- Rapid connect/disconnect cycles
- Memory usage under load

## Implementation Priority

### Phase 1: Core Functionality Tests (High Priority)
1. Basic connection lifecycle tests
2. Message send/receive tests
3. Connection state tracking tests
4. Basic error handling tests

### Phase 2: Resilience Features (Medium Priority)
1. Implement exponential backoff
2. Add connection health checking
3. Implement circuit breaker pattern
4. Add connection pool limits

### Phase 3: Advanced Features (Lower Priority)
1. Message priority system
2. Message expiration/TTL
3. Advanced metrics collection
4. Performance optimizations

## Success Criteria

- **Test Coverage**: >90% line coverage for PeerConnector
- **Reliability**: No connection leaks under stress testing
- **Performance**: <10ms overhead for message sending
- **Resilience**: Graceful handling of all failure scenarios
- **Observability**: Clear metrics for debugging production issues

## Next Steps

1. **Create test infrastructure** (1 day)
   - Set up mock gRPC server for testing
   - Create test utilities for simulating network conditions
   - Add test fixtures for common scenarios

2. **Implement core tests** (2-3 days)
   - Connection lifecycle tests
   - Message buffering tests
   - Concurrent access tests
   - Error handling tests

3. **Add resilience features** (2-3 days)
   - Exponential backoff implementation
   - Connection health checking
   - Circuit breaker pattern
   - Resource limit enforcement

4. **Performance testing** (1 day)
   - Load testing framework
   - Benchmark message throughput
   - Memory usage profiling
   - Connection pool optimization