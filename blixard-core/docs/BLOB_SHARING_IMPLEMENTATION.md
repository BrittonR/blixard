# Blob Sharing Implementation in IrohTransportV2

## Overview

We have successfully implemented blob sharing functionality in IrohTransportV2, enabling content-addressed file storage and retrieval. This implementation provides the foundation for P2P file sharing between Blixard nodes.

## Implementation Details

### Storage Backend

- **In-Memory Storage**: Currently uses a simple `HashMap<Hash, Vec<u8>>` for blob storage
- **Content Addressing**: Uses Blake3 hashing for content-addressed storage
- **Automatic Deduplication**: Identical content produces the same hash, preventing duplicate storage

### Core API

#### 1. `share_file(path: &Path) -> BlixardResult<Hash>`
- Reads file content from disk
- Computes Blake3 hash of the content
- Stores content in the blob store
- Returns the content hash for future retrieval

#### 2. `download_file(hash: Hash, output_path: &Path) -> BlixardResult<()>`
- Retrieves blob content by hash from local store
- Writes content to the specified output path
- Returns error if blob not found

#### 3. `send_blob_info(peer_addr: &NodeAddr, hash: Hash, filename: &str) -> BlixardResult<()>`
- Sends blob metadata to a peer
- Uses FileTransfer document type
- Message format: `BLOB:<hash>:<filename>`

#### 4. `download_blob_from_peer(peer_addr: &NodeAddr, hash: Hash, output_path: &Path) -> BlixardResult<()>`
- Requests a blob from a peer using GET_BLOB protocol
- Verifies received content matches expected hash
- Stores blob locally for future access

#### 5. `handle_blob_requests<F>(handler: F) -> BlixardResult<()>`
- Accepts incoming blob requests from peers
- Responds to GET_BLOB requests with blob content
- Supports custom request handlers

## Usage Examples

### Local Blob Storage

```rust
// Share a file
let hash = transport.share_file(&Path::new("document.pdf")).await?;
println!("File shared with hash: {}", hash);

// Download the file
transport.download_file(hash, &Path::new("downloaded.pdf")).await?;
```

### P2P Blob Sharing

```rust
// Node A: Share a file and send info to Node B
let hash = transport_a.share_file(&file_path).await?;
transport_a.send_blob_info(&node_b_addr, hash, "important.txt").await?;

// Node B: Request and download the blob from Node A
transport_b.download_blob_from_peer(&node_a_addr, hash, &output_path).await?;
```

## Key Features

1. **Content Addressing**: Files are identified by their content hash, not location
2. **Deduplication**: Identical files share the same storage
3. **Integrity Verification**: Downloaded content is verified against expected hash
4. **P2P Protocol**: Simple GET_BLOB protocol for peer-to-peer transfers
5. **Extensible**: Custom handlers can be added for specialized requests

## Future Enhancements

1. **Persistent Storage**: Replace in-memory store with disk-based storage (e.g., using iroh-blobs FsStore)
2. **Chunking**: Support for large file chunking and streaming
3. **Compression**: Optional compression for stored blobs
4. **Metadata**: Extended metadata support (MIME types, creation time, etc.)
5. **Replication**: Automatic replication across cluster nodes
6. **Garbage Collection**: Remove unreferenced blobs
7. **Progress Tracking**: Download/upload progress callbacks

## Testing

The implementation includes comprehensive tests:
- `test_file_operations`: Tests local blob storage and retrieval
- `test_blob_sharing_between_peers`: Tests P2P blob info exchange
- Example programs demonstrate usage patterns

## Integration with Blixard

This blob sharing capability can be used for:
- VM image distribution across the cluster
- Log file aggregation and sharing
- Configuration file distribution
- General file transfer between nodes

The implementation is ready for integration with higher-level Blixard components that need file sharing capabilities.