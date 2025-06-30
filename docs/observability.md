# Blixard Observability Guide

This guide covers the observability features in Blixard, including metrics and distributed tracing using OpenTelemetry.

## Overview

Blixard provides comprehensive observability through:
- **Metrics**: Prometheus-compatible metrics for monitoring system health
- **Distributed Tracing**: OpenTelemetry-based tracing for request flow analysis  
- **Structured Logging**: Contextual logging with trace correlation
- **Cloud Export**: Native support for AWS, GCP, Azure, and Datadog
- **Exemplars**: Trace-to-metrics correlation for debugging
- **Dashboards**: Pre-built Grafana dashboards for visualization
- **Alerting**: Production-ready Prometheus alert rules
- **Runbooks**: Operational procedures for incident response

## Metrics

### Available Metrics

Blixard exposes the following metrics:

#### Raft Metrics
- `raft_proposals_total`: Total number of Raft proposals
- `raft_proposals_failed`: Failed Raft proposals
- `raft_committed_entries_total`: Total committed entries
- `raft_applied_entries_total`: Total applied entries
- `raft_leader_changes_total`: Number of leader changes
- `raft_term`: Current Raft term
- `raft_commit_index`: Current commit index
- `raft_proposal_duration`: Histogram of proposal durations

#### Network Metrics
- `grpc_requests_total`: Total gRPC requests by method
- `grpc_requests_failed`: Failed gRPC requests
- `grpc_request_duration`: Request duration histogram
- `peer_connections_active`: Active peer connections
- `peer_reconnect_attempts`: Peer reconnection attempts

#### VM Metrics
- `vm_total`: Total number of VMs
- `vm_running`: Number of running VMs
- `vm_create_total`: VM creation attempts
- `vm_create_failed`: Failed VM creations
- `vm_create_duration`: VM creation duration histogram

#### Storage Metrics
- `storage_reads_total`: Total storage read operations
- `storage_writes_total`: Total storage write operations
- `storage_read_duration`: Read operation duration
- `storage_write_duration`: Write operation duration

### Accessing Metrics

Metrics are exposed on the HTTP port (gRPC port + 1000):

```bash
# If gRPC is on port 7001, metrics are on 8001
curl http://localhost:8001/metrics
```

The metrics endpoint also provides:
- `/health` - Health check endpoint
- `/` - Simple HTML page with links

## Distributed Tracing

### Configuration

Tracing can be configured via environment variables:

```bash
# Enable OTLP export
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# Optional: Set specific endpoints
export OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=http://localhost:4317
export OTEL_EXPORTER_OTLP_METRICS_ENDPOINT=http://localhost:4317

# Authentication (if required)
export OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer your-token

# Start Blixard
cargo run -- node --id 1 --bind 127.0.0.1:7001
```

### Trace Context Propagation

Blixard automatically propagates trace context through:
- gRPC calls between nodes
- Storage operations
- VM operations
- Raft consensus operations

### Instrumented Components

The following components are instrumented with spans:
- **gRPC Server**: All incoming RPC calls
- **gRPC Client**: Outgoing calls with context injection
- **Storage**: Database read/write operations
- **VM Manager**: VM lifecycle operations
- **Raft Manager**: Consensus operations

## Local Development Setup

### Using Docker Compose

Create a `docker-compose.yml` with the observability stack:

```yaml
version: '3.8'
services:
  otel-collector:
    image: otel/opentelemetry-collector-contrib:latest
    command: ["--config=/etc/otel-collector-config.yaml"]
    volumes:
      - ./otel-collector-config.yaml:/etc/otel-collector-config.yaml
    ports:
      - "4317:4317"   # OTLP gRPC
      - "4318:4318"   # OTLP HTTP
      - "8889:8889"   # Prometheus metrics

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686" # Jaeger UI

  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9090:9090"

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
```

### OpenTelemetry Collector Configuration

Create `otel-collector-config.yaml`:

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:

