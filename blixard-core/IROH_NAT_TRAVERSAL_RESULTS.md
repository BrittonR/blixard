# Iroh NAT Traversal Test Results

## Summary

Created and tested NAT traversal capabilities with Iroh P2P transport.

## Test Implementation

- **File**: `examples/iroh_nat_traversal_test.rs`
- **Features**: 
  - Two-node connectivity test
  - Configurable modes (direct + relay vs NAT-only)
  - Bidirectional communication testing
  - Retry logic with timeout handling

## Results

### 1. Direct + Relay Mode (Default)
- **Status**: ✅ WORKING
- **Description**: When direct addresses are included in NodeAddr
- **Connection Time**: Immediate
- **Bidirectional**: Yes, both directions work
- **Use Case**: LAN environments, same network, or when ports are accessible

### 2. NAT-Only Mode (NAT_ONLY=1)
- **Status**: ❌ NOT WORKING
- **Description**: When only NodeId and relay URL are provided
- **Issue**: Connection timeouts after 10 seconds per attempt
- **Error**: Timeouts suggest relay discovery not completing

## Key Findings

1. **Direct connectivity works excellently** - Sub-second connections when addresses are known
2. **Relay configuration appears correct** - Using default relay.iroh.network
3. **NAT traversal needs investigation** - Pure relay mode not establishing connections

## Possible Issues with NAT-Only Mode

1. **Relay URL format** - Currently using "https://relay.iroh.network/" but may need different format
2. **Discovery time** - May need longer timeout for initial relay discovery
3. **Missing configuration** - Endpoints might need additional relay configuration
4. **Network restrictions** - Local testing environment may not simulate NAT correctly

## Next Steps

1. Research Iroh relay server requirements and configuration
2. Check Iroh documentation for NAT traversal examples
3. Test with actual NAT environments (different networks)
4. Consider using iroh-net examples as reference
5. Investigate relay discovery mechanisms

## Usage

```bash
# Test with direct addresses (works)
cargo run --example iroh_nat_traversal_test

# Test NAT-only mode (currently not working)
NAT_ONLY=1 cargo run --example iroh_nat_traversal_test
```

## Technical Notes

- Both endpoints configured with `RelayMode::Default`
- ALPN protocol set to `b"blixard/1"`
- Using Iroh v0.28 endpoint API
- Message protocol working correctly when connected