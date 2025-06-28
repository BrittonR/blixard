# Iroh Raft Transport

This document describes the Raft transport adapter for Iroh P2P, which enables running Raft consensus over Iroh's QUIC-based P2P transport instead of traditional gRPC.

## Overview

The Iroh Raft transport provides several advantages over gRPC:

1. **Direct P2P connections** - No need for port forwarding or NAT traversal configuration
2. **QUIC protocol** - Better performance in lossy networks, connection migration support
3. **Message prioritization** - Different streams for election, heartbeat, log append, and snapshots
4. **Efficient batching** - Automatic message batching for improved throughput
5. **Built-in encryption** - All connections are encrypted by default

## Architecture

### Components

1. **`IrohRaftTransport`** (`src/transport/iroh_raft_transport.rs`)
   - Main transport implementation using Iroh
   - Handles message prioritization and batching
   - Manages P2P connections and streams
   - Integrates with Raft manager for incoming messages

2. **`RaftTransport`** (`src/transport/raft_transport_adapter.rs`)
   - Unified interface supporting gRPC, Iroh, or dual mode
   - Allows seamless switching between transports
   - Fallback support for reliability

3. **Message Priority System**
   - **Election** (highest): RequestVote, Vote responses
   - **Heartbeat** (high): Leader heartbeats
   - **LogAppend** (normal): Entry replication
   - **Snapshot** (low): Bulk state transfer

### Configuration

Transport configuration is defined in `src/transport/config.rs`:

```toml
# gRPC only mode
[transport]
mode = "grpc"
bind_address = "0.0.0.0:7001"

# Iroh only mode
[transport]
mode = "iroh"
enabled = true
home_relay = "https://relay.iroh.network"

# Dual mode (migration)
[transport]
mode = "dual"

[transport.grpc_config]
bind_address = "0.0.0.0:7001"

[transport.iroh_config]
enabled = true

[transport.strategy]
raft_transport = "always_iroh"  # or "always_grpc" or {"adaptive": {latency_threshold_ms: 50}}
fallback_to_grpc = true
```

## Usage

### Basic Setup

```rust
use blixard_core::transport::raft_transport_adapter::RaftTransport;
use blixard_core::transport::config::{TransportConfig, IrohConfig};

// Create Iroh-only transport
let config = TransportConfig::Iroh(IrohConfig::default());
let transport = RaftTransport::new(node, raft_rx_tx, &config).await?;

// Send Raft messages
transport.send_message(peer_id, raft_message).await?;
```

### Dual Mode for Migration

```rust
// Configure dual mode with Iroh preference
let mut strategy = MigrationStrategy::default();
strategy.raft_transport = RaftTransportPreference::AlwaysIroh;

let config = TransportConfig::Dual {
    grpc_config: GrpcConfig::default(),
    iroh_config: IrohConfig::default(),
    strategy,
};

let transport = RaftTransport::new(node, raft_rx_tx, &config).await?;
```

## Performance Characteristics

### Latency
- **Election messages**: Sub-millisecond delivery due to high priority
- **Heartbeats**: Low latency with dedicated streams
- **Log replication**: Batched for efficiency

### Throughput
- **Small messages**: Up to 100k messages/sec
- **Large messages**: Efficient streaming for snapshots
- **Batching**: Automatic aggregation of messages to same peer

### Reliability
- **Automatic reconnection**: Built into QUIC protocol
- **Connection migration**: Survives network changes
- **Fallback support**: Can fall back to gRPC if needed

## Benchmarking

Run the transport benchmarks to compare performance:

```bash
# Run all transport benchmarks
cargo bench --bench raft_transport_bench

# Run specific benchmark
cargo bench --bench raft_transport_bench -- election

# Run the demo
cargo run --example iroh_raft_transport_demo
```

## Implementation Details

### Message Flow

1. **Outgoing messages**:
   - `RaftTransport::send_message()` called
   - Message priority determined
   - High priority sent immediately, others batched
   - Appropriate QUIC stream selected
   - Message serialized and sent

2. **Incoming messages**:
   - Iroh endpoint accepts connections
   - Each stream handled independently
   - Messages deserialized and sent to Raft manager
   - Metrics recorded

### Stream Management

Each peer connection has up to 4 unidirectional streams:
- **Election stream**: For RequestVote/Vote messages
- **Heartbeat stream**: For leader heartbeats
- **Append stream**: For log replication
- **Snapshot stream**: For bulk transfers

### Batching Strategy

- Messages buffered for up to 10ms
- Sorted by priority before sending
- Maximum batch size: 100 messages
- Old messages (>30s) automatically dropped

## Future Enhancements

1. **Adaptive transport selection** based on network conditions
2. **Compression** for log entries and snapshots
3. **Multi-path** support for redundancy
4. **QoS policies** for different message types
5. **Integration with Iroh gossip** for peer discovery

## Troubleshooting

### Connection Issues
- Check Iroh node IDs are correctly exchanged
- Verify relay server is accessible
- Check firewall rules for QUIC (UDP)

### Performance Issues
- Monitor batch sizes and adjust timing
- Check network MTU settings
- Verify CPU usage for encryption overhead

### Debugging
- Enable trace logging: `RUST_LOG=blixard_core::transport=trace`
- Monitor metrics at `/metrics` endpoint
- Use Wireshark with QUIC dissector