exporters:
  jaeger:
    endpoint: jaeger:14250
    tls:
      insecure: true
  
  prometheus:
    endpoint: "0.0.0.0:8889"

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [jaeger]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [prometheus]
```

### Running a Traced Cluster

1. Start the observability stack:
   ```bash
   docker-compose up -d
   ```

2. Start Blixard nodes with tracing:
   ```bash
   export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
   
   # Node 1
   cargo run -- node --id 1 --bind 127.0.0.1:7001 &
   
   # Node 2
   cargo run -- node --id 2 --bind 127.0.0.1:7002 --join 127.0.0.1:7001 &
   
   # Node 3
   cargo run -- node --id 3 --bind 127.0.0.1:7003 --join 127.0.0.1:7001 &
   ```

3. Access the UIs:
   - Grafana: http://localhost:3000 (admin/admin)
   - Jaeger: http://localhost:16686
   - Prometheus: http://localhost:9090

## Cloud Provider Integration

Blixard now includes native cloud provider support. Simply set the provider and credentials:

### AWS X-Ray
```bash
export BLIXARD_CLOUD_PROVIDER=aws
export AWS_REGION=us-west-2
# Uses IAM role or AWS credentials automatically
```

### Google Cloud Operations
```bash
export BLIXARD_CLOUD_PROVIDER=gcp
export GCP_PROJECT=your-project-id
# Uses Application Default Credentials
```

### Azure Monitor
```bash
export BLIXARD_CLOUD_PROVIDER=azure
export AZURE_INSTRUMENTATION_KEY=your-key
```

### Datadog
```bash
export BLIXARD_CLOUD_PROVIDER=datadog
export DD_API_KEY=your-api-key
export DD_SITE=datadoghq.com  # or datadoghq.eu
```

See the [Cloud Observability Guide](./cloud-observability.md) for detailed configuration.

## Grafana Dashboards

We provide comprehensive Grafana dashboards:

### Quick Start
Import `/monitoring/grafana/dashboards/blixard-comprehensive.json` for:
- **Cluster Overview**: Node health, resource usage, Raft state
- **Raft Consensus**: Proposals, commits, leader changes
- **Network & gRPC**: Request rates, latencies, peer connections  
- **VM Management**: Operations, health checks, recovery
- **VM Placement**: Scheduling success, resource allocation
- **Storage Performance**: Read/write operations and latencies
- **P2P Distribution**: Transfer rates, cache efficiency, verification

### Dashboard Features
- Auto-refresh every 10 seconds
- Prometheus datasource selector
- Time range controls
- Drill-down capabilities

## Prometheus Alerting

Production-ready alert rules are provided in `/monitoring/prometheus/alerts/blixard-alerts.yml`:

### Alert Categories
- **Cluster Health**: Node availability, health status
- **Raft Consensus**: Leader election, proposal failures
- **Resource Usage**: CPU, memory, disk thresholds  
- **VM Operations**: Creation/start failures, health checks
- **Network/gRPC**: Error rates, latency, connectivity
- **Storage**: Operation latency, throughput
- **P2P Transfers**: Verification failures, transfer errors

### Setting Up Alerts
```yaml
# prometheus.yml
alerting:
  alertmanagers:
    - static_configs:
        - targets: ['localhost:9093']

rule_files:
  - '/path/to/blixard-alerts.yml'
```

## Operational Runbooks

Each alert has a corresponding runbook in `/docs/runbooks/`:
- Step-by-step diagnostic procedures
- Common resolution scenarios
- Recovery verification steps
- Prevention recommendations

See the [Runbook Index](./runbooks/README.md) for the complete list.

## Exemplar Support

Blixard supports exemplars for trace-to-metrics correlation:

### How It Works
- Metrics recorded within trace context automatically include trace IDs
- Prometheus stores these as exemplars with metric samples
- Grafana can jump from metric spikes to distributed traces

### Using Exemplars
```rust
// Automatic exemplar recording
use blixard_core::observability::exemplars::integrations::*;

// Record metric with trace context
record_grpc_request_with_trace("CreateVm", duration, success);
```

### Viewing Exemplars
1. In Grafana, enable exemplars in datasource settings
2. Look for dots on graph lines indicating exemplars
3. Click to see trace ID and jump to trace view

## Troubleshooting

### No Traces Appearing
1. Check OTEL_EXPORTER_OTLP_ENDPOINT is set correctly
2. Verify the collector is running and accessible
3. Check for errors in Blixard logs about OTLP export

### Missing Metrics
1. Ensure metrics server is running (check logs)
2. Verify firewall allows access to metrics port
3. Check Prometheus scrape configuration

### Performance Impact
- Tracing with sampling can be configured via OTEL SDK
- Metrics have minimal overhead
- In production, consider using head sampling or tail sampling

## Best Practices

1. **Use trace context**: When making gRPC calls, trace context is automatically propagated
2. **Add span attributes**: Use `tracing_otel::add_attributes()` for important context
3. **Record errors**: Errors are automatically recorded in spans
4. **Monitor metrics**: Set up alerts on key metrics like error rates
5. **Sample appropriately**: In production, use sampling to reduce overhead

## Future Enhancements

- ✅ ~~Exemplars linking metrics to traces~~ (Implemented)
- ✅ ~~Cloud provider integrations~~ (AWS, GCP, Azure, Datadog supported)  
- ✅ ~~Production dashboards~~ (Comprehensive Grafana dashboard)
- ✅ ~~Alert rules~~ (Complete Prometheus alerting)
- ✅ ~~Operational runbooks~~ (Alert response procedures)
- Custom span processors for business logic
- Baggage propagation for request metadata
- Log correlation with trace IDs
- Continuous profiling integration
- Service dependency mapping
- Automated anomaly detection