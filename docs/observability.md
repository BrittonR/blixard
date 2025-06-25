# Blixard Observability Guide

This guide covers the observability features in Blixard, including metrics and distributed tracing using OpenTelemetry.

## Overview

Blixard provides comprehensive observability through:
- **Metrics**: Prometheus-compatible metrics for monitoring system health
- **Distributed Tracing**: OpenTelemetry-based tracing for request flow analysis
- **Structured Logging**: Contextual logging with trace correlation

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

### AWS X-Ray
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
# Run AWS ADOT Collector configured for X-Ray
```

### Google Cloud Operations
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json
# Run OTel Collector with googlecloud exporter
```

### Azure Monitor
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://dc.services.visualstudio.com/v2/track
export OTEL_EXPORTER_OTLP_HEADERS=X-API-Key=your-instrumentation-key
```

### Datadog
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export DD_API_KEY=your-api-key
# Run Datadog Agent with OTLP receiver
```

See `examples/otlp-config.yaml` for more detailed cloud provider configurations.

## Grafana Dashboards

Import the example dashboard from `examples/grafana-dashboard.json` which includes:
- gRPC request rates and latencies
- Raft consensus metrics
- VM lifecycle metrics
- Storage operation metrics
- Cluster health overview

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

- Exemplars linking metrics to traces
- Custom span processors for business logic
- Baggage propagation for request metadata
- Log correlation with trace IDs
- Profiling integration