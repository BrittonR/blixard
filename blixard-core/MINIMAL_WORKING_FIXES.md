# Minimal Fixes to Get Blixard Working

## Current State
- The TransportConfig enum->struct migration is complete
- Iroh transport infrastructure is in place
- Many compilation errors remain in various subsystems

## Minimal Path Forward

### Option 1: Disable Broken Features (Fastest)
Create a minimal build that only includes core functionality:

1. **Disable features in Cargo.toml**:
   - Remove backup_manager from the build
   - Remove audit_log from the build
   - Remove config_hot_reload from the build
   - Focus on core node, transport, and VM functionality

2. **Comment out non-essential initializations in node.rs**:
   - Backup manager
   - Audit logging
   - Hot reload functionality

3. **Fix critical errors only**:
   - NotFound error usage
   - ConfigurationError -> use Internal error instead
   - RaftState type issues

### Option 2: Fix All Errors (More Complete)
Systematically fix each error type:

1. **Error enum mismatches**:
   - Replace `NotFound { entity, id }` with `NotFound { resource: format!("{} {}", entity, id) }`
   - Replace `ConfigurationError` with `ConfigError` or `Internal`

2. **Missing trait imports**:
   - Add `use raft::Storage;` where needed
   - Import missing traits for HashMap entry API

3. **Type mismatches**:
   - Fix RaftState usage (it's not Option-like)
   - Fix function argument counts

## Recommended Approach

For immediate testing, go with Option 1:

1. Create a minimal feature set in Cargo.toml:
```toml
[features]
default = ["iroh-transport"]
iroh-transport = []
full = ["backup", "audit", "hot-reload"]
backup = []
audit = []
hot-reload = []
```

2. Use conditional compilation to exclude broken modules:
```rust
#[cfg(feature = "backup")]
mod backup_manager;

#[cfg(feature = "audit")]
mod audit_log;

#[cfg(feature = "hot-reload")]
mod config_hot_reload;
```

3. Update node initialization to skip disabled features:
```rust
#[cfg(feature = "backup")]
{
    // Initialize backup manager
}
```

This will allow us to:
- Get a node running with Iroh transport
- Test basic cluster formation
- Verify VM operations work
- Fix other features incrementally

## Next Steps After Minimal Build

1. Test single node startup
2. Test multi-node cluster formation
3. Test VM creation/management
4. Gradually re-enable and fix each feature

## Alternative: Create a test binary

Create a minimal test binary that just uses the working parts:
- `examples/minimal_node.rs` - Just starts a node with Iroh
- `examples/test_cluster.rs` - Forms a 3-node cluster
- Skip all the fancy features until core works