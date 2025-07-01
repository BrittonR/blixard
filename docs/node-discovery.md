# Node Discovery System

The Blixard node discovery system enables automatic peer discovery and connection management for cluster nodes using Iroh P2P transport.

## Overview

The discovery system consists of two main components:

1. **NodeDiscovery** - Low-level discovery of individual node connection information
2. **DiscoveryManager** - High-level management of discovered peers with automatic connection

## Discovery Sources

### 1. Node Registry Files

Node registry files contain Iroh connection information for a single node:

```json
{
  "cluster_node_id": 1,
  "iroh_node_id": "base64-encoded-node-id",
  "direct_addresses": ["192.168.1.10:7001", "10.0.0.5:7001"],
  "relay_url": "https://relay.iroh.network",
  "address": "192.168.1.10:7001"
}
```

Registry files can be:
- Local files on disk
- HTTP/HTTPS URLs (planned)
- Shared network locations

### 2. Static Configuration Files

Static discovery files list multiple peers in one file:

```json
[
  {
    "node_id": 1,
    "iroh_node_id": "base64-encoded-node-id",
    "direct_addresses": ["192.168.1.10:7001"],
    "relay_url": "https://relay.iroh.network",
    "source": "static",
    "discovered_at": "2024-01-01T00:00:00Z"
  },
  {
    "node_id": 2,
    "iroh_node_id": "another-base64-node-id",
    "direct_addresses": ["192.168.1.11:7002"],
    "relay_url": null,
    "source": "static",
    "discovered_at": "2024-01-01T00:00:00Z"
  }
]
```

### 3. DNS-Based Discovery (Planned)

DNS discovery will use SRV or TXT records to discover cluster nodes.

## Configuration

### Automatic Discovery Setup

Discovery is automatically configured when:

1. **Join Address**: When `--join` is specified, it's used as a discovery source
2. **Environment Variable**: `BLIXARD_NODE_REGISTRIES` can list multiple registry paths (comma-separated)
3. **Static File**: If `{data_dir}/discovery-peers.json` exists, it's loaded automatically

### Manual Configuration

```rust
use blixard::discovery_manager::{DiscoveryConfig, DiscoverySource};

let config = DiscoveryConfig {
    sources: vec![
        DiscoverySource::NodeRegistry {
            locations: vec![
                "/path/to/node1.json".to_string(),
                "/path/to/node2.json".to_string(),
            ],
        },
        DiscoverySource::StaticFile {
            path: "/etc/blixard/peers.json".to_string(),
        },
    ],
    refresh_interval: Duration::from_secs(60),
    auto_connect: true,
};
```

## Usage Examples

### Bootstrap Node

```bash
# Start a bootstrap node
blixard node --id 1 --bind 127.0.0.1:7001

# The node will save its registry to data/node1/node-registry.json
```

### Joining Node with Discovery

```bash
# Join using a registry file
blixard node --id 2 --bind 127.0.0.1:7002 --join /path/to/node1-registry.json

# Or set environment variable for multiple registries
export BLIXARD_NODE_REGISTRIES="/path/to/node1.json,/path/to/node2.json"
blixard node --id 3 --bind 127.0.0.1:7003
```

### Creating Registry Files

```rust
// Save node registry programmatically
let entry = NodeRegistryEntry {
    cluster_node_id: node_id,
    iroh_node_id: base64::encode(node_addr.node_id().as_bytes()),
    direct_addresses: node_addr.direct_addresses()
        .map(|addr| addr.to_string())
        .collect(),
    relay_url: node_addr.relay_url().map(|url| url.to_string()),
    address: bind_addr.to_string(),
};

save_node_registry("/path/to/registry.json", &entry).await?;
```

## Auto-Connection

When `auto_connect` is enabled (default), the discovery manager will:

1. Automatically connect to discovered peers
2. Retry failed connections with exponential backoff
3. Respect circuit breaker states for failing peers
4. Update peer connection status in the cluster

## Integration with Raft

The discovery system integrates with Raft consensus by:

1. Discovering initial cluster members before Raft starts
2. Providing peer addresses for Raft message routing
3. Maintaining connection health for leader election

## Monitoring

Discovery events are logged at INFO level:
- Peer discovery from each source
- Successful connections
- Connection failures
- Discovery source errors

## Best Practices

1. **Registry Distribution**: Use a shared filesystem, configuration management, or cloud storage to distribute registry files
2. **Multiple Sources**: Configure multiple discovery sources for redundancy
3. **Refresh Interval**: Set appropriate refresh intervals based on cluster dynamics (default: 60s)
4. **Security**: Protect registry files as they contain connection information

## Future Enhancements

1. **Cloud Provider Integration**: Discover nodes via AWS/GCP/Azure metadata services
2. **mDNS Discovery**: Local network discovery for development clusters
3. **Gossip Protocol**: Peer-to-peer discovery information exchange
4. **Registry Signing**: Cryptographic signatures for registry authenticity