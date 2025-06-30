# Cedar Authorization for Iroh Transport

## Current Status

Cedar authorization is currently **NOT integrated** with Iroh transport services. The Phase 3-4 implementation only covered gRPC services.

## Why Cedar Doesn't Work with Iroh Yet

1. **Different Service Architecture**:
   - gRPC services use `GrpcMiddleware` which has Cedar integration
   - Iroh services implement `IrohService` trait directly without middleware layer
   - No authentication/authorization framework exists for Iroh transport

2. **Direct Service Calls**:
   ```rust
   // Current Iroh implementation bypasses all security
   async fn handle_vm_operation(&self, request: VmOperationRequest) -> BlixardResult<VmOperationResponse> {
       match request {
           VmOperationRequest::Create { name, vcpus, memory_mb } => {
               // Direct call - no auth check!
               match self.vm_service.create_vm(name.clone(), vcpus, memory_mb).await {
   ```

3. **Missing Components**:
   - No token extraction from Iroh connections
   - No security context propagation
   - No access to SecurityManager from Iroh services

## Integration Plan for Iroh

### Option 1: Iroh Middleware Layer (Recommended)
Create an `IrohMiddleware` similar to `GrpcMiddleware`:

```rust
pub struct IrohMiddleware {
    security_manager: Option<Arc<SecurityManager>>,
    quota_manager: Option<Arc<QuotaManager>>,
}

impl IrohMiddleware {
    pub async fn authenticate_and_authorize_cedar(
        &self,
        // Extract auth from Iroh connection metadata
        connection: &Connection,
        action: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> BlixardResult<(SecurityContext, String)> {
        // Similar to gRPC implementation
    }
}
```

### Option 2: Service Wrapper Pattern
Wrap the base services with security-aware versions:

```rust
pub struct SecureIrohVmService {
    inner: IrohVmService,
    middleware: IrohMiddleware,
}

impl SecureIrohVmService {
    async fn handle_call(&self, method: &str, payload: Bytes) -> BlixardResult<Bytes> {
        // Check authorization before delegating to inner service
        match method {
            "create_vm" => {
                // Extract request and check Cedar
                self.middleware.authenticate_and_authorize_cedar(...)
            }
        }
    }
}
```

### Option 3: Protocol-Level Security
Add authentication to the Iroh protocol itself:

```rust
// In iroh_protocol.rs
pub struct AuthenticatedRpcRequest {
    pub auth_token: String,
    pub request: RpcRequest,
}
```

## Implementation Steps

1. **Define Iroh Authentication**:
   - How to pass auth tokens over Iroh connections
   - Whether to use connection-level or request-level auth

2. **Create IrohMiddleware**:
   - Port `GrpcMiddleware` functionality to Iroh
   - Handle Cedar authorization checks

3. **Update Service Implementations**:
   - Add middleware to all Iroh services
   - Ensure consistent authorization across transports

4. **Test Dual Transport Auth**:
   - Verify same authorization decisions on both transports
   - Test Cedar policies work identically

## Current Workaround

If you need Cedar authorization NOW with Iroh:
- Use gRPC for security-sensitive operations
- Use Iroh only for non-critical operations
- Configure transport selection based on security requirements

## Future Considerations

- P2P authentication is complex - may need peer identity verification
- Iroh's decentralized nature requires different trust models
- Consider using Iroh's built-in identity system for authentication