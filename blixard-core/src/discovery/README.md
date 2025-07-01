# Discovery Module

The discovery module provides a pluggable system for finding and managing Iroh nodes in the Blixard network. It supports multiple discovery mechanisms that can work together to maintain a comprehensive view of available nodes.

## Features

- **Static Discovery**: Configure known nodes via configuration files
- **DNS Discovery**: Use DNS SRV and TXT records to discover nodes
- **mDNS Discovery**: Automatic local network discovery using multicast DNS
- **Unified Management**: Single interface to manage all discovery providers
- **Event System**: Subscribe to node discovery/removal events
- **Metadata Support**: Attach custom metadata to discovered nodes

## Architecture

### Core Components

1. **DiscoveryProvider Trait**: Common interface for all discovery mechanisms
2. **DiscoveryManager**: Coordinates multiple providers and aggregates results
3. **IrohNodeInfo**: Information about discovered nodes
4. **DiscoveryEvent**: Events emitted when nodes are discovered or removed

### Discovery Providers

#### Static Discovery
- Reads node information from configuration
- Useful for bootstrap nodes or known infrastructure
- Configuration format: `node_id -> [addresses]`

#### DNS Discovery
- Queries DNS for SRV records (e.g., `_iroh._tcp.example.com`)
- Supports TXT records for additional metadata
- Automatic periodic refresh of DNS records
- Node ID encoded in hostname or TXT records

#### mDNS Discovery
- Automatic discovery on local networks
- Advertises own node and discovers others
- No configuration needed for basic operation
- Useful for development and local deployments

## Usage

```rust
use blixard_core::discovery::{DiscoveryConfig, DiscoveryManager};
use std::time::Duration;

// Configure discovery
let config = DiscoveryConfig {
    enable_static: true,
    enable_dns: true,
    enable_mdns: true,
    refresh_interval: 30,
    node_stale_timeout: 300,
    static_nodes: /* ... */,
    dns_domains: vec!["_iroh._tcp.example.com".to_string()],
    mdns_service_name: "_blixard-iroh._tcp.local".to_string(),
};

// Create and start manager
let mut manager = DiscoveryManager::new(config);

// Configure for local node (needed for mDNS advertisement)
manager.configure_for_node(my_node_id, vec![my_address]);

// Subscribe to events
let mut events = manager.subscribe().await;

// Start discovery
manager.start().await?;

// Handle discovery events
while let Some(event) = events.recv().await {
    match event {
        DiscoveryEvent::NodeDiscovered(info) => {
            println!("Found node: {} at {:?}", info.node_id, info.addresses);
        }
        DiscoveryEvent::NodeRemoved(node_id) => {
            println!("Node removed: {}", node_id);
        }
        DiscoveryEvent::NodeUpdated(info) => {
            println!("Node updated: {}", info.node_id);
        }
    }
}
```

## Configuration

### Static Nodes Format
```toml
[discovery.static_nodes]
"node_id_base64" = ["127.0.0.1:7000", "192.168.1.100:7000"]
```

### DNS Configuration
- Create SRV records: `_iroh._tcp.example.com`
- Target hostname should encode node ID
- Or use TXT records: `"node=<node_id> addr=<socket_addr>"`

### mDNS Service Name
- Default: `_blixard-iroh._tcp.local`
- Must follow mDNS service naming convention
- All nodes should use the same service name

## Implementation Notes

1. **Thread Safety**: All components are thread-safe and can be shared across tasks
2. **Error Handling**: Non-fatal errors (e.g., DNS failures) are logged but don't stop discovery
3. **Stale Node Cleanup**: Nodes not seen for `node_stale_timeout` are automatically removed
4. **Event Delivery**: Events are delivered asynchronously via channels
5. **Resource Management**: Background tasks are properly cleaned up on shutdown

## Future Enhancements

- DHT-based discovery for decentralized networks
- Kubernetes service discovery integration
- Consul/etcd backend support
- Node capability/service advertisement
- Geographic awareness for node selection
- Discovery result caching and persistence