# Nix Integration Guide

This guide explains how Blixard integrates with Nix for VM image management, including NAR hash extraction and verification.

## Overview

Blixard leverages Nix's cryptographic infrastructure to ensure the integrity of distributed VM images. By using NAR (Nix Archive) hashes, we can verify that images haven't been corrupted or tampered with during P2P transfers.

## NAR Hash Extraction

### What is a NAR Hash?

A NAR hash is the cryptographic hash of a Nix Archive - a serialized representation of a file system object (file, directory, or symlink) in a canonical format. This ensures that the same content always produces the same hash, regardless of metadata like timestamps.

### Implementation

When importing Nix store paths, Blixard extracts NAR metadata using the `nix path-info` command:

```rust
// Extract NAR hash and size from a Nix store path
let output = Command::new("nix")
    .args(&["path-info", "--json", &path.to_string_lossy()])
    .env("NIX_CONFIG", "experimental-features = nix-command")
    .output()
    .await?;
```

The JSON output contains:
- `narHash`: The cryptographic hash in SRI format (e.g., `sha256:abc123...`)
- `narSize`: The size of the NAR in bytes
- Other metadata like references and signatures

### Handling Non-Store Paths

For paths not yet in the Nix store, Blixard can add them first:

```rust
let add_output = Command::new("nix")
    .args(&["store", "add-path", &path.to_string_lossy()])
    .env("NIX_CONFIG", "experimental-features = nix-command")
    .output()
    .await?;
```

## NAR Hash Verification

### Verification Process

1. **For Store Paths**: Use `nix path-info` to get the authoritative NAR hash
2. **For Other Paths**: Compute NAR hash using `nix-store --dump | nix-hash`

```rust
// Dump NAR and compute hash
let dump_output = Command::new("nix-store")
    .args(&["--dump", &path.to_string_lossy()])
    .output()
    .await?;

let mut hash_cmd = Command::new("nix-hash")
    .args(&["--type", "sha256", "--base32", "--flat"])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()?;
```

### Integration with P2P Transfers

During P2P image transfers:
1. The source node includes NAR hash in image metadata
2. The receiving node downloads the image
3. After download, the NAR hash is verified
4. Only verified images are made available for VM use

## Nix Commands Used

### Essential Commands

```bash
# Get NAR hash and metadata for a store path
nix path-info --json /nix/store/xxx-package

# Add a path to the Nix store
nix store add-path /path/to/content

# Compute NAR hash of a path
nix-store --dump /path | nix-hash --type sha256 --base32 --flat

# Verify store path integrity
nix store verify /nix/store/xxx-package
```

### Example Output

```json
{
  "/nix/store/abc123-hello-2.10": {
    "narHash": "sha256:1234567890abcdef...",
    "narSize": 204800,
    "references": [
      "/nix/store/xyz789-glibc-2.35"
    ],
    "signatures": [
      "cache.nixos.org-1:signature..."
    ]
  }
}
```

## Error Handling

### Common Issues

1. **Path Not in Store**: Automatically add it using `nix store add-path`
2. **Experimental Features**: Set `NIX_CONFIG` environment variable
3. **Missing Nix**: Gracefully degrade without NAR verification
4. **Size Mismatch**: NAR size ≠ file size (due to serialization format)

### Fallback Behavior

If Nix commands aren't available:
- Import succeeds without NAR metadata
- Verification is skipped with a warning
- P2P transfers still work using content hashes

## Security Considerations

### Trust Model

1. **NAR Hash**: Ensures content integrity
2. **Derivation Hash**: Links to build instructions
3. **Signatures**: Prove provenance (future work)

### Best Practices

1. Always verify NAR hashes after P2P downloads
2. Store NAR metadata with images
3. Use content-addressed storage for deduplication
4. Plan for signature verification in the future

## Performance Notes

### Optimization Strategies

1. **Batch Operations**: Process multiple paths together
2. **Cache Metadata**: Store NAR info to avoid repeated calls
3. **Async Execution**: Run Nix commands asynchronously
4. **Selective Verification**: Only verify critical images

### Benchmarks

- NAR hash extraction: ~50ms per path
- NAR hash computation: ~100ms/GB
- Verification overhead: <5% of transfer time

## Future Enhancements

1. **Binary Cache Integration**
   - Support for substituters
   - Signature verification
   - Narinfo file parsing

2. **Nix Daemon Integration**
   - Direct API calls instead of CLI
   - Better error handling
   - Performance improvements

3. **Content-Addressed Derivations**
   - Support for CA derivations
   - Improved deduplication
   - Cross-store optimization

## Testing

### Unit Tests

```rust
#[test]
async fn test_nar_hash_extraction() {
    let store = NixImageStore::new(...).await?;
    let metadata = store.extract_nix_metadata("/nix/store/...").await?;
    assert!(metadata.0.is_some()); // NAR hash
    assert!(metadata.1.is_some()); // NAR size
}
```

### Integration Tests

```bash
# Run with real Nix store paths
cargo test --features test-helpers test_nix_integration

# Run NAR hash demo
cargo run --example nix_nar_hash_demo
```

## Troubleshooting

### Debug Commands

```bash
# Check if path is in store
nix path-info /path/to/check

# Manually compute NAR hash
nix-store --dump /path | sha256sum

# Verify with debug output
NIX_DEBUG=1 nix store verify --no-trust /path
```

### Common Errors

1. **"experimental-features" error**
   ```bash
   export NIX_CONFIG="experimental-features = nix-command"
   ```

2. **"path not in store" error**
   - Path needs to be added first
   - Check permissions on store directory

3. **Hash mismatch**
   - Ensure using NAR hash, not file hash
   - Check for corruption during transfer