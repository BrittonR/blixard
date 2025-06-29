# P2P Transfer Metrics Guide

This guide describes the metrics available for monitoring P2P Nix image transfers in Blixard.

## Overview

Blixard collects comprehensive metrics for P2P image operations to help monitor:
- Transfer performance and bandwidth usage
- Cache effectiveness and deduplication rates
- Image verification success rates
- Active transfer tracking

All metrics are exposed via OpenTelemetry and available at the `/metrics` endpoint in Prometheus format.

## Available Metrics

### Import Metrics

**p2p.image.imports.total**
- Type: Counter
- Description: Total number of P2P image imports
- Labels: `artifact_type` (microvm, container, storepath, closure)

**p2p.image.imports.failed**
- Type: Counter
- Description: Number of failed P2P image imports
- Labels: `artifact_type`

### Download Metrics

**p2p.image.downloads.total**
- Type: Counter
- Description: Total number of P2P image downloads

**p2p.image.downloads.failed**
- Type: Counter
- Description: Number of failed P2P image downloads

**p2p.transfer.duration**
- Type: Histogram
- Description: Duration of P2P transfers in seconds
- Useful for tracking transfer speeds and identifying slow transfers

### Transfer Volume Metrics

**p2p.bytes_transferred.total**
- Type: Counter
- Description: Total bytes transferred via P2P
- Labels: `artifact_type` (for imports)

**p2p.chunks_transferred.total**
- Type: Counter
- Description: Total chunks transferred via P2P

**p2p.chunks_deduplicated.total**
- Type: Counter
- Description: Number of chunks deduplicated during P2P transfers
- Higher values indicate better storage efficiency

### Verification Metrics

**p2p.verification.success**
- Type: Counter
- Description: Number of successful P2P image verifications
- Labels: `verification_type` (nar_hash, chunk_hash)

**p2p.verification.failed**
- Type: Counter
- Description: Number of failed P2P image verifications
- Labels: `verification_type`

### Cache Metrics

**p2p.cache.hits**
- Type: Counter
- Description: Number of P2P cache hits
- Labels: `cache_type` (chunk, vm_image)

**p2p.cache.misses**
- Type: Counter
- Description: Number of P2P cache misses
- Labels: `cache_type`

### Active Transfer Tracking

**p2p.transfers.active**
- Type: UpDownCounter
- Description: Number of active P2P transfers
- Useful for monitoring concurrent transfer load

## Usage Examples

### Prometheus Queries

1. **Transfer Success Rate**
   ```promql
   rate(p2p_image_downloads_total[5m]) - rate(p2p_image_downloads_failed[5m])
   ```

2. **Deduplication Efficiency**
   ```promql
   p2p_chunks_deduplicated_total / (p2p_chunks_transferred_total + p2p_chunks_deduplicated_total)
   ```

3. **Average Transfer Speed (MB/s)**
   ```promql
   rate(p2p_bytes_transferred_total[5m]) / 1048576
   ```

4. **Cache Hit Rate**
   ```promql
   p2p_cache_hits / (p2p_cache_hits + p2p_cache_misses)
   ```

5. **Verification Failure Rate**
   ```promql
   rate(p2p_verification_failed[5m]) / rate(p2p_verification_success[5m] + p2p_verification_failed[5m])
   ```

### Grafana Dashboard

A sample Grafana dashboard configuration:

```json
{
  "panels": [
    {
      "title": "P2P Transfer Rate",
      "targets": [{
        "expr": "rate(p2p_bytes_transferred_total[5m]) / 1048576"
      }]
    },
    {
      "title": "Active Transfers",
      "targets": [{
        "expr": "p2p_transfers_active"
      }]
    },
    {
      "title": "Deduplication Rate",
      "targets": [{
        "expr": "100 * (p2p_chunks_deduplicated_total / (p2p_chunks_transferred_total + p2p_chunks_deduplicated_total))"
      }]
    },
    {
      "title": "Cache Hit Rate",
      "targets": [{
        "expr": "100 * (p2p_cache_hits / (p2p_cache_hits + p2p_cache_misses))"
      }]
    }
  ]
}
```

## Integration Points

### NixImageStore

The `NixImageStore` automatically records metrics for:
- Image imports (with deduplication stats)
- Image downloads (with transfer timing)
- Chunk operations (transfers and deduplication)
- Verification operations

### MicrovmBackend

The VM backend records:
- VM image cache hits/misses when starting VMs
- Image download triggers for VMs using P2P images

## Example Code

```rust
use blixard_core::metrics_otel::{
    record_p2p_image_import,
    record_p2p_image_download,
    record_p2p_chunk_transfer,
    record_p2p_verification,
    record_p2p_cache_access,
    start_p2p_transfer,
};

// Record successful import
record_p2p_image_import("microvm", true, total_size);

// Track transfer with automatic duration recording
{
    let _guard = start_p2p_transfer();
    // Transfer happens here
    // Duration recorded when guard drops
}

// Record chunk operations
for chunk in chunks {
    if already_have_chunk {
        record_p2p_chunk_transfer(chunk.size, true); // deduplicated
    } else {
        record_p2p_chunk_transfer(chunk.size, false); // transferred
    }
}

// Record verification result
match verify_nar_hash() {
    Ok(_) => record_p2p_verification(true, "nar_hash"),
    Err(_) => record_p2p_verification(false, "nar_hash"),
}
```

## Monitoring Best Practices

1. **Set up alerts for:**
   - High verification failure rates (> 1%)
   - Low cache hit rates (< 50%)
   - Stuck active transfers (> 0 for extended periods)
   - Failed imports/downloads spikes

2. **Track trends for:**
   - Deduplication efficiency over time
   - Transfer speeds by time of day
   - Cache effectiveness as the cluster grows

3. **Capacity planning:**
   - Monitor bytes transferred to plan bandwidth
   - Track unique chunks to estimate storage needs
   - Watch active transfers for concurrency limits

## Future Enhancements

- Per-node transfer metrics
- Transfer speed by peer distance
- Chunk popularity tracking
- Predictive prefetching metrics
- Network topology impact metrics