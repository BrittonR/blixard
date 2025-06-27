# P2P Implementation in Blixard

## Overview

Blixard includes a comprehensive peer-to-peer (P2P) networking system built on [Iroh](https://github.com/n0-computer/iroh), providing efficient resource sharing and direct node-to-node communication. This implementation enables:

- **Direct VM image sharing** between nodes without central storage
- **NAT traversal** for nodes behind firewalls
- **Content-addressed storage** using BLAKE3 hashing
- **Automatic peer discovery** and connection management
- **Transfer queue management** with priority support
- **Real-time monitoring** through the TUI

## Architecture

### Core Components

1. **IrohTransport** (`iroh_transport.rs`)
   - Low-level Iroh node management
   - Document synchronization
   - File sharing primitives
   - Network address management

2. **P2pImageStore** (`p2p_image_store.rs`)
   - VM image metadata management
   - Content-addressed storage interface
   - Cache management
   - Image versioning support

3. **P2pManager** (`p2p_manager.rs`)
   - High-level P2P orchestration
   - Peer lifecycle management
   - Transfer queue with priorities
   - Event-driven architecture
   - Health monitoring

4. **TUI Integration** (`p2p_view.rs`)
   - Real-time P2P status display
   - Peer connection visualization
   - Transfer progress monitoring
   - Interactive controls

## Usage

### Enabling P2P in the TUI

1. Launch the Blixard TUI
2. Press `5` to switch to the P2P tab
3. Press `p` to enable P2P networking

### P2P Keyboard Shortcuts

- `p` - Toggle P2P on/off
- `c` - Connect to a peer
- `r` - Refresh P2P statistics
- `u` - Upload selected VM image
- `d` - Download selected image
- `q` - Queue download with priority
- `s` - Show P2P statistics
- `l` - List available resources

### Programmatic Usage

```rust
use blixard_core::p2p_manager::{P2pManager, P2pConfig, ResourceType, TransferPriority};

// Initialize P2P manager
let config = P2pConfig::default();
let manager = P2pManager::new(node_id, data_dir, config).await?;
manager.start().await?;

// Connect to a peer
manager.connect_peer("192.168.1.100:7001").await?;

// Upload a VM image
manager.upload_resource(
    ResourceType::VmImage,
    "ubuntu-server",
    "22.04",
    Path::new("/path/to/image.img"),
).await?;

// Queue a download
let request_id = manager.request_download(
    ResourceType::VmImage,
    "alpine-linux",
    "3.18",
    TransferPriority::High,
).await?;

// Monitor events
let event_rx = manager.event_receiver();
let mut rx = event_rx.write().await;
while let Some(event) = rx.recv().await {
    match event {
        P2pEvent::TransferCompleted(id) => {
            println!("Transfer {} completed", id);
        }
        _ => {}
    }
}
```

## Features

### 1. Automatic Peer Discovery
- Periodic scanning for new peers
- Automatic connection retry with exponential backoff
- Health monitoring with disconnection detection

### 2. Resource Management
- **VM Images**: Full support for VM image sharing with metadata
- **Configurations**: Share cluster configurations
- **Logs**: Distributed log aggregation (planned)
- **Metrics**: Time-series data sharing (planned)

### 3. Transfer Management
- **Priority Queue**: Critical > High > Normal > Low
- **Concurrent Transfers**: Configurable limit (default: 3)
- **Bandwidth Limiting**: Optional per-transfer limits
- **Progress Tracking**: Real-time speed and ETA
- **Automatic Retry**: Failed transfers with backoff

### 4. Security Features
- **Content Verification**: BLAKE3 hash verification
- **Peer Authentication**: Based on Iroh's cryptographic identities
- **Encrypted Transport**: All P2P traffic is encrypted

### 5. Performance Optimizations
- **Incremental Sync**: Only transfer changed blocks
- **Local Caching**: Avoid redundant downloads
- **Direct Connections**: Peer-to-peer when possible
- **Relay Fallback**: Through Iroh relay servers when needed

## Configuration

### P2pConfig Options

```rust
pub struct P2pConfig {
    pub max_concurrent_transfers: usize,     // Default: 3
    pub transfer_timeout: Duration,          // Default: 5 minutes
    pub peer_discovery_interval: Duration,   // Default: 30 seconds
    pub health_check_interval: Duration,     // Default: 60 seconds
    pub max_retry_attempts: u32,            // Default: 3
    pub bandwidth_limit_mbps: Option<f64>,  // Default: None (unlimited)
}
```

### Environment Variables

- `BLIXARD_P2P_PORT`: Override default P2P port
- `BLIXARD_P2P_RELAY`: Custom relay server URL
- `BLIXARD_P2P_CACHE_DIR`: Custom cache directory

## Monitoring

### P2P Events

The P2P manager emits events for all significant operations:

```rust
pub enum P2pEvent {
    PeerDiscovered(PeerInfo),
    PeerConnected(String),
    PeerDisconnected(String),
    ResourceAvailable(ResourceInfo),
    TransferStarted(String),
    TransferProgress(String, TransferProgress),
    TransferCompleted(String),
    TransferFailed(String, String),
}
```

### Metrics

The P2P system exposes metrics through the standard Blixard metrics system:

- `blixard_p2p_peers_total`: Number of connected peers
- `blixard_p2p_transfers_active`: Active transfer count
- `blixard_p2p_bytes_sent`: Total bytes uploaded
- `blixard_p2p_bytes_received`: Total bytes downloaded
- `blixard_p2p_transfer_duration`: Transfer completion times

## Troubleshooting

### Common Issues

1. **"Failed to create Iroh node"**
   - Ensure the data directory is writable
   - Check if another instance is using the same port

2. **"No peers discovered"**
   - Verify network connectivity
   - Check firewall rules for P2P ports
   - Ensure peers are running compatible versions

3. **"Transfer stuck in queue"**
   - Check if source peer has the resource
   - Verify network path between peers
   - Check transfer timeout settings

### Debug Mode

Enable P2P debug logging:
```bash
RUST_LOG=blixard_core::p2p_manager=debug,iroh=debug blixard
```

## Future Enhancements

1. **Distributed Hash Table (DHT)** for automatic resource discovery
2. **Bandwidth scheduling** with time-based policies
3. **Partial file sharing** for large VM images
4. **Compression** for network efficiency
5. **Deduplication** across similar images
6. **P2P cluster formation** for automatic mesh networking

## Example: Multi-Node P2P Cluster

```bash
# Node 1: Start with P2P enabled
blixard node --id 1 --bind 127.0.0.1:7001 --enable-p2p

# Node 2: Start and connect to Node 1
blixard node --id 2 --bind 127.0.0.1:7002 --enable-p2p --p2p-peer 127.0.0.1:7001

# Node 3: Start and connect to both
blixard node --id 3 --bind 127.0.0.1:7003 --enable-p2p \
  --p2p-peer 127.0.0.1:7001 \
  --p2p-peer 127.0.0.1:7002

# Upload an image from Node 1
blixard vm upload --name ubuntu-server --version 22.04 --file ubuntu.img

# Download on Node 3 (will find it via P2P)
blixard vm download --name ubuntu-server --version 22.04
```

## API Reference

See the API documentation for detailed interface descriptions:
- `blixard_core::iroh_transport`
- `blixard_core::p2p_image_store`
- `blixard_core::p2p_manager`