# Resource Quotas and Rate Limiting Guide

This guide explains how to configure and manage resource quotas and rate limits in Blixard for multi-tenant environments.

## Overview

Blixard's quota system provides:
- **Per-tenant resource limits** - CPU, memory, disk, VM count
- **Rate limiting** - Control API request rates per tenant
- **Resource reservation** - Two-phase commit for safe allocation
- **Usage tracking** - Historical resource consumption data
- **Warning thresholds** - Alerts before hitting hard limits
- **Dynamic adjustment** - Change quotas without restarts

## Architecture

### Resource Types

The system tracks these resource types:
- **CPU** - Virtual CPUs (vCPUs)
- **Memory** - RAM in megabytes
- **Disk** - Storage in gigabytes
- **VM Count** - Number of VMs
- **Network Bandwidth** - Mbps (future)
- **IP Addresses** - Count (future)

### Quota Lifecycle

```
1. Check Quota → 2. Reserve Resources → 3. Perform Operation → 4. Commit/Release
```

This two-phase commit pattern prevents race conditions and ensures accurate tracking.

## Configuration

### Default Quotas

When a tenant is created without custom quotas:
```toml
[quotas.defaults]
cpu = { limit = 100, warning = 80 }
memory = { limit = 102400, warning = 81920 }  # 100GB, 80GB warning
disk = { limit = 1000, warning = 800 }         # 1TB, 800GB warning
vm_count = { limit = 50, warning = 40 }
```

### Custom Tenant Quotas

Configure per-tenant quotas in `/etc/blixard/quotas.toml`:

```toml
[tenants.enterprise]
cpu = { limit = 500, warning = 400 }
memory = { limit = 512000, warning = 409600 }  # 500GB
disk = { limit = 5000, warning = 4000 }         # 5TB
vm_count = { limit = 200, warning = 160 }

[tenants.startup]
cpu = { limit = 50, warning = 40 }
memory = { limit = 51200, warning = 40960 }    # 50GB
disk = { limit = 500, warning = 400 }           # 500GB
vm_count = { limit = 25, warning = 20 }
```

### Rate Limits

Configure operation rate limits:

```toml
[rate_limits.defaults]
vm_create = { max_requests = 10, window_seconds = 60 }    # 10 VMs per minute
vm_delete = { max_requests = 20, window_seconds = 60 }    # 20 deletes per minute
vm_list = { max_requests = 100, window_seconds = 60 }     # 100 lists per minute
api_calls = { max_requests = 1000, window_seconds = 60 }  # 1000 API calls per minute

[rate_limits.tenants.free_tier]
vm_create = { max_requests = 2, window_seconds = 3600 }   # 2 VMs per hour
api_calls = { max_requests = 100, window_seconds = 60 }   # 100 API calls per minute
```

## Usage Examples

### CLI Commands

```bash
# Initialize tenant with default quotas
blixard quota init-tenant --name acme

# Set custom quotas
blixard quota set --tenant acme --resource cpu --limit 200 --warning 160
blixard quota set --tenant acme --resource memory --limit 204800 --warning 163840

# Check tenant usage
blixard quota usage --tenant acme

# Set rate limits
blixard quota rate-limit --tenant acme --operation vm_create --max 5 --window 60

# View usage history
blixard quota history --tenant acme --resource cpu --limit 50
```

### API Integration

The quota system automatically integrates with VM operations:

```bash
# Create VM (automatically checks and reserves quota)
curl -H "Authorization: Bearer $TOKEN" \
     -H "x-tenant-id: acme" \
     -H "Content-Type: application/json" \
     -d '{"config": {"name": "web-1", "vcpus": 4, "memory": 8192}}' \
     https://blixard-api/v1/vms

# Response if quota exceeded:
{
  "error": "Resource quota exceeded",
  "code": "RESOURCE_EXHAUSTED",
  "details": "Cpu quota exceeded: requested 4, available 2"
}
```

### Programmatic Usage

```rust
use blixard_core::quota_system::{QuotaManager, ResourceType};

// Check quota before operation
let quota_check = quota_manager.check_quota(&tenant_id, &vm_config).await?;
if !quota_check.allowed {
    return Err(format!("Quota exceeded: {}", quota_check.reason.unwrap()));
}

// Reserve resources
let mut resources = HashMap::new();
resources.insert(ResourceType::Cpu, vm_config.vcpus as u64);
resources.insert(ResourceType::Memory, vm_config.memory as u64);
quota_manager.reserve_resources(&tenant_id, &vm_id, resources.clone()).await?;

// Perform operation...

// Commit on success
quota_manager.commit_resources(&tenant_id, &vm_id, resources).await?;

// Or release on failure
quota_manager.release_reserved(&tenant_id, &vm_id, resources).await?;
```

## Monitoring and Alerts

### Quota Metrics

Available metrics for monitoring:
- `blixard_quota_usage` - Current resource usage by tenant and type
- `blixard_quota_limit` - Configured limits
- `blixard_quota_available` - Available resources
- `blixard_quota_usage_percent` - Usage percentage
- `blixard_rate_limit_requests` - Rate limit request counts
- `blixard_rate_limit_rejected` - Rejected requests

### Prometheus Alerts

Example alert rules:

```yaml
groups:
  - name: quota_alerts
    rules:
      - alert: QuotaWarningThreshold
        expr: blixard_quota_usage_percent > 80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Tenant {{ $labels.tenant }} approaching {{ $labels.resource }} quota"
          description: "{{ $labels.tenant }} is using {{ $value }}% of {{ $labels.resource }} quota"
      
      - alert: QuotaCritical
        expr: blixard_quota_usage_percent > 95
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Tenant {{ $labels.tenant }} critical {{ $labels.resource }} usage"
          description: "{{ $labels.tenant }} is using {{ $value }}% of {{ $labels.resource }} quota"
      
      - alert: RateLimitExceeded
        expr: rate(blixard_rate_limit_rejected[5m]) > 10
        labels:
          severity: warning
        annotations:
          summary: "High rate limit rejections for {{ $labels.tenant }}"
          description: "{{ $labels.tenant }} has {{ $value }} rejected requests per second"
```

### Grafana Dashboard

Import the quota dashboard from `/dashboards/quota-overview.json` which includes:
- Per-tenant resource usage gauges
- Usage trends over time
- Rate limit hit rates
- Top consumers by resource type
- Quota utilization heatmap

## Best Practices

### 1. Capacity Planning

- Monitor usage trends to predict when quotas need adjustment
- Set warning thresholds at 80% to allow time for increases
- Plan for peak usage periods (deployments, scaling events)

### 2. Grace Periods

For production workloads, implement grace periods:
```rust
// Allow temporary 10% overuse during grace period
if usage_percent > 100.0 && usage_percent <= 110.0 {
    if within_grace_period(&tenant_id) {
        log::warn!("Tenant {} exceeding quota during grace period", tenant_id);
        return Ok(true); // Allow operation
    }
}
```

### 3. Quota Inheritance

Use quota templates for similar tenants:
```toml
[templates.startup]
cpu = { limit = 50, warning = 40 }
memory = { limit = 51200, warning = 40960 }

[tenants.startup_a]
template = "startup"

[tenants.startup_b]
template = "startup"
cpu = { limit = 75, warning = 60 }  # Override CPU only
```

### 4. Emergency Overrides

For critical situations:
```bash
# Temporarily disable quota enforcement
blixard quota override --tenant acme --disable --duration 1h --reason "Emergency scaling"

# Increase limits temporarily
blixard quota boost --tenant acme --resource cpu --factor 2.0 --duration 4h
```

## Troubleshooting

### Common Issues

#### "Quota exceeded but resources appear available"

Check for reserved but uncommitted resources:
```bash
blixard quota debug --tenant acme --show-reserved
```

#### "Rate limit hit despite low usage"

Verify window configuration:
```bash
blixard quota rate-limit-status --tenant acme --operation vm_create
```

#### "Quotas not enforced"

1. Check quota enforcement is enabled:
   ```toml
   [quotas]
   enabled = true
   enforcement_mode = "strict"  # or "soft" for warnings only
   ```

2. Verify tenant has quotas initialized:
   ```bash
   blixard quota list-tenants
   ```

### Debug Logging

Enable detailed quota logging:
```bash
export RUST_LOG=blixard_core::quota_system=debug
```

## Migration Guide

### From No Quotas to Quota System

1. **Analyze current usage**
   ```bash
   blixard analyze-usage --output usage-report.json
   ```

2. **Set initial quotas 20% above current usage**
   ```bash
   blixard quota migrate --safety-margin 1.2 --dry-run
   blixard quota migrate --safety-margin 1.2 --apply
   ```

3. **Enable in soft mode first**
   ```toml
   [quotas]
   enforcement_mode = "soft"  # Log warnings but don't block
   ```

4. **Monitor for 1 week, then switch to strict mode**

### Quota Data Migration

Export/import quota configurations:
```bash
# Export current quotas
blixard quota export --output quotas-backup.json

# Import to new cluster
blixard quota import --file quotas-backup.json
```

## Integration with Billing

The quota system can integrate with billing systems:

```rust
// Example billing integration
impl BillingIntegration for QuotaManager {
    async fn get_billable_usage(&self, tenant_id: &str, period: Period) -> BillableUsage {
        let history = self.get_usage_history(tenant_id, None, usize::MAX).await;
        
        // Calculate peak usage for each resource
        let peak_cpu = calculate_peak_usage(&history, ResourceType::Cpu, period);
        let peak_memory = calculate_peak_usage(&history, ResourceType::Memory, period);
        
        BillableUsage {
            tenant_id: tenant_id.to_string(),
            period,
            peak_cpu_hours: peak_cpu * period.hours(),
            peak_memory_gb_hours: peak_memory / 1024.0 * period.hours(),
            // ... other billable metrics
        }
    }
}
```

## Performance Considerations

- Quota checks are very fast (microseconds)
- Resource reservation uses in-memory locks
- History is capped at 10,000 records per tenant
- Rate limit windows use sliding window algorithm

For high-frequency operations, consider caching:
```rust
// Cache quota check results for 1 second
let cache_key = format!("{}:{:?}", tenant_id, resource_type);
if let Some(cached) = quota_cache.get(&cache_key) {
    if cached.timestamp.elapsed() < Duration::from_secs(1) {
        return Ok(cached.result);
    }
}
```

## Security Considerations

1. **Tenant Isolation** - Quotas are strictly per-tenant
2. **Admin Override Audit** - All manual quota changes are logged
3. **DoS Protection** - Rate limits prevent abuse
4. **Resource Exhaustion** - Hard limits prevent system overload

## Future Enhancements

Planned features:
- Hierarchical quotas (organization → team → user)
- Burst capacity with token buckets
- Predictive quota warnings using ML
- Cross-region quota federation
- Resource type priorities (guarantee minimums)