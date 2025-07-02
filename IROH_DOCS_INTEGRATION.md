# Iroh-Docs Integration for Distributed Document Storage

## Current Status

The current implementation provides a foundation for distributed document storage using iroh-docs 0.90. However, due to API changes and the evolving nature of the iroh-docs library, we've implemented a simplified approach that provides the necessary interfaces while the full iroh-docs integration is developed.

## Implementation Overview

### IrohTransportV3 (Simplified)

Located in `blixard-core/src/iroh_transport_v3.rs`, this implementation:

1. **Uses IrohTransportV2 internally** - Provides compatibility and a working baseline
2. **Maintains the same API** - All document operations have the same interface
3. **Ready for migration** - When iroh-docs 0.90 API stabilizes, we can implement the full version

### Key Features

1. **Document Types**
   - ClusterConfig - Shared cluster configuration
   - VmImages - VM image catalog and metadata
   - Logs - Distributed log aggregation
   - Metrics - Time-series metrics data
   - FileTransfer - Temporary documents for file transfers

2. **Operations Supported**
   - `create_or_join_doc` - Create or join a document
   - `write_to_doc` - Write key-value pairs to documents
   - `read_from_doc` - Read values by key
   - `get_doc_ticket` - Get sharing tickets
   - `join_doc_from_ticket` - Join documents via tickets
   - `subscribe_to_doc_events` - Event subscription (placeholder)

3. **P2P Features**
   - Direct peer-to-peer messaging
   - Health checks with RTT measurement
   - Bandwidth tracking
   - Connection monitoring

## Examples

### Basic Document Operations

```rust
use blixard_core::iroh_transport_v3::IrohTransportV3;
use blixard_core::iroh_transport_v2::DocumentType;

// Create transport
let transport = IrohTransportV3::new(1, "/tmp/node1").await?;

// Create a document
transport.create_or_join_doc(DocumentType::ClusterConfig, true).await?;

// Write data
transport.write_to_doc(DocumentType::ClusterConfig, "leader", b"node-1").await?;

// Read data
let leader = transport.read_from_doc(DocumentType::ClusterConfig, "leader").await?;
```

### Document Sharing

```rust
// Node 1: Create and share
let transport1 = IrohTransportV3::new(1, "/tmp/node1").await?;
transport1.create_or_join_doc(DocumentType::VmImages, true).await?;
let ticket = transport1.get_doc_ticket(DocumentType::VmImages).await?;

// Node 2: Join via ticket
let transport2 = IrohTransportV3::new(2, "/tmp/node2").await?;
transport2.join_doc_from_ticket(&ticket, DocumentType::VmImages).await?;
```

## Running the Examples

1. **Basic Demo** - Shows document creation, sharing, and synchronization
   ```bash
   cargo run --example iroh_docs_demo
   ```

2. **Multi-Writer Demo** - Demonstrates multiple nodes writing to the same document
   ```bash
   cargo run --example iroh_docs_multi_writer
   ```

## Future Work

When iroh-docs 0.90 API is fully documented and stable, we should:

1. **Implement Real Document Storage**
   - Use actual iroh-docs Store API
   - Implement proper author management
   - Add document persistence

2. **Enable Document Synchronization**
   - Use iroh-gossip for document sync
   - Implement proper CRDT operations
   - Add conflict resolution

3. **Integrate Blob Storage**
   - Connect iroh-blobs for large file storage
   - Implement content-addressed storage
   - Add deduplication

4. **Add Advanced Features**
   - Document versioning
   - Access control
   - Encryption at rest
   - Selective sync

## Technical Details

### Dependencies

```toml
iroh = "0.90"
iroh-blobs = "0.90"
iroh-docs = { git = "https://github.com/n0-computer/iroh-docs", branch = "Frando/iroh-0.90" }
iroh-gossip = { git = "https://github.com/n0-computer/iroh-gossip", branch = "main" }
```

### Architecture

The current architecture uses a layered approach:

```
IrohTransportV3 (API Layer)
    ↓
IrohTransportV2 (Implementation)
    ↓
Iroh Endpoint (Network Layer)
```

This allows us to maintain API stability while the underlying implementation evolves.

## Known Limitations

1. **In-Memory Storage** - Documents are currently stored in memory only
2. **No Real Sync** - Document synchronization is not yet implemented
3. **Simplified Tickets** - Sharing tickets are placeholder implementations
4. **No Persistence** - Documents are lost on restart

These limitations will be addressed once the iroh-docs 0.90 API is finalized.

## Testing

The implementation includes comprehensive tests:

- Unit tests for all operations
- Integration tests for multi-node scenarios
- Examples demonstrating real-world usage

Run tests with:
```bash
cargo test -p blixard-core iroh_transport_v3
```