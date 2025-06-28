# Tonic-Iroh-Transport Integration Issues

## Version Compatibility Issue

We discovered that `tonic-iroh-transport` v0.0.3 requires tonic v0.13, but our project was using tonic v0.12. This caused compilation errors.

### Actions Taken:
1. Updated `tonic` from 0.12 to 0.13
2. Updated `tonic-build` from 0.12 to 0.13
3. Added `dashmap = "6.1"` dependency

## API Differences from Documentation

The actual API of tonic-iroh-transport differs from what was shown in the crates.io documentation:

### 1. GrpcProtocolHandler::for_service()
- **Expected**: Takes a service as parameter
- **Actual**: Takes no parameters (0 arguments)

### 2. IrohPeerInfo
- **Expected**: Available as an export from the crate
- **Actual**: Not exported or doesn't exist

### 3. API Design
The library appears to have a different API structure than documented. The examples on crates.io may be outdated or incorrect.

## Current Status

Due to these API differences, we cannot directly use tonic-iroh-transport as initially planned. We have several options:

### Option 1: Figure Out Correct API
- Study the source code of tonic-iroh-transport
- Contact the library maintainers for correct usage examples
- Update our implementation to match the actual API

### Option 2: Revert to Custom Implementation
- Remove tonic-iroh-transport dependency
- Continue with our custom iroh_grpc_bridge implementation
- Implement our own protocol handling

### Option 3: Use Lower-Level Iroh APIs
- Use iroh directly without the tonic-iroh-transport wrapper
- Implement our own gRPC-over-QUIC bridge
- More work but full control over the implementation

## Recommendation

Given the API mismatch and potential instability of a v0.0.3 library, I recommend **Option 2 or 3** - continuing with our custom implementation. This gives us:

1. Full control over the protocol
2. No dependency on an unstable external library
3. Ability to optimize for our specific use case
4. Better understanding of the implementation

## Next Steps

1. Revert the tonic-iroh-transport integration
2. Continue with our custom iroh_grpc_bridge implementation
3. Focus on getting basic P2P communication working
4. Add gRPC compatibility layer if needed

The library looked promising but appears to be too early in development for production use.