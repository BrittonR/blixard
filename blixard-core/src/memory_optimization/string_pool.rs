//! String pooling and interning for reduced allocations
//!
//! This module provides string interning capabilities to avoid duplicate
//! string allocations for commonly used strings like node IDs, VM names,
//! and status strings.

use std::borrow::Cow;
use std::sync::Arc;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;

/// An interned string that references a pooled string value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InternedString(Arc<str>);

impl InternedString {
    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for InternedString {
    type Target = str;
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<InternedString> for String {
    fn from(interned: InternedString) -> Self {
        interned.0.to_string()
    }
}

/// A thread-safe string pool for interning commonly used strings
pub struct StringPool {
    pool: Arc<RwLock<FxHashMap<Box<str>, Arc<str>>>>,
    /// Maximum number of strings to pool (to prevent unbounded growth)
    max_size: usize,
}

impl StringPool {
    /// Create a new string pool with the specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            pool: Arc::new(RwLock::new(FxHashMap::default())),
            max_size,
        }
    }
    
    /// Create a string pool with default settings
    pub fn default() -> Self {
        Self::new(10_000)
    }
    
    /// Intern a string, returning a reference to the pooled version
    pub fn intern(&self, s: &str) -> InternedString {
        // Fast path: check if already interned
        {
            let pool = self.pool.read();
            if let Some(arc_str) = pool.get(s) {
                return InternedString(arc_str.clone());
            }
        }
        
        // Slow path: add to pool
        let mut pool = self.pool.write();
        
        // Double-check after acquiring write lock
        if let Some(arc_str) = pool.get(s) {
            return InternedString(arc_str.clone());
        }
        
        // Check size limit and evict if necessary
        if pool.len() >= self.max_size {
            // Simple eviction: remove ~10% of least recently used
            // In production, could use LRU cache instead
            let to_remove = self.max_size / 10;
            let keys_to_remove: Vec<_> = pool.keys()
                .take(to_remove)
                .cloned()
                .collect();
            for key in keys_to_remove {
                pool.remove(&key);
            }
        }
        
        // Add new string
        let boxed_str: Box<str> = s.into();
        let arc_str: Arc<str> = boxed_str.clone().into();
        pool.insert(boxed_str, arc_str.clone());
        InternedString(arc_str)
    }
    
    /// Intern a String, avoiding allocation if already interned
    pub fn intern_string(&self, s: String) -> InternedString {
        // Check if already interned before allocating
        {
            let pool = self.pool.read();
            if let Some(arc_str) = pool.get(s.as_str()) {
                return InternedString(arc_str.clone());
            }
        }
        
        // Not found, intern it (will convert String to Box<str>)
        self.intern(&s)
    }
    
    /// Get current pool size
    pub fn size(&self) -> usize {
        self.pool.read().len()
    }
    
    /// Clear the string pool
    pub fn clear(&self) {
        self.pool.write().clear();
    }
}

/// Global string pool for commonly used strings
lazy_static::lazy_static! {
    static ref GLOBAL_STRING_POOL: StringPool = StringPool::default();
}

/// Intern a string using the global pool
pub fn intern(s: &str) -> InternedString {
    GLOBAL_STRING_POOL.intern(s)
}

/// Intern a String using the global pool
pub fn intern_string(s: String) -> InternedString {
    GLOBAL_STRING_POOL.intern_string(s)
}

/// Common VM status strings pre-interned for efficiency
pub mod vm_status {
    use super::*;
    
    lazy_static::lazy_static! {
        pub static ref CREATING: InternedString = intern("creating");
        pub static ref STARTING: InternedString = intern("starting");
        pub static ref RUNNING: InternedString = intern("running");
        pub static ref STOPPING: InternedString = intern("stopping");
        pub static ref STOPPED: InternedString = intern("stopped");
        pub static ref FAILED: InternedString = intern("failed");
    }
}

/// Common node status strings pre-interned for efficiency
pub mod node_status {
    use super::*;
    
    lazy_static::lazy_static! {
        pub static ref LEADER: InternedString = intern("leader");
        pub static ref FOLLOWER: InternedString = intern("follower");
        pub static ref CANDIDATE: InternedString = intern("candidate");
        pub static ref LEARNER: InternedString = intern("learner");
        pub static ref INITIALIZING: InternedString = intern("initializing");
        pub static ref READY: InternedString = intern("ready");
    }
}

/// Helper to use Cow for conditional string ownership
pub fn cow_str<'a>(s: &'a str, condition: bool) -> Cow<'a, str> {
    if condition {
        Cow::Owned(s.to_string())
    } else {
        Cow::Borrowed(s)
    }
}

/// Helper to avoid allocation when concatenating small strings
pub fn small_string_concat<const N: usize>(parts: &[&str]) -> String {
    let total_len: usize = parts.iter().map(|s| s.len()).sum();
    let mut result = String::with_capacity(total_len);
    for part in parts {
        result.push_str(part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_interning() {
        let pool = StringPool::new(100);
        
        let s1 = pool.intern("test");
        let s2 = pool.intern("test");
        
        // Should be the same Arc
        assert!(Arc::ptr_eq(&s1.0, &s2.0));
        
        // Different strings should have different Arcs
        let s3 = pool.intern("other");
        assert!(!Arc::ptr_eq(&s1.0, &s3.0));
    }
    
    #[test]
    fn test_string_pool_eviction() {
        let pool = StringPool::new(10);
        
        // Fill the pool
        for i in 0..15 {
            pool.intern(&format!("string{}", i));
        }
        
        // Pool should not exceed max size (with some tolerance for eviction)
        assert!(pool.size() <= 10);
    }
    
    #[test]
    fn test_global_interning() {
        let s1 = intern("global_test");
        let s2 = intern("global_test");
        
        assert!(Arc::ptr_eq(&s1.0, &s2.0));
    }
    
    #[test]
    fn test_vm_status_constants() {
        // Pre-interned constants should be ready to use
        assert_eq!(vm_status::RUNNING.as_str(), "running");
        assert_eq!(vm_status::STOPPED.as_str(), "stopped");
    }
}