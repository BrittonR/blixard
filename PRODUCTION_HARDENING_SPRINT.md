# Production Hardening Sprint Plan

## 🎯 Goal: Eliminate Panic Risk in Production

Transform Blixard from a working prototype to a production-grade system by systematically removing all panic-inducing code patterns.

## Current State: 192 Files with unwrap() Calls

### Top 10 Worst Offenders (Fix These First)

1. **iroh_transport_v3.rs** - 34 unwrap() calls
   - Risk: Core networking layer crashes
   - Priority: CRITICAL
   
2. **security.rs** - 13 unwrap() calls  
   - Risk: Security failures cause panics
   - Priority: CRITICAL

3. **nix_image_store.rs** - 13 unwrap() calls
   - Risk: VM image operations fail catastrophically
   - Priority: HIGH

4. **audit_log.rs** - 10 unwrap() calls
   - Risk: Compliance logging crashes system
   - Priority: HIGH

5. **observability.rs** - 9 unwrap() calls
   - Risk: Metrics collection causes failures
   - Priority: MEDIUM

6. **cedar_authz.rs** - 8 unwrap() calls
   - Risk: Authorization failures panic
   - Priority: HIGH

7. **node.rs** - 8 unwrap() calls
   - Risk: Core node operations fail
   - Priority: CRITICAL

8. **iroh_transport_v2.rs** - 7 unwrap() calls
   - Risk: P2P operations crash
   - Priority: CRITICAL

9. **vm_manager.rs** - 7 unwrap() calls
   - Risk: VM lifecycle failures
   - Priority: HIGH

10. **storage.rs** - 6 unwrap() calls
    - Risk: Data persistence failures
    - Priority: HIGH

## Replacement Patterns

### Pattern 1: Error Propagation with ?
```rust
// ❌ BAD
let config = fs::read_to_string(path).unwrap();

// ✅ GOOD
let config = fs::read_to_string(path)
    .map_err(|e| BlixardError::IoError(e))?;
```

### Pattern 2: Default Values
```rust
// ❌ BAD
let port = env::var("PORT").unwrap().parse().unwrap();

// ✅ GOOD
let port = env::var("PORT")
    .unwrap_or_else(|_| "8080".to_string())
    .parse()
    .unwrap_or(8080);
```

### Pattern 3: Early Returns
```rust
// ❌ BAD
let socket = TcpListener::bind(addr).unwrap();

// ✅ GOOD
let socket = match TcpListener::bind(addr) {
    Ok(s) => s,
    Err(e) => {
        error!("Failed to bind to {}: {}", addr, e);
        return Err(BlixardError::NetworkError(e.to_string()));
    }
};
```

### Pattern 4: Graceful Degradation
```rust
// ❌ BAD
let metrics = prometheus::default_registry().unwrap();

// ✅ GOOD
let metrics = prometheus::default_registry()
    .unwrap_or_else(|_| {
        warn!("Metrics registry unavailable, using no-op");
        &NO_OP_REGISTRY
    });
```

## Sprint Schedule

### Day 1-2: Critical Infrastructure
- [ ] iroh_transport_v3.rs (34 unwraps)
- [ ] node.rs (8 unwraps)
- [ ] iroh_transport_v2.rs (7 unwraps)

### Day 3-4: Security & Data
- [ ] security.rs (13 unwraps)
- [ ] cedar_authz.rs (8 unwraps)
- [ ] storage.rs (6 unwraps)

### Day 5-6: VM Operations
- [ ] nix_image_store.rs (13 unwraps)
- [ ] vm_manager.rs (7 unwraps)
- [ ] vm_scheduler.rs (5 unwraps)

### Day 7-8: Supporting Systems
- [ ] audit_log.rs (10 unwraps)
- [ ] observability.rs (9 unwraps)
- [ ] metrics_otel_v2.rs (5 unwraps)

### Day 9-10: Sweep Remaining Files
- [ ] Fix all files with 3+ unwraps
- [ ] Document patterns for future contributors

## Automation Tools

### 1. Find All unwrap() Calls
```bash
# Count by file
rg "\.unwrap\(\)" --type rust -c | sort -t: -k2 -nr | head -20

# Show context
rg "\.unwrap\(\)" --type rust -B2 -A2
```

### 2. Automated Replacement Script
```bash
#!/bin/bash
# replace_unwrap.sh
cargo clippy --fix -- -W clippy::unwrap_used
cargo fmt
```

### 3. CI Check
```yaml
# .github/workflows/no-unwrap.yml
- name: Check for unwrap()
  run: |
    count=$(rg "\.unwrap\(\)" --type rust -c | wc -l)
    if [ $count -gt 0 ]; then
      echo "Found $count files with unwrap() calls"
      exit 1
    fi
```

## Success Criteria

1. **Zero unwrap() in critical paths**
   - Network handling
   - Storage operations
   - Security checks

2. **Graceful error messages**
   - User-friendly errors
   - Actionable suggestions
   - Proper error codes

3. **No performance regression**
   - Benchmark before/after
   - Monitor memory usage
   - Check latency impact

## Error Handling Guidelines

### DO ✅
- Use `?` for error propagation
- Add context with `.context()` or `.with_context()`
- Log errors at appropriate levels
- Provide recovery mechanisms
- Test error paths

### DON'T ❌
- Use `.unwrap()` in production code
- Panic on recoverable errors
- Ignore errors silently
- Use `.expect()` without clear messages
- Chain multiple `.unwrap()` calls

## Testing Strategy

For each file fixed:
1. Add error injection tests
2. Verify error messages are helpful
3. Check error propagation works
4. Test recovery mechanisms
5. Benchmark performance impact

## Tracking Progress

### Day 1 Progress ✅
- [x] iroh_transport_v2.rs: 1/1 production unwrap fixed (was in BandwidthTracker)
- [x] transport/iroh_service.rs: 1/1 unwrap fixed 
- [x] transport/iroh_raft_transport.rs: 4/4 unwraps fixed
- [x] nix_image_store.rs: 6/6 unwraps fixed
- [x] VM subsystem: Verified all are safe unwrap_or_else() calls

**Total Day 1: 12 production unwraps eliminated!**

### Remaining Work
- [ ] Check remaining files for production unwraps
- [ ] Add CI check to prevent new unwraps
- [ ] Document error handling patterns

## Post-Sprint Actions

1. **Add linting rules** to prevent unwrap() 
2. **Update contribution guidelines**
3. **Create error handling best practices doc**
4. **Set up monitoring for panics**
5. **Plan regular audits**

---

Remember: Every unwrap() removed is one less 3am incident!