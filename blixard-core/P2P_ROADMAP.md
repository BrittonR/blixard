# P2P Implementation Roadmap

## Phase 1: Core Document Operations (Week 1)
**Goal**: Unblock P2pImageStore and basic state synchronization

### Tasks:
1. **Implement Document API** (`iroh_transport.rs`)
   ```rust
   // Replace NotImplemented with actual Iroh calls:
   pub async fn create_or_join_doc(&self, doc_type: DocumentType, create: bool) -> BlixardResult<()> {
       let doc = self.endpoint.create_document().await?;
       self.documents.insert(doc_type, doc);
       Ok(())
   }
   ```

2. **Add Key-Value Operations**
   ```rust
   pub async fn write_to_doc(&self, doc_type: DocumentType, key: &str, value: &[u8]) -> BlixardResult<()> {
       let doc = self.documents.get(&doc_type)?;
       doc.set(key.as_bytes(), value).await?;
       Ok(())
   }
   ```

3. **Enable P2pImageStore**
   - Fix initialization failures
   - Test basic image metadata storage

### Deliverables:
- ✅ P2pImageStore initializes successfully
- ✅ Basic document read/write working
- ✅ Unit tests passing for document operations

## Phase 2: Blob Storage & File Sharing (Week 1-2)
**Goal**: Enable actual P2P file transfers

### Tasks:
1. **Implement Blob Storage**
   ```rust
   pub async fn share_file(&self, path: &Path) -> BlixardResult<iroh_blobs::Hash> {
       let blob = self.endpoint.add_from_path(path).await?;
       Ok(blob.hash())
   }
   ```

2. **Add Download Capability**
   ```rust
   pub async fn download_data(&self, hash: &iroh_blobs::Hash) -> BlixardResult<Vec<u8>> {
       let blob = self.endpoint.get_blob(hash).await?;
       Ok(blob.read_to_bytes().await?)
   }
   ```

3. **Progress Tracking**
   - Add transfer progress events
   - Implement bandwidth monitoring

### Deliverables:
- ✅ File sharing returns real hashes
- ✅ Files can be downloaded by hash
- ✅ Transfer progress events working

## Phase 3: Peer Discovery & Connection (Week 2)
**Goal**: Automatic peer discovery and robust connections

### Tasks:
1. **Enable Iroh Discovery**
   ```rust
   // In IrohTransport::new()
   let endpoint = iroh::Endpoint::builder()
       .discovery(iroh_dns_discovery::DnsDiscovery::n0_dns())
       .bind()
       .await?;
   ```

2. **Local Network Discovery**
   - Add mDNS discovery
   - Configure local peer finding

3. **Connection Management**
   - Automatic reconnection
   - Peer health monitoring
   - Connection quality tracking

### Deliverables:
- ✅ Peers discover each other automatically
- ✅ Connections recover from failures
- ✅ Peer list stays up-to-date

## Phase 4: VM Image Distribution (Week 2-3)
**Goal**: Distributed VM image storage and retrieval

### Tasks:
1. **Image Chunking**
   - Split large images into chunks
   - Parallel chunk transfers

2. **Distributed Storage**
   - Replicate images across nodes
   - Smart placement based on usage

3. **Caching Strategy**
   - LRU cache for popular images
   - Pre-fetch based on patterns

### Deliverables:
- ✅ VM images distributed via P2P
- ✅ Fast image retrieval from nearest peer
- ✅ Automatic cleanup of unused images

## Phase 5: Production Hardening (Week 3-4)
**Goal**: Make P2P reliable and performant

### Tasks:
1. **Security**
   - Peer authentication
   - Encrypted transfers
   - Access control lists

2. **Performance**
   - Bandwidth optimization
   - Transfer prioritization
   - Connection pooling

3. **Monitoring**
   - P2P metrics dashboard
   - Transfer analytics
   - Network topology visualization

### Deliverables:
- ✅ Secure peer-to-peer transfers
- ✅ Optimized for production loads
- ✅ Complete observability

## Implementation Order

### Week 1: Foundation
1. Document operations (2 days)
2. Basic blob storage (2 days)
3. Fix P2pImageStore (1 day)

### Week 2: Connectivity
1. Peer discovery (2 days)
2. File download (2 days)
3. Connection management (1 day)

### Week 3: Features
1. Image distribution (3 days)
2. Caching layer (2 days)

### Week 4: Polish
1. Security implementation (2 days)
2. Performance optimization (2 days)
3. Monitoring setup (1 day)

## Success Metrics

- **Week 1**: Document operations working, files can be shared
- **Week 2**: Automatic peer discovery, reliable transfers
- **Week 3**: VM images distributed efficiently
- **Week 4**: Production-ready with security and monitoring

## Risk Mitigation

1. **Iroh API Changes**: Pin to specific version, monitor changelog
2. **Performance Issues**: Start with small files, optimize incrementally
3. **Network Complexity**: Test in isolated environments first
4. **Security Concerns**: Get security review before production

## Next Steps

1. Create `iroh_document_impl.rs` with actual Iroh document calls
2. Write integration tests for each phase
3. Set up performance benchmarks
4. Document P2P protocol for other implementations