# IrohTransport to IrohTransportV2 Migration Plan

## Overview
This document outlines the migration from `IrohTransport` to `IrohTransportV2` in the Blixard codebase.

## Key API Differences

### 1. Constructor and Type Name
- **Old**: `IrohTransport::new(node_id, data_dir)`
- **New**: `IrohTransportV2::new(node_id, data_dir)`

### 2. ALPN Configuration
- **IrohTransport**: Explicitly configures ALPNs for each DocumentType
- **IrohTransportV2**: No ALPN configuration in builder (simpler initialization)

### 3. Connection Management
- **IrohTransport**: Has `connections` HashMap and `connect_to_peer()` method
- **IrohTransportV2**: No explicit connection management (removed complexity)

### 4. Document Operations
- **IrohTransport**: Stub implementations returning `NotImplemented` errors
- **IrohTransportV2**: Actual in-memory implementations with working storage

### 5. File Operations
- **IrohTransport**: Dummy hash generation (all zeros)
- **IrohTransportV2**: Proper blake3 hash calculation

### 6. Data Transfer
- **IrohTransport**: Has `send_to_peer()` method for direct peer communication
- **IrohTransportV2**: No direct send method (uses document-based communication)

## Files Requiring Updates

### 1. Source Files

#### `/src/p2p_manager.rs`
- **Import**: `use crate::iroh_transport::{IrohTransport, DocumentType};`
- **Field**: `transport: IrohTransport`
- **Update**: Change import and field type to `IrohTransportV2`

#### `/src/cluster_state.rs`
- **Import**: `use crate::iroh_transport::{IrohTransport, DocumentType};`
- **Field**: `transport: Arc<IrohTransport>`
- **Update**: Change import and field type to `IrohTransportV2`

#### `/src/p2p_image_store.rs`
- **Import**: `use crate::iroh_transport::{IrohTransport, DocumentType};`
- **Field**: `transport: IrohTransport`
- **Update**: Change import and field type to `IrohTransportV2`

### 2. Test Files

#### `/tests/p2p_simple_test.rs`
- **Import**: `use blixard_core::{iroh_transport::{IrohTransport, DocumentType}, ...}`
- **Usage**: Creates `IrohTransport` instances and uses `send_to_peer()`
- **Update**: 
  - Change import to `iroh_transport_v2::{IrohTransportV2, DocumentType}`
  - Remove or rewrite `send_to_peer()` test (method doesn't exist in V2)

#### `/tests/p2p_integration_test.rs`
- **Import**: `use blixard_core::{iroh_transport::{IrohTransport, DocumentType}, ...}`
- **Usage**: Creates `IrohTransport` instances
- **Update**: Change import and constructor calls to `IrohTransportV2`

### 3. Module Declaration

#### `/src/lib.rs`
- Keep both modules for now during migration:
  ```rust
  pub mod iroh_transport;      // Can be removed after migration
  pub mod iroh_transport_v2;   // The new module
  ```

### 4. Other Module Conflicts

#### `/src/transport/mod.rs`
- **Issue**: Defines a different `IrohTransport` struct (lines 37-58)
- **Resolution**: This appears to be a different transport layer and won't conflict
- **Note**: No changes needed here

## Migration Steps

### Phase 1: Update Imports and Types
1. Update all imports from `iroh_transport` to `iroh_transport_v2`
2. Change all type references from `IrohTransport` to `IrohTransportV2`
3. Keep `DocumentType` import (same in both modules)

### Phase 2: Update Test Code
1. Fix `test_basic_p2p_initialization` in `p2p_simple_test.rs`:
   - Remove or rewrite the `send_to_peer()` test
   - Consider testing document operations instead
2. Update constructor calls in all test files

### Phase 3: Verify Functionality
1. Run all P2P tests to ensure they compile and pass
2. Check that document operations now work (were stubs before)
3. Verify file hashing produces proper blake3 hashes

### Phase 4: Cleanup
1. Remove old `iroh_transport` module from `lib.rs`
2. Delete `src/iroh_transport.rs` file
3. Update any documentation references

## Code Changes Required

### Example: p2p_manager.rs
```rust
// Old
use crate::iroh_transport::{IrohTransport, DocumentType};
// ...
transport: IrohTransport,

// New
use crate::iroh_transport_v2::{IrohTransportV2, DocumentType};
// ...
transport: IrohTransportV2,
```

### Example: Test Updates
```rust
// Old test that won't work with V2
match transport1.send_to_peer(&addr2, DocumentType::ClusterConfig, &test_data).await {
    // ...
}

// New approach - use document operations
transport1.create_or_join_doc(DocumentType::ClusterConfig, true).await?;
transport1.write_to_doc(DocumentType::ClusterConfig, "test_key", &test_data).await?;
```

## Benefits of Migration

1. **Working Document Operations**: V2 has actual implementations instead of stubs
2. **Proper File Hashing**: Blake3 hashing instead of dummy zeros
3. **Simpler API**: No manual connection management or ALPN configuration
4. **Better Testing**: Document operations can actually be tested

## Risks and Considerations

1. **Missing send_to_peer()**: Direct peer communication method is removed
   - Mitigation: Use document-based communication pattern instead
2. **No Connection Pooling**: V2 doesn't manage connections explicitly
   - Impact: May affect performance for frequent peer communications
3. **In-Memory Storage**: V2 uses in-memory document storage
   - Consideration: Data is not persisted across restarts

## Testing Plan

1. Update all test imports and verify compilation
2. Rewrite tests that use removed methods (send_to_peer)
3. Add new tests for document operations (now functional)
4. Integration test with P2pManager and P2pImageStore
5. Verify cluster state export/import functionality