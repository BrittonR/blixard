# P2P Initialization Fix

## Problem
Nodes were failing to start with P2P enabled due to `NotImplemented` error from `create_or_join_doc()` call in `P2pImageStore::new()`.

## Root Cause
The `create_or_join_doc()` method in `IrohTransport` was attempting to use Iroh document operations that aren't fully implemented yet. While the main implementation returns `Ok(())`, there was also a stub file that was returning errors.

## Solution Applied

1. **Commented out the problematic call** in `p2p_image_store.rs:61`:
   ```rust
   // TODO: Re-enable when Iroh document API is available
   // transport.create_or_join_doc(DocumentType::VmImages, true).await?;
   ```

2. **Removed unused stub file** `iroh_transport_stub.rs` to avoid confusion

3. **Updated stub methods** (before removal) to match the real implementation signatures

## Result
- Nodes can now start successfully with P2P enabled
- P2P manager initializes without errors
- Iroh transport creates endpoints successfully
- Basic P2P infrastructure is ready for connectivity testing

## Next Steps
1. Test P2P connectivity between nodes
2. Implement P2P address exchange in cluster join/leave messages
3. Wire up IrohPeerConnector for actual P2P connections
4. Re-enable document operations when Iroh API is available

## Testing
Use `test_p2p_startup.sh` to verify nodes can start with P2P enabled.