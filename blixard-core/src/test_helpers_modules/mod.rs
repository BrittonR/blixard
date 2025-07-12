pub mod timing_helpers;
pub mod cluster_helpers;
pub mod vm_helpers;

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};

// Re-export key components
pub use timing_helpers::*;
pub use cluster_helpers::*;
pub use vm_helpers::*;

/// Global port allocator for tests
static PORT_ALLOCATOR: AtomicU16 = AtomicU16::new(20000);

/// Diagnostic counters for port allocation
static PORT_ALLOCATION_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static PORT_ALLOCATION_FAILURES: AtomicU64 = AtomicU64::new(0);
static PORT_ALLOCATION_SUCCESSES: AtomicU64 = AtomicU64::new(0);

/// Port allocator for automatic port assignment
pub struct PortAllocator;

impl PortAllocator {
    /// Get the next available port for testing
    pub fn next_port() -> u16 {
        PORT_ALLOCATION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        
        let port = PORT_ALLOCATOR.fetch_add(1, Ordering::Relaxed);
        
        // Check if port is actually available (simple validation)
        if port > 65000 {
            // Reset to avoid port exhaustion in long-running test suites
            PORT_ALLOCATOR.store(20000, Ordering::Relaxed);
            PORT_ALLOCATION_FAILURES.fetch_add(1, Ordering::Relaxed);
            20000
        } else {
            PORT_ALLOCATION_SUCCESSES.fetch_add(1, Ordering::Relaxed);
            port
        }
    }

    /// Get allocation statistics for debugging
    pub fn get_stats() -> (u64, u64, u64) {
        (
            PORT_ALLOCATION_ATTEMPTS.load(Ordering::Relaxed),
            PORT_ALLOCATION_SUCCESSES.load(Ordering::Relaxed),
            PORT_ALLOCATION_FAILURES.load(Ordering::Relaxed),
        )
    }

    /// Reset the port allocator (useful for test isolation)
    pub fn reset() {
        PORT_ALLOCATOR.store(20000, Ordering::Relaxed);
        PORT_ALLOCATION_ATTEMPTS.store(0, Ordering::Relaxed);
        PORT_ALLOCATION_SUCCESSES.store(0, Ordering::Relaxed);
        PORT_ALLOCATION_FAILURES.store(0, Ordering::Relaxed);
    }
}

/// Database utilities for tests
pub mod database {
    use std::sync::Arc;
    use crate::error::{BlixardError, BlixardResult};
    use crate::raft_storage::init_database_tables;

    /// Create a temporary database for testing
    pub fn create() -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(&db_path)?);
        
        // Initialize all required tables
        init_database_tables(&database)?;
        
        Ok((database, temp_dir))
    }

    /// Create a temporary database with a specific name
    pub fn create_with_name(name: &str) -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join(format!("{}.db", name));
        let database = Arc::new(redb::Database::create(&db_path)?);
        
        // Initialize all required tables
        init_database_tables(&database)?;
        
        Ok((database, temp_dir))
    }

    /// Create a raw database without Arc wrapper
    pub fn create_raw() -> BlixardResult<(redb::Database, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let database = redb::Database::create(&db_path)?;
        
        // Initialize all required tables
        let database_arc = Arc::new(database);
        init_database_tables(&database_arc)?;
        
        // Extract the raw database from Arc
        let database = Arc::try_unwrap(database_arc).map_err(|_| BlixardError::Internal {
            message: "Failed to unwrap Arc<Database>".to_string(),
        })?;
        
        Ok((database, temp_dir))
    }

    /// Create database with only specific tables initialized
    pub fn create_with_tables(tables: &[&'static str]) -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let database = redb::Database::create(&db_path)?;
        
        // Initialize only requested tables
        let write_txn = database.begin_write()?;
        for table_name in tables {
            let _ = write_txn.open_table(redb::TableDefinition::<&str, &[u8]>::new(table_name))?;
        }
        write_txn.commit()?;
        
        Ok((Arc::new(database), temp_dir))
    }
}