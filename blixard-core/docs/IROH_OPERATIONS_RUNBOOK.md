# Iroh Transport Operations Runbook

## Overview

This runbook provides operational procedures for managing Blixard nodes using the Iroh P2P transport.

## Table of Contents

1. [Health Checks](#health-checks)
2. [Troubleshooting Connection Issues](#troubleshooting-connection-issues)
3. [NAT Traversal Problems](#nat-traversal-problems)
4. [Performance Degradation](#performance-degradation)
5. [Rollback Procedures](#rollback-procedures)
6. [Monitoring and Alerts](#monitoring-and-alerts)

## Health Checks

### Quick Health Check

```bash
# Check if Iroh endpoint is listening
$ ss -tuln | grep :7001
tcp   LISTEN 0      128    0.0.0.0:7001

# Check node status via CLI
$ blixard-cli status --node 127.0.0.1:7001

# Check Iroh-specific metrics
$ curl -s localhost:9090/metrics | grep iroh_
blixard_iroh_connections_active 42
blixard_iroh_messages_total 1234567
```

### Detailed Health Assessment

```bash
# 1. Check Iroh endpoint status
$ blixard-cli node debug --iroh-status

# 2. Verify NAT traversal capability
$ blixard-cli node test-nat

# 3. Check peer connectivity
$ blixard-cli cluster peers --detailed
```

## Troubleshooting Connection Issues

### Symptom: "No addressing information available"

**Cause**: Nodes cannot discover each other's Iroh addresses.

**Resolution**:
1. Verify node addresses are being shared:
   ```bash
   $ blixard-cli cluster peers
   ```

2. Check if relay server is accessible:
   ```bash
   $ curl -I https://relay.iroh.network/
   ```

3. Manually add peer address:
   ```bash
   $ blixard-cli cluster add-peer \
     --node-id 2 \
     --iroh-addr "node_id:relay_url"
   ```

### Symptom: High connection establishment time

**Cause**: Direct connections failing, falling back to relay.

**Resolution**:
1. Check firewall rules:
   ```bash
   # Allow UDP for QUIC
   $ sudo iptables -A INPUT -p udp --dport 7001:7100 -j ACCEPT
   ```

2. Enable STUN for better NAT traversal:
   ```toml
   [transport.iroh]
   stun_servers = ["stun.l.google.com:19302"]
   ```

3. Deploy local relay server for better performance.

## NAT Traversal Problems

### Diagnosing NAT Type

```bash
# Run NAT type detection
$ blixard-cli debug nat-type

# Expected output:
# NAT Type: Full Cone (best)
# NAT Type: Restricted Cone (good)
# NAT Type: Port Restricted (okay)
# NAT Type: Symmetric (problematic)
```

### Solutions by NAT Type

**Symmetric NAT**:
- Deploy relay servers in same network
- Use static port mappings
- Consider VPN for node communication

**Port Restricted**:
- Ensure consistent source ports
- Enable UDP hole punching optimization

## Performance Degradation

### High Message Latency

1. **Check metrics**:
   ```bash
   # Query Prometheus
   $ promtool query instant \
     'histogram_quantile(0.99, rate(blixard_iroh_message_duration_seconds_bucket[5m]))'
   ```

2. **Identify bottlenecks**:
   - Network congestion
   - CPU saturation
   - Too many connections

3. **Remediation**:
   ```toml
   # Tune connection limits
   [transport.iroh]
   max_connections = 100
   max_streams_per_connection = 50
   
   # Enable connection pooling
   connection_pool_size = 10
   ```

### Message Throughput Issues

1. **Enable batching**:
   ```toml
   [transport.iroh]
   batch_window_ms = 10
   max_batch_size = 100
   ```

2. **Optimize buffer sizes**:
   ```toml
   [transport.iroh]
   send_buffer_size = 2097152  # 2MB
   recv_buffer_size = 2097152  # 2MB
   ```

## Rollback Procedures

### Emergency Rollback to gRPC

1. **Update configuration** (no restart required):
   ```toml
   [transport]
   mode = "grpc"  # was "iroh" or "dual"
   ```

2. **Verify rollback**:
   ```bash
   $ blixard-cli status --transport
   Current transport: gRPC
   ```

3. **Monitor for issues**:
   ```bash
   $ watch -n 5 'curl -s localhost:9090/metrics | grep -E "(error|latency)"'
   ```

### Gradual Rollback

For canary deployments:
```bash
# Rollback specific nodes
$ for node in node1 node2 node3; do
    blixard-cli config set \
      --node $node \
      --key transport.mode \
      --value grpc
done
```

## Monitoring and Alerts

### Key Metrics to Watch

| Metric | Threshold | Action |
|--------|-----------|---------|
| `iroh_connection_error_rate` | > 5% | Check network, firewall |
| `iroh_message_latency_p99` | > 10ms | Investigate congestion |
| `iroh_nat_traversal_success` | < 95% | Check relay availability |
| `raft_election_stability` | < 99% | Review timeout settings |

### Alert Response Procedures

**Alert: IrohHighConnectionFailureRate**
1. Check recent deployments
2. Verify relay server status
3. Review firewall changes
4. Check for network issues

**Alert: IrohNATTraversalFailures**
1. Test relay connectivity
2. Check STUN server availability
3. Review NAT configuration
4. Consider deploying local relay

### Useful Commands

```bash
# Show Iroh connection stats
$ blixard-cli debug iroh-stats

# Test connectivity to specific peer
$ blixard-cli debug ping-peer --node-id 2 --transport iroh

# Dump Iroh routing table
$ blixard-cli debug iroh-routes

# Force reconnect to peer
$ blixard-cli cluster reconnect --node-id 2
```

## Common Issues and Solutions

### Issue: Nodes behind corporate firewall can't connect

**Solution**:
```toml
# Use HTTPS relay fallback
[transport.iroh]
relay_mode = "https"
relay_servers = ["https://internal-relay.company.com"]
```

### Issue: High CPU usage after Iroh deployment

**Solution**:
1. Enable CPU profiling:
   ```bash
   $ blixard-cli debug profile --cpu --duration 30s
   ```

2. Tune crypto operations:
   ```toml
   [transport.iroh]
   crypto_threads = 2  # Limit crypto thread pool
   ```

### Issue: Memory growth over time

**Solution**:
1. Check connection leaks:
   ```bash
   $ blixard-cli debug connections --detailed
   ```

2. Enable connection limits:
   ```toml
   [transport.iroh]
   max_idle_timeout = "5m"
   connection_gc_interval = "1m"
   ```

## Best Practices

1. **Always test in staging first**
2. **Monitor key metrics during rollout**
3. **Keep dual-mode as fallback option**
4. **Document any custom relay deployments**
5. **Regular firewall rule audits**
6. **Maintain runbook with lessons learned**

## Emergency Contacts

- On-call Engineer: [pager]
- Network Team: [network-oncall]
- Platform Team: [platform-oncall]

## Appendix: Configuration Reference

```toml
[transport]
mode = "iroh"  # "grpc", "iroh", or "dual"

[transport.iroh]
# Connection settings
max_connections = 100
max_streams_per_connection = 50
connection_pool_size = 10

# Timeouts
connect_timeout = "5s"
request_timeout = "30s"
max_idle_timeout = "5m"

# NAT traversal
stun_servers = ["stun.l.google.com:19302"]
relay_mode = "auto"  # "auto", "always", "never"
relay_servers = ["https://relay.iroh.network"]

# Performance tuning
batch_window_ms = 10
max_batch_size = 100
send_buffer_size = 2097152
recv_buffer_size = 2097152

# Security
require_node_id_verification = true
allowed_node_ids = []  # Empty means allow all

# Debugging
enable_packet_capture = false
verbose_logging = false
```