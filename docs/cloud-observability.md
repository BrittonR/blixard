# Cloud Observability Configuration

This guide explains how to configure Blixard to export metrics and traces to major cloud providers.

## Overview

Blixard supports automatic OTLP (OpenTelemetry Protocol) export to:
- AWS X-Ray and CloudWatch
- Google Cloud Operations (formerly Stackdriver)
- Azure Monitor
- Datadog

## Configuration

### Environment Variables

Set the following environment variables to enable cloud export:

```bash
# Enable cloud provider export
export BLIXARD_CLOUD_PROVIDER=aws|gcp|azure|datadog

# Provider-specific configuration (see sections below)
```

### AWS X-Ray/CloudWatch

```bash
export BLIXARD_CLOUD_PROVIDER=aws
export AWS_REGION=us-west-2  # or AWS_DEFAULT_REGION

# AWS credentials via IAM role (recommended) or:
export AWS_ACCESS_KEY_ID=your-key-id
export AWS_SECRET_ACCESS_KEY=your-secret-key
```

**IAM Permissions Required:**
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "xray:PutTraceSegments",
        "xray:PutTelemetryRecords",
        "cloudwatch:PutMetricData"
      ],
      "Resource": "*"
    }
  ]
}
```

### Google Cloud Operations

```bash
export BLIXARD_CLOUD_PROVIDER=gcp
export GCP_PROJECT=your-project-id  # or GOOGLE_CLOUD_PROJECT

# Authentication via Application Default Credentials:
# - GKE Workload Identity (recommended)
# - Service Account JSON file:
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json
```

**Required APIs:**
- Cloud Trace API
- Cloud Monitoring API

**IAM Permissions:**
- `roles/cloudtrace.agent`
- `roles/monitoring.metricWriter`

### Azure Monitor

```bash
export BLIXARD_CLOUD_PROVIDER=azure
export AZURE_INSTRUMENTATION_KEY=your-instrumentation-key

# Get instrumentation key from Azure Portal:
# Application Insights > Overview > Instrumentation Key
```

### Datadog

```bash
export BLIXARD_CLOUD_PROVIDER=datadog
export DD_API_KEY=your-api-key
export DD_SITE=datadoghq.com  # or datadoghq.eu for EU

# Optional: Additional tags
export DD_TAGS=env:production,team:platform
```

## Kubernetes Deployment

### AWS EKS Example

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: blixard
  annotations:
    eks.amazonaws.com/role-arn: arn:aws:iam::123456789012:role/blixard-observability
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: blixard
spec:
  template:
    spec:
      serviceAccountName: blixard
      containers:
      - name: blixard
        env:
        - name: BLIXARD_CLOUD_PROVIDER
          value: "aws"
        - name: AWS_REGION
          value: "us-west-2"
```

### GKE Example

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: blixard
  annotations:
    iam.gke.io/gcp-service-account: blixard-observability@project.iam.gserviceaccount.com
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: blixard
spec:
  template:
    spec:
      serviceAccountName: blixard
      containers:
      - name: blixard
        env:
        - name: BLIXARD_CLOUD_PROVIDER
          value: "gcp"
        - name: GCP_PROJECT
          value: "your-project-id"
```

### AKS Example

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: blixard
spec:
  template:
    spec:
      containers:
      - name: blixard
        env:
        - name: BLIXARD_CLOUD_PROVIDER
          value: "azure"
        - name: AZURE_INSTRUMENTATION_KEY
          valueFrom:
            secretKeyRef:
              name: blixard-observability
              key: instrumentation-key
```

## Custom OTLP Endpoints

For other OTLP-compatible backends or on-premise installations:

```bash
# Use standard OpenTelemetry environment variables
export OTEL_EXPORTER_OTLP_ENDPOINT=https://your-otlp-endpoint:4317
export OTEL_EXPORTER_OTLP_HEADERS="Authorization=Bearer your-token"

# Or configure specific endpoints
export OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=https://traces.example.com:4317
export OTEL_EXPORTER_OTLP_METRICS_ENDPOINT=https://metrics.example.com:4317
```

## Batch Export Configuration

Fine-tune export behavior via environment variables:

```bash
# Maximum queue size (default: 2048)
export OTEL_BSP_MAX_QUEUE_SIZE=4096

# Maximum export batch size (default: 512)
export OTEL_BSP_MAX_EXPORT_BATCH_SIZE=1024

# Export interval in milliseconds (default: 5000)
export OTEL_BSP_SCHEDULE_DELAY=10000

# Maximum concurrent exports (default: 1)
export OTEL_BSP_MAX_CONCURRENT_EXPORTS=2
```

## Sampling Configuration

Control trace sampling to manage costs:

```bash
# Always sample (default for development)
export OTEL_TRACES_SAMPLER=always_on

# Sample ratio (0.0 to 1.0)
export OTEL_TRACES_SAMPLER=traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1  # Sample 10% of traces

# Parent-based sampling (recommended for production)
export OTEL_TRACES_SAMPLER=parentbased_traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.05  # Sample 5% of root spans
```

## Monitoring Costs

### AWS
- X-Ray: $5.00 per million traces recorded
- CloudWatch: $0.30 per million custom metrics

### GCP
- Cloud Trace: $0.20 per million spans
- Cloud Monitoring: $0.258 per million metrics

### Azure
- Application Insights: Based on data volume (GB)
- First 5GB per month free

### Datadog
- APM: Based on indexed spans and hosts
- Custom metrics: Based on unique time series

## Troubleshooting

### Enable Debug Logging

```bash
export OTEL_LOG_LEVEL=debug
export RUST_LOG=blixard=debug,opentelemetry=debug
```

### Verify Export

Check metrics endpoint:
```bash
curl http://localhost:9090/metrics | grep -E "otel_|traces_|spans_"
```

### Common Issues

1. **No data in cloud provider**
   - Verify authentication/credentials
   - Check network connectivity to endpoints
   - Ensure required APIs are enabled
   - Review IAM permissions

2. **High memory usage**
   - Reduce batch size and queue size
   - Increase export frequency
   - Enable sampling

3. **Export failures**
   - Check logs for specific errors
   - Verify endpoint URLs
   - Test with `otel-cli` or similar tools

## Best Practices

1. **Use IAM roles** instead of API keys when possible
2. **Enable sampling** in production to control costs
3. **Set resource limits** for the metrics/tracing pipeline
4. **Monitor the monitors** - track export success rates
5. **Use structured logging** with trace IDs for correlation
6. **Configure alerts** for export failures

## Example Dashboards

Import these dashboard templates for quick starts:

- **Grafana Cloud**: ID 15892 (Blixard Comprehensive)
- **AWS CloudWatch**: Available in AWS Observability Accelerator
- **GCP Operations**: Use "Trace" and "Metrics Explorer"
- **Azure Monitor**: Application Insights workbooks
- **Datadog**: APM Service Map and Dashboards