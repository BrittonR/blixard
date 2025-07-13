# Blixard Error Handling Macro Analysis

## Summary

Analyzed 476 `map_err` calls across the Blixard codebase to identify patterns that could benefit from error handling macros.

## Most Common Error Patterns

### 1. Internal Errors (134 occurrences - 28%)
```rust
.map_err(|e| BlixardError::Internal {
    message: format!("Failed to {}: {}", operation, e),
})?
```

### 2. Storage Errors (77 occurrences - 16%)
```rust
.map_err(|e| BlixardError::Storage {
    operation: "save VM config".to_string(),
    source: Box::new(e),
})?
```

### 3. Serialization Errors (41 occurrences - 9%)
```rust
.map_err(|e| BlixardError::Serialization {
    operation: "serialize worker capabilities".to_string(),
    source: Box::new(e),
})?
```

### 4. Security Errors (27 occurrences - 6%)
```rust
.map_err(|e| BlixardError::Security {
    context: "certificate generation".to_string(),
    details: e.to_string(),
})?
```

### 5. Lock Poisoned Errors (24 occurrences - 5%)
```rust
.map_err(|_| BlixardError::lock_poisoned_internal("resource name"))?
```

### 6. IO Errors (19 occurrences - 4%)
```rust
.map_err(|e| BlixardError::IoError(Box::new(e)))?
```

### 7. Raft Storage Errors (15 occurrences - 3%)
```rust
.map_err(|e| raft::Error::Store(raft::StorageError::Other(Box::new(e))))?
```

### 8. Network/P2P Errors with format! (20+ occurrences)
```rust
.map_err(|e| BlixardError::NetworkError(format!("Failed to connect: {}", e)))?
.map_err(|e| BlixardError::P2PError(format!("Accept failed: {}", e)))?
```

### 9. Config Errors with format! (10+ occurrences)
```rust
.map_err(|e| BlixardError::ConfigError(format!("Invalid log level '{}': {}", level, e)))?
```

## Existing Infrastructure

The codebase already has:
1. **Helper methods** on `BlixardError` for common patterns:
   - `storage()`, `raft()`, `serialization()`, `lock_poisoned()`, `database()`
2. **Existing macros** in `unwrap_helpers.rs`:
   - `serialize_or_error!`, `deserialize_or_error!`, `acquire_lock!`
3. **Context traits** in `patterns/error_context.rs` for adding context

## Recommended Macro Designs

### 1. `map_internal!` - For Internal errors (134 uses)
```rust
#[macro_export]
macro_rules! map_internal {
    // Simple message
    ($expr:expr, $msg:expr) => {
        $expr.map_err(|e| BlixardError::Internal {
            message: format!("{}: {}", $msg, e),
        })?
    };
    // Format string with args
    ($expr:expr, $fmt:expr, $($arg:tt)*) => {
        $expr.map_err(|e| BlixardError::Internal {
            message: format!(concat!($fmt, ": {}"), $($arg)*, e),
        })?
    };
}

// Usage:
map_internal!(fs::read_to_string(path), "Failed to read config file");
map_internal!(operation(), "Failed to {} for VM {}", action, vm_name);
```

### 2. `map_storage!` - For Storage errors (77 uses)
```rust
#[macro_export]
macro_rules! map_storage {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::Storage {
            operation: $operation.to_string(),
            source: Box::new(e),
        })?
    };
}

// Usage:
map_storage!(db.begin_write(), "begin transaction");
```

### 3. `map_io!` - For IO errors (19 uses)
```rust
#[macro_export]
macro_rules! map_io {
    ($expr:expr) => {
        $expr.map_err(|e| BlixardError::IoError(Box::new(e)))?
    };
}

// Usage:
map_io!(file.write_all(&data));
```

### 4. `map_security!` - For Security errors (27 uses)
```rust
#[macro_export]
macro_rules! map_security {
    ($expr:expr, $context:expr) => {
        $expr.map_err(|e| BlixardError::Security {
            context: $context.to_string(),
            details: e.to_string(),
        })?
    };
}

// Usage:
map_security!(Certificate::from_params(params), "certificate generation");
```

### 5. `map_network!` - For Network/P2P errors with formatting
```rust
#[macro_export]
macro_rules! map_network {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::NetworkError(
            format!("{}: {}", $operation, e)
        ))?
    };
}

#[macro_export]
macro_rules! map_p2p {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::P2PError(
            format!("{}: {}", $operation, e)
        ))?
    };
}

// Usage:
map_network!(stream.connect().await, "Failed to connect");
map_p2p!(endpoint.accept().await, "Accept failed");
```

### 6. `map_config!` - For Config errors with formatting
```rust
#[macro_export]
macro_rules! map_config {
    ($expr:expr, $fmt:expr, $($arg:tt)*) => {
        $expr.map_err(|e| BlixardError::ConfigError(
            format!(concat!($fmt, ": {}"), $($arg)*, e)
        ))?
    };
}

// Usage:
map_config!(level.parse(), "Invalid log level '{}'", level);
```

### 7. `map_raft_storage!` - For Raft storage errors (15 uses)
```rust
#[macro_export]
macro_rules! map_raft_storage {
    ($expr:expr) => {
        $expr.map_err(|e| raft::Error::Store(
            raft::StorageError::Other(Box::new(e))
        ))?
    };
}

// Usage:
map_raft_storage!(txn.commit());
```

## Implementation Priority

Based on frequency and boilerplate reduction:

1. **High Priority** (200+ total uses):
   - `map_internal!` (134 uses)
   - `map_storage!` (77 uses)

2. **Medium Priority** (40-60 total uses):
   - `map_security!` (27 uses)
   - `map_network!` + `map_p2p!` (20+ uses)
   - `map_io!` (19 uses)

3. **Low Priority** (< 20 uses):
   - `map_config!` (10+ uses)
   - `map_raft_storage!` (15 uses)

## Benefits

1. **Consistency**: Standardized error handling patterns
2. **Less Boilerplate**: ~50% reduction in error handling code
3. **Maintainability**: Central place to update error handling logic
4. **Type Safety**: Compile-time checks for error construction
5. **Documentation**: Macro names self-document the error type

## Migration Strategy

1. Add macros to `unwrap_helpers.rs` or create new `error_macros.rs`
2. Start with high-priority macros (`map_internal!`, `map_storage!`)
3. Gradually migrate existing code using search-and-replace
4. Update coding guidelines to prefer macros for new code
5. Add clippy lints to enforce macro usage

## Example Migration

Before:
```rust
let data = fs::read_to_string(&path)
    .map_err(|e| BlixardError::Internal {
        message: format!("Failed to read config file: {}", e),
    })?;

let txn = db.begin_write()
    .map_err(|e| BlixardError::Storage {
        operation: "begin transaction".to_string(),
        source: Box::new(e),
    })?;
```

After:
```rust
let data = map_internal!(fs::read_to_string(&path), "Failed to read config file");
let txn = map_storage!(db.begin_write(), "begin transaction");
```

## Estimated Impact

- **Lines saved**: ~1,900 lines (4 lines → 1 line per usage)
- **Character reduction**: ~60% reduction in error handling code
- **Readability**: Significant improvement in code clarity