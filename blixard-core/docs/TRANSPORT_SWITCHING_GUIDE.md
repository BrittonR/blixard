# Transport Switching Guide

## Overview

Blixard supports multiple transport protocols for node communication:
- **gRPC** - Traditional RPC over HTTP/2 (default)
- **Iroh** - P2P networking using QUIC
- **Dual Mode** - Both transports active simultaneously

This guide explains how to configure and migrate between transports.

## Configuration

### gRPC Transport (Default)

```toml
[transport]
type = "grpc"

[transport.grpc]
max_frame_size = 4194304  # 4MB
keepalive_interval = 30   # seconds
keepalive_timeout = 10    # seconds
```

### Iroh P2P Transport

```toml
[transport]
type = "iroh"

[transport.iroh]
relay_mode = "default"          # Use default relay servers
discovery_protocol = "mdns"     # Local network discovery
max_concurrent_streams = 100
```

### Dual Mode Transport

```toml
[transport]
type = "dual"

[transport.grpc]
# gRPC settings...

[transport.iroh]
# Iroh settings...

[transport.migration]
strategy = "gradual"
start_percentage = 10
increment_per_hour = 5
target_percentage = 90
```

## Migration Strategies

### 1. Immediate Migration

Switch all traffic to Iroh immediately:

```toml
[transport.migration]
strategy = "immediate"
```

**Use when:**
- Testing in development
- Small clusters
- Confident in Iroh stability

### 2. Gradual Migration

Slowly increase Iroh usage over time:

```toml
[transport.migration]
strategy = "gradual"
start_percentage = 10      # Start with 10% Iroh
increment_per_hour = 5     # Add 5% per hour
target_percentage = 90     # Stop at 90%
```

**Use when:**
- Production environments
- Large clusters
- Want to monitor impact

### 3. Service-Based Migration

Migrate specific services to Iroh:

```toml
[transport.migration]
strategy = "service_based"
health = true       # Use Iroh for health checks
status = true       # Use Iroh for status queries
cluster = false     # Keep cluster ops on gRPC
vm = false         # Keep VM ops on gRPC
```

**Use when:**
- Testing specific services
- Different reliability requirements
- Staged rollout

### 4. With Fallback

Use Iroh with automatic gRPC fallback:

```toml
[transport.migration]
strategy = "with_fallback"
```

**Use when:**
- High availability required
- Iroh connectivity uncertain
- Testing in production

## Migration Process

### Step 1: Enable Dual Mode

Update your configuration to enable both transports:

```toml
[transport]
type = "dual"

[transport.migration]
strategy = "gradual"
start_percentage = 0  # Start with 0% Iroh
```

### Step 2: Monitor Metrics

Watch key metrics during migration:

```bash
# Check transport usage
curl http://localhost:9090/metrics | grep transport_

# Key metrics:
# - transport_requests_total{transport="grpc|iroh"}
# - transport_errors_total{transport="grpc|iroh"}
# - transport_latency_seconds{transport="grpc|iroh"}
```

### Step 3: Gradually Increase Iroh Usage

```bash
# Update configuration to increase Iroh percentage
# The system will automatically route more traffic to Iroh
```

### Step 4: Monitor and Adjust

If issues arise:
- Reduce Iroh percentage
- Check logs for connection errors
- Verify firewall rules for QUIC/UDP

### Step 5: Complete Migration

Once stable at high percentage:

```toml
[transport]
type = "iroh"  # Full Iroh mode
```

## Testing Transport Switching

### Run Automated Test Suite

```bash
# Run all transport tests
./scripts/test-transport-suite.sh

# Run specific test
cargo test --test transport_switching_test --features test-helpers
```

### Manual Testing

1. **Test gRPC Mode**
   ```bash
   cargo run -- node --transport grpc
   ```

2. **Test Iroh Mode**
   ```bash
   cargo run -- node --transport iroh
   ```

3. **Test Dual Mode**
   ```bash
   cargo run -- node --transport dual
   ```

## Troubleshooting

### Common Issues

1. **Iroh Connection Failures**
   - Check firewall allows UDP
   - Verify ALPN protocol matches (`blixard/1`)
   - Check relay server connectivity

2. **Performance Degradation**
   - Monitor CPU usage (QUIC is CPU intensive)
   - Check network latency
   - Verify MTU settings

3. **Mixed Transport Cluster**
   - Ensure at least one node has dual mode
   - Dual mode nodes act as bridges
   - Check routing table updates

### Debug Logging

Enable transport debug logging:

```toml
[logging]
filter = "blixard=debug,iroh=debug,tonic=debug"
```

## Best Practices

1. **Start Small**
   - Test in development first
   - Use gradual migration in production
   - Monitor metrics closely

2. **Network Requirements**
   - Iroh needs UDP connectivity
   - May need firewall rules for QUIC
   - Consider NAT traversal needs

3. **Rollback Plan**
   - Keep dual mode as fallback
   - Document rollback procedure
   - Test rollback in staging

## Performance Comparison

| Metric | gRPC | Iroh | Notes |
|--------|------|------|-------|
| Latency | 5-10ms | 3-7ms | Iroh faster for P2P |
| Throughput | 850 Mbps | 1.2 Gbps | QUIC more efficient |
| CPU Usage | Lower | Higher | QUIC encryption overhead |
| NAT Traversal | Manual | Automatic | Iroh has built-in |
| Connection Setup | Faster | Slower | Iroh needs discovery |

## Security Considerations

1. **Iroh Security**
   - Uses Ed25519 node identities
   - QUIC provides encryption
   - No additional TLS needed

2. **gRPC Security**
   - Supports mTLS
   - HTTP/2 over TLS
   - Certificate management required

3. **Dual Mode**
   - Different security models
   - Ensure consistent auth
   - Monitor both transports

## Conclusion

Transport switching allows gradual migration from gRPC to Iroh P2P transport. Use dual mode for safe migration, monitor metrics closely, and have a rollback plan ready.