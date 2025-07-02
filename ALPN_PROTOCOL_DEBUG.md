# ALPN Protocol Mismatch Debug Investigation

## Issue
The error "peer doesn't support any known protocol" occurs when attempting P2P connections between nodes.

## Root Cause
The Iroh endpoint in `IrohTransportV2` is not configured with any ALPN protocols, while the client is attempting to connect using `BLIXARD_ALPN` (b"blixard/rpc/1").

### Key Findings

1. **ALPN Protocol Definition**: 
   - Defined in `src/transport/mod.rs`: `pub const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";`

2. **Client Usage**:
   - `src/transport/iroh_client.rs` correctly uses BLIXARD_ALPN when connecting:
   ```rust
   let conn = self.endpoint
       .connect(self.node_addr.clone(), crate::transport::BLIXARD_ALPN)
       .await
   ```

3. **Server Configuration Issue**:
   - `IrohTransportV2::new_with_monitor()` creates endpoint without ALPN configuration:
   ```rust
   let endpoint = builder
       .bind()
       .await
   ```
   - Missing: `.alpns(vec![BLIXARD_ALPN.to_vec()])`

4. **Correct Configuration** (from test examples):
   ```rust
   let endpoint = Endpoint::builder()
       .secret_key(secret)
       .alpns(vec![BLIXARD_ALPN.to_vec()])  // This is missing!
       .bind()
       .await?;
   ```

## Solution
The `IrohTransportV2` needs to configure the endpoint with the BLIXARD_ALPN protocol to accept RPC connections.

## References
- Iroh ALPN documentation: https://docs.rs/iroh/latest/iroh/endpoint/struct.Builder.html#method.alpns
- Test example: `examples/test_alpn_with_accept.rs`