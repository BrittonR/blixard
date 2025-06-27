# Cluster Export/Import

## Overview

Blixard provides comprehensive cluster state export and import functionality, allowing you to:

- **Backup cluster configurations** for disaster recovery
- **Migrate clusters** between environments
- **Share cluster templates** with other teams
- **Version control** cluster configurations
- **Replicate environments** for testing

The export/import system supports both file-based and P2P-based transfers, with optional compression and selective data inclusion.

## Export Format

Cluster exports use a versioned JSON format (currently v1.0) containing:

```json
{
  "version": "1.0",
  "exported_at": "2025-01-27T10:30:00Z",
  "cluster_name": "production-cluster",
  "exported_by_node": 1,
  "nodes": [...],
  "vms": {...},
  "raft_state": {...},
  "metadata": {...}
}
```

## Command Line Usage

### Export Cluster State

Basic export to file:
```bash
blixard cluster export -o cluster-backup.json -c my-cluster
```

Export with compression (default):
```bash
blixard cluster export -o cluster-backup.json.gz -c my-cluster
```

Export without compression:
```bash
blixard cluster export -o cluster-backup.json -c my-cluster --no-compress
```

Include VM images in export:
```bash
blixard cluster export -o cluster-full.json.gz -c my-cluster --include-images
```

Include telemetry data (logs, metrics):
```bash
blixard cluster export -o cluster-with-telemetry.json.gz -c my-cluster --include-telemetry
```

### Import Cluster State

Import from file (replace existing state):
```bash
blixard cluster import -i cluster-backup.json.gz
```

Import and merge with existing state:
```bash
blixard cluster import -i cluster-backup.json.gz --merge
```

### P2P Export/Import

Share cluster state via P2P network:
```bash
# On source node
blixard cluster export -o backup.json.gz -c my-cluster --p2p-share
# Output: Share this ticket with other nodes: <ticket-string>

# On target node
blixard cluster import -i <ticket-string> --p2p
```

## Use Cases

### 1. Disaster Recovery

Create regular backups of your cluster state:

```bash
# Automated daily backup
0 2 * * * blixard cluster export -o /backups/cluster-$(date +\%Y\%m\%d).json.gz -c prod-cluster
```

Restore from backup:
```bash
blixard cluster import -i /backups/cluster-20250127.json.gz
```

### 2. Environment Replication

Export production cluster configuration:
```bash
blixard cluster export -o prod-config.json.gz -c production
```

Import to staging environment:
```bash
blixard cluster import -i prod-config.json.gz --merge
```

### 3. Cluster Migration

Export complete cluster state including VM images:
```bash
blixard cluster export -o migration.json.gz -c old-cluster --include-images
```

Import on new infrastructure:
```bash
blixard cluster import -i migration.json.gz
```

### 4. Configuration Templates

Create a template cluster configuration:
```bash
blixard cluster export -o template.json -c template-cluster --no-compress
```

Edit the JSON file to create variations, then import:
```bash
blixard cluster import -i custom-template.json
```

### 5. P2P Cluster Cloning

Share cluster configuration across air-gapped networks:
```bash
# Source network
blixard cluster export -o cluster.json.gz -c source --p2p-share
# Copy the ticket

# Target network
blixard cluster import -i <ticket> --p2p
```

## Export Options

| Option | Description | Default |
|--------|-------------|---------|
| `--include-images` | Include VM disk images in export | false |
| `--include-telemetry` | Include logs and metrics data | false |
| `--no-compress` | Skip gzip compression | false (compress by default) |
| `--p2p-share` | Share via P2P network | false |

## Import Options

| Option | Description | Default |
|--------|-------------|---------|
| `--merge` | Merge with existing state instead of replacing | false |
| `--p2p` | Import from P2P ticket instead of file | false |

## Best Practices

1. **Regular Backups**: Schedule automated exports for disaster recovery
2. **Version Control**: Store export files in git for configuration tracking
3. **Selective Exports**: Only include VM images when necessary (large files)
4. **Compression**: Use compression for storage efficiency
5. **Security**: Store exports securely, they contain sensitive cluster information

## Limitations

- Export files can be large if VM images are included
- P2P transfers require network connectivity between nodes
- Import operations may require cluster downtime for consistency
- Raft state is exported for reference but not fully restored

## Examples

### Complete Backup Script

```bash
#!/bin/bash
# backup-cluster.sh

BACKUP_DIR="/var/backups/blixard"
CLUSTER_NAME="production"
DATE=$(date +%Y%m%d-%H%M%S)

# Create backup directory
mkdir -p $BACKUP_DIR

# Export cluster state
blixard cluster export \
  -o "$BACKUP_DIR/cluster-$DATE.json.gz" \
  -c "$CLUSTER_NAME" \
  --include-telemetry

# Keep only last 7 days of backups
find $BACKUP_DIR -name "cluster-*.json.gz" -mtime +7 -delete

echo "Backup completed: cluster-$DATE.json.gz"
```

### Migration Workflow

```bash
# 1. Export from source cluster
blixard cluster export -o migration.json.gz -c source-cluster --include-images

# 2. Transfer file to new environment
scp migration.json.gz newserver:/tmp/

# 3. Import on target cluster
ssh newserver
blixard cluster import -i /tmp/migration.json.gz

# 4. Verify migration
blixard cluster status
```

## Troubleshooting

### "Failed to create Iroh node"
- Ensure the data directory is writable
- Check if another P2P instance is running

### "Unsupported export format version"
- Update Blixard to a compatible version
- Use the export tool from the same version

### Large export files
- Exclude VM images unless necessary
- Enable compression
- Consider P2P transfer for large files

### Import failures
- Verify file integrity (checksum)
- Check available disk space
- Review logs for specific errors