# OpenTelemetry Integration Guide

## Current Status

Blixard has comprehensive OpenTelemetry support for both metrics and tracing.

### Quick Setup

1. **Enable in configuration**:
```toml
[observability.metrics]
enabled = true
runtime_metrics = true

[observability.metrics.http_server]
enabled = true
bind = "0.0.0.0:9090"

[observability.tracing]
enabled = true
otlp_endpoint = "http://localhost:4317"  # Your OTLP collector
service_name = "blixard"
sampling_ratio = 1.0
```

2. **Access metrics**:
```bash
# Prometheus format (OpenMetrics compatible)
curl http://localhost:9090/metrics

# Send to OpenTelemetry Collector
# Metrics are automatically exported if OTLP endpoint is configured
```

3. **View traces**:
- Traces are sent to your OTLP endpoint
- Compatible with Jaeger, Tempo, etc.

## OpenTelemetry Collector Configuration

Example collector config to receive Blixard telemetry:

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

  prometheus:
    config:
      scrape_configs:
        - job_name: 'blixard'
          static_configs:
            - targets: ['blixard-node:9090']

exporters:
  prometheus:
    endpoint: "0.0.0.0:8889"
  
  jaeger:
    endpoint: jaeger:14250
    
  # For metrics
  prometheusremotewrite:
    endpoint: "http://prometheus:9090/api/v1/write"

processors:
  batch:

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [jaeger]
    
    metrics:
      receivers: [otlp, prometheus]
      processors: [batch]
      exporters: [prometheus, prometheusremotewrite]
```

## Metrics Available

All metrics follow the format: `blixard_<subsystem>_<metric>`

### Key Metrics:
- `blixard_raft_*` - Consensus metrics
- `blixard_vm_*` - VM lifecycle metrics
- `blixard_grpc_*` - RPC metrics
- `blixard_peer_*` - P2P metrics
- `blixard_node_*` - Resource utilization

## Integration with Monitoring Stacks

### Grafana Cloud
```toml
[observability.tracing]
otlp_endpoint = "https://otlp-gateway-prod-us-central-0.grafana.net:443"
# Add auth headers as needed
```

### Datadog
```toml
[observability.tracing]
otlp_endpoint = "http://datadog-agent:4317"
```

### AWS X-Ray
Use OpenTelemetry Collector with X-Ray exporter

## Known Limitations

1. **OpenTelemetry Version**: Currently on v0.20 (planning upgrade to v0.27+)
2. **Semantic Conventions**: Partial compliance with OTel semantic conventions
3. **Metrics Format**: Exposed as Prometheus, not native OTLP

## Future Improvements

- [ ] Upgrade to OpenTelemetry 0.27+
- [ ] Add OTLP/HTTP endpoint for metrics
- [ ] Full semantic convention compliance
- [ ] Baggage propagation support