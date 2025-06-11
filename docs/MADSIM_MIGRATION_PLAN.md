# MadSim Migration Plan

## Current Status

We have successfully integrated MadSim into the project and verified that it works for deterministic simulation testing. The project currently has:

1. **Custom Runtime Abstraction Layer**: A complete runtime abstraction (`runtime_traits.rs`, `runtime/simulation.rs`) that allows switching between real and simulated execution.

2. **MadSim Integration**: Basic MadSim support has been added:
   - Added to `Cargo.toml` with the `simulation` feature
   - Created `raft_node_madsim.rs` as a proof-of-concept
   - Working examples in `examples/madsim_demo.rs`
   - Test cases in `tests/madsim_test.rs` and `tests/raft_madsim_test.rs`

## Key Findings

1. **MadSim vs Tokio Conflict**: MadSim is designed to completely replace tokio in simulation mode. It provides its own implementations of:
   - `tokio` → `madsim-tokio`
   - `tonic` → `madsim-tonic`
   - Network APIs that support fault injection

2. **Deterministic Features**: MadSim provides:
   - Deterministic time control
   - Deterministic task scheduling
   - Network simulation with partition/latency injection
   - Reproducible execution with seeds

## Migration Options

### Option 1: Keep Current Runtime Abstraction (Recommended Short-term)
- **Pros**: 
  - Already working and tested
  - No breaking changes needed
  - Can gradually adopt MadSim features
- **Cons**: 
  - Maintains custom code
  - Doesn't leverage full MadSim ecosystem

### Option 2: Full MadSim Migration (Recommended Long-term)
- **Pros**:
  - Leverage MadSim's ecosystem (madsim-tonic, etc.)
  - Less custom code to maintain
  - Better integration with other MadSim-based projects
- **Cons**:
  - Requires significant refactoring
  - Need to handle tokio replacement carefully

## Recommended Migration Path

### Phase 1: Current State ✅
- Keep the existing runtime abstraction layer
- Use MadSim for specific deterministic tests
- Both approaches coexist

### Phase 2: Gradual Adoption
1. Create feature flags to switch between tokio and madsim-tokio:
   ```toml
   [dependencies]
   tokio = { version = "1.35", features = ["full"], optional = true }
   madsim-tokio = { version = "0.2", optional = true }
   
   [features]
   default = ["tokio-runtime"]
   tokio-runtime = ["tokio"]
   madsim-runtime = ["madsim-tokio", "madsim"]
   ```

2. Use conditional compilation:
   ```rust
   #[cfg(feature = "madsim-runtime")]
   use madsim_tokio as tokio;
   
   #[cfg(feature = "tokio-runtime")]
   use tokio;
   ```

### Phase 3: Full Migration
1. Replace all tokio imports with conditional imports
2. Replace tonic with madsim-tonic in simulation mode
3. Remove custom runtime abstraction layer
4. Update all tests to use MadSim's test macros

## Implementation Checklist

- [x] Add MadSim to dependencies
- [x] Create basic MadSim examples
- [x] Verify MadSim works with simple tests
- [ ] Create feature flags for runtime selection
- [ ] Update imports to be conditional
- [ ] Migrate network code to use MadSim's network simulation
- [ ] Update all simulation tests to use MadSim
- [ ] Remove custom runtime abstraction (after full migration)

## Example: Conditional Compilation Pattern

```rust
// In lib.rs or a common module
#[cfg(feature = "madsim-runtime")]
pub use madsim_tokio as tokio;
#[cfg(feature = "madsim-runtime")]
pub use madsim_tonic as tonic;

#[cfg(not(feature = "madsim-runtime"))]
pub use tokio;
#[cfg(not(feature = "madsim-runtime"))]
pub use tonic;
```

## Testing Strategy

1. **Unit Tests**: Continue using standard tokio
2. **Integration Tests**: Use feature flags to run with both runtimes
3. **Simulation Tests**: Exclusively use MadSim
4. **CI/CD**: Run tests with both feature sets

## Benefits of Migration

1. **Ecosystem Compatibility**: MadSim is used by major projects like RisingWave
2. **Less Maintenance**: No need to maintain custom runtime abstraction
3. **Better Testing**: More sophisticated network simulation capabilities
4. **Community Support**: Active development and community

## Risks and Mitigations

1. **Risk**: Breaking existing functionality
   - **Mitigation**: Use feature flags for gradual migration

2. **Risk**: Performance differences between runtimes
   - **Mitigation**: Benchmark both implementations

3. **Risk**: Missing features in MadSim
   - **Mitigation**: Contribute back to MadSim or maintain minimal custom code

## Conclusion

The current runtime abstraction layer is working well and provides the deterministic testing capabilities needed. MadSim offers a more comprehensive solution with better ecosystem support. A gradual migration using feature flags is the safest approach, allowing us to:

1. Keep the system stable
2. Gradually adopt MadSim features
3. Maintain backward compatibility
4. Eventually simplify the codebase

The migration is not urgent but would be beneficial for long-term maintenance and ecosystem compatibility.