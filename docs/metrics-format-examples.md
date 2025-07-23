# Blixard Metrics Format Examples

## Current Prometheus/OpenMetrics Format

```bash
# HELP blixard_raft_is_leader Whether this node is the current Raft leader
# TYPE blixard_raft_is_leader gauge
blixard_raft_is_leader 1

# HELP blixard_vm_total_count Total number of VMs in the cluster
# TYPE blixard_vm_total_count gauge
blixard_vm_total_count 5

# HELP blixard_grpc_request_duration_seconds Duration of gRPC requests
# TYPE blixard_grpc_request_duration_seconds histogram
blixard_grpc_request_duration_seconds_bucket{method="CreateVm",le="0.005"} 8
blixard_grpc_request_duration_seconds_bucket{method="CreateVm",le="0.01"} 10
blixard_grpc_request_duration_seconds_bucket{method="CreateVm",le="+Inf"} 12
blixard_grpc_request_duration_seconds_sum{method="CreateVm"} 0.0523
blixard_grpc_request_duration_seconds_count{method="CreateVm"} 12
```

## OTLP JSON Format (Future)

```json
{
  "resourceMetrics": [{
    "resource": {
      "attributes": [{
        "key": "service.name",
        "value": { "stringValue": "blixard" }
      }, {
        "key": "node.id",
        "value": { "intValue": "1" }
      }]
    },
    "scopeMetrics": [{
      "scope": {
        "name": "blixard.raft",
        "version": "1.0.0"
      },
      "metrics": [{
        "name": "raft.is_leader",
        "description": "Whether this node is the current Raft leader",
        "unit": "",
        "gauge": {
          "dataPoints": [{
            "timeUnixNano": "1678901234567890123",
            "asInt": "1"
          }]
        }
      }]
    }]
  }]
}
```

## OpenAPI Response Format

```json
{
  "status": "healthy",
  "metrics": {
    "raft": {
      "is_leader": true,
      "term": 42,
      "committed_index": 1234
    },
    "vms": {
      "total": 5,
      "running": 3,
      "stopped": 2
    },
    "resources": {
      "cpu_percent": 45.2,
      "memory_percent": 62.1,
      "disk_percent": 30.5
    }
  },
  "timestamp": "2025-01-25T10:30:45Z"
}
```

## Integration Examples

### Prometheus Scrape Config
```yaml
scrape_configs:
  - job_name: 'blixard'
    scrape_interval: 15s
    static_configs:
      - targets: ['blixard-1:9090', 'blixard-2:9090', 'blixard-3:9090']
```

### OpenTelemetry Collector Config
```yaml
receivers:
  prometheus:
    config:
      scrape_configs:
        - job_name: 'blixard'
          static_configs:
            - targets: ['blixard:9090']
            
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  otlphttp:
    endpoint: https://your-backend.com/v1/metrics
    
service:
  pipelines:
    metrics:
      receivers: [prometheus, otlp]
      exporters: [otlphttp]
```

### Direct API Access
```bash
# Get metrics in different formats
curl http://localhost:9090/metrics              # Prometheus format
curl http://localhost:8080/api/v1/metrics        # REST API (JSON)
curl http://localhost:8080/api/v1/cluster/metrics # Cluster-wide metrics

# With authentication
TOKEN=$(curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"secret"}' | jq -r '.token')

curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics
```