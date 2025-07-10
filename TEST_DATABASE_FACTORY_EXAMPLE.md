# Test Database Factory Implementation Example

This demonstrates how the new test database factory can replace the repeated setup patterns found throughout the Blixard test suite.

## Pattern Analysis Summary

### Before (84+ occurrences):
```rust
// Repeated across 29+ test files
let temp_dir = TempDir::new().unwrap();
let db_path = temp_dir.path().join("test.db");
let database = Arc::new(Database::create(&db_path).unwrap());

// Sometimes with manual table initialization:
init_database_tables(&database)?;

// Or manual table creation:
{
    let write_txn = db.begin_write().unwrap();
    write_txn.open_table(WORKER_TABLE).unwrap();
    write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    write_txn.open_table(VM_STATE_TABLE).unwrap();
    write_txn.commit().unwrap();
}
```

### After (using factory):
```rust
use blixard_core::test_helpers::{TestDatabaseFactory, TestWorkerFactory, TestVmFactory};

// Basic database with all tables initialized
let (database, _temp_dir) = TestDatabaseFactory::create()?;

// Or with custom name for debugging
let (database, _temp_dir) = TestDatabaseFactory::create_with_name("scheduler_test")?;

// Worker registration made simple
TestWorkerFactory::register_worker(&database, 1, TestWorkerFactory::standard_capabilities(), true)?;

// VM creation and storage
let vm_state = TestVmFactory::create_vm_state("test-vm", 1);
TestVmFactory::store_vm_state(&database, &vm_state)?;
```

## Examples of Pattern Refactoring

### 1. VM Scheduler Tests (vm_scheduler_tests.rs)
**Before** (repeated 10+ times):
```rust
fn create_test_database() -> BlixardResult<(Arc<Database>, TempDir)> {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(&db_path)?);
    init_database_tables(&database)?;
    Ok((database, temp_dir))
}

fn add_worker(database: &Database, node_id: u64, capabilities: WorkerCapabilities, is_online: bool) -> BlixardResult<()> {
    let write_txn = database.begin_write()?;
    // ... 15+ lines of worker registration
    write_txn.commit()?;
    Ok(())
}
```

**After**:
```rust
use blixard_core::test_helpers::{TestDatabaseFactory, TestWorkerFactory};

#[tokio::test]
async fn test_scheduler_basic_placement() {
    let (database, _temp_dir) = TestDatabaseFactory::create()?;
    let scheduler = VmScheduler::new(database.clone());

    // Add workers with different capacities
    TestWorkerFactory::register_worker(&database, 1, TestWorkerFactory::standard_capabilities(), true)?;
    TestWorkerFactory::register_worker(&database, 2, TestWorkerFactory::gpu_capabilities(), true)?;
    
    // Test continues...
}
```

### 2. Property Tests (raft_proptest.rs)
**Before** (repeated 6+ times in proptest blocks):
```rust
RUNTIME.block_on(async {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(Database::create(db_path).unwrap());

    // Manual worker registration
    let write_txn = database.begin_write().unwrap();
    let mut worker_table = write_txn.open_table(WORKER_TABLE).unwrap();
    let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE).unwrap();
    // ... complex serialization and insertion logic
    write_txn.commit().unwrap();
    
    // Test logic...
});
```

**After**:
```rust
RUNTIME.block_on(async {
    let (database, _temp_dir) = TestDatabaseFactory::create().unwrap();
    
    // Simple worker registration
    TestWorkerFactory::register_worker(&database, 1, capabilities, true).unwrap();
    
    // Test logic...
});
```

### 3. Storage Tests (storage_tests.rs)
**Before**:
```rust
async fn create_test_database() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::create(&db_path).unwrap();
    (database, temp_dir)
}

fn create_test_vm_state(name: &str, node_id: u64) -> VmState {
    let mut config = VmConfig::default();
    config.name = name.to_string();
    // ... manual config setup
    VmState {
        name: name.to_string(),
        config,
        status: VmStatus::Creating,
        // ... manual state setup
    }
}
```

**After**:
```rust
use blixard_core::test_helpers::{TestDatabaseFactory, TestVmFactory};

#[tokio::test]
async fn test_vm_state_persistence() {
    let (database, _temp_dir) = TestDatabaseFactory::create_raw()?; // Returns Database, not Arc<Database>
    let vm_state = TestVmFactory::create_vm_state("test-vm", 1);
    
    // Test storage logic...
}
```

## Factory Methods Available

### TestDatabaseFactory
- `create()` - Standard database with all tables initialized (returns Arc<Database>)
- `create_with_name(name)` - Named database for debugging
- `create_raw()` - Non-Arc database for compatibility
- `create_with_tables(&[table_names])` - Selective table initialization

### TestWorkerFactory  
- `register_worker(db, node_id, capabilities, is_online)` - Register worker in database
- `standard_capabilities()` - 8 CPU, 16GB RAM, 100GB disk
- `gpu_capabilities()` - 16 CPU, 32GB RAM, 500GB disk, GPU feature
- `low_resource_capabilities()` - 2 CPU, 4GB RAM, 50GB disk

### TestVmFactory
- `create_vm_config(name)` - Standard VM configuration
- `create_vm_state(name, node_id)` - Complete VM state
- `store_vm_state(db, vm_state)` - Store VM state in database

## Migration Strategy

1. **Phase 1**: Update highest-impact files first
   - `vm_scheduler_tests.rs` (10+ occurrences)
   - `raft_proptest.rs` (6+ occurrences)  
   - `storage_tests.rs` (8+ occurrences)

2. **Phase 2**: Property tests and integration tests
   - `multi_datacenter_test.rs`
   - `resource_admission_integration_test.rs`
   - `priority_scheduling_test.rs`

3. **Phase 3**: Remaining test files in blixard-core/tests/

## Benefits

1. **Code Reduction**: Eliminate 84+ instances of repeated setup code
2. **Consistency**: Standardized database initialization across all tests
3. **Maintainability**: Single location for test setup changes
4. **Error Handling**: Proper error context and handling
5. **Debugging**: Named databases and better error messages
6. **Extensibility**: Easy to add new factory methods for common patterns

## Files Impacted

### High Priority (10+ pattern occurrences):
- `blixard-core/tests/vm_scheduler_tests.rs` - 1 helper + 10 test usages
- `blixard-core/tests/raft_proptest.rs` - 6 proptest blocks
- `blixard-core/tests/storage_tests.rs` - 1 helper + 8 test usages
- `blixard-core/tests/multi_datacenter_test.rs` - 3 test functions

### Medium Priority (3-5 occurrences):
- `blixard-core/tests/priority_scheduling_test.rs` - 4 tests
- `blixard-core/tests/p2p_integration_test.rs` - 4 tests  
- `blixard-core/tests/p2p_manager_v2_test.rs` - 3 tests
- `blixard-core/tests/node_tests.rs` - 4 tests

### Low Priority (1-2 occurrences):
- 15+ additional test files with 1-2 occurrences each

## Total Impact
- **84+ pattern eliminations**
- **29+ files affected**
- **Estimated 500+ lines of code reduction**
- **Improved test maintainability and consistency**