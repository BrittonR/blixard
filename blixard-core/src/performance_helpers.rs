//! Performance optimization helpers to reduce unnecessary clone() calls
//!
//! This module provides patterns and utilities to minimize expensive cloning
//! operations in hot paths throughout the codebase.

use std::borrow::Cow;
use std::sync::Arc;

/// Trait for types that can provide lightweight string references
pub trait StringRef {
    /// Get a reference to the string data without cloning
    fn as_str_ref(&self) -> &str;
    
    /// Get a Cow<str> that avoids cloning when possible
    fn as_cow_str(&self) -> Cow<'_, str>;
}

impl StringRef for String {
    fn as_str_ref(&self) -> &str {
        self.as_str()
    }
    
    fn as_cow_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_str())
    }
}

impl StringRef for &str {
    fn as_str_ref(&self) -> &str {
        self
    }
    
    fn as_cow_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

impl StringRef for Arc<String> {
    fn as_str_ref(&self) -> &str {
        self.as_str()
    }
    
    fn as_cow_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_str())
    }
}

/// Lightweight view for Raft status that avoids cloning the full struct
#[derive(Debug)]
pub struct RaftStatusView<'a> {
    pub is_leader: bool,
    pub node_id: u64,
    pub leader_id: Option<u64>,
    pub term: u64,
    pub state: &'a str,
}

impl<'a> From<&'a crate::node_shared::RaftStatus> for RaftStatusView<'a> {
    fn from(status: &'a crate::node_shared::RaftStatus) -> Self {
        Self {
            is_leader: status.is_leader,
            node_id: status.node_id,
            leader_id: status.leader_id,
            term: status.term,
            state: &status.state,
        }
    }
}

/// Helper for efficient iteration over collections without cloning
pub trait IterWithoutClone<'a, T> {
    type Iterator: Iterator<Item = T>;
    
    /// Iterate over references without cloning the collection
    fn iter_ref(&'a self) -> Self::Iterator;
}

impl<'a, K, V> IterWithoutClone<'a, (&'a K, &'a V)> for std::collections::HashMap<K, V> {
    type Iterator = std::collections::hash_map::Iter<'a, K, V>;
    
    fn iter_ref(&'a self) -> Self::Iterator {
        self.iter()
    }
}

impl<'a, T> IterWithoutClone<'a, &'a T> for Vec<T> {
    type Iterator = std::slice::Iter<'a, T>;
    
    fn iter_ref(&'a self) -> Self::Iterator {
        self.iter()
    }
}

/// Metrics attributes builder that avoids repeated cloning
#[cfg(feature = "observability")]
pub struct AttributesBuilder {
    attributes: Vec<opentelemetry::KeyValue>,
}

#[cfg(feature = "observability")]
impl AttributesBuilder {
    pub fn new() -> Self {
        Self {
            attributes: Vec::new(),
        }
    }
    
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            attributes: Vec::with_capacity(capacity),
        }
    }
    
    /// Add an attribute without cloning if it's already a KeyValue
    pub fn add_kv(mut self, kv: opentelemetry::KeyValue) -> Self {
        self.attributes.push(kv);
        self
    }
    
    /// Add a string attribute using Cow to avoid unnecessary cloning
    pub fn add_str(mut self, key: &'static str, value: impl StringRef) -> Self {
        self.attributes.push(opentelemetry::KeyValue::new(key, value.as_cow_str().into_owned()));
        self
    }
    
    /// Add a numeric attribute
    pub fn add_u64(mut self, key: &'static str, value: u64) -> Self {
        self.attributes.push(opentelemetry::KeyValue::new(key, value as i64));
        self
    }
    
    /// Build the final attributes vector
    pub fn build(self) -> Vec<opentelemetry::KeyValue> {
        self.attributes
    }
    
    /// Build as a slice reference for immediate use
    pub fn as_slice(&self) -> &[opentelemetry::KeyValue] {
        &self.attributes
    }
}

/// No-op version when observability is disabled
#[cfg(not(feature = "observability"))]
pub struct AttributesBuilder;

#[cfg(not(feature = "observability"))]
impl AttributesBuilder {
    pub fn new() -> Self {
        Self
    }
    
    pub fn with_capacity(_capacity: usize) -> Self {
        Self
    }
    
    pub fn add_kv(self, _kv: ()) -> Self {
        self
    }
    
    pub fn add_str(self, _key: &'static str, _value: impl StringRef) -> Self {
        self
    }
    
    pub fn add_u64(self, _key: &'static str, _value: u64) -> Self {
        self
    }
    
    pub fn build(self) -> Vec<()> {
        Vec::new()
    }
    
    pub fn as_slice(&self) -> &[()] {
        &[]
    }
}

impl Default for AttributesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for Arc<T> that implements common borrowing patterns
pub struct SharedRef<T> {
    inner: Arc<T>,
}

impl<T> SharedRef<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(value),
        }
    }
    
    pub fn from_arc(arc: Arc<T>) -> Self {
        Self { inner: arc }
    }
    
    /// Get a reference without cloning the Arc
    pub fn get(&self) -> &T {
        &self.inner
    }
    
    /// Get an Arc clone only when needed
    pub fn arc_clone(&self) -> Arc<T> {
        Arc::clone(&self.inner)
    }
    
    /// Check if this is the only reference (useful for optimization)
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
}

impl<T> Clone for SharedRef<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> AsRef<T> for SharedRef<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::Deref for SharedRef<T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Extension trait for Result types to avoid cloning on success paths
pub trait ResultExt<T, E> {
    /// Map the ok value without cloning the error
    fn map_no_clone<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> U;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn map_no_clone<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Ok(value) => Ok(f(value)),
            Err(error) => Err(error),
        }
    }
}

/// Object pool for reusing expensive-to-allocate objects
pub struct ObjectPool<T> {
    pool: parking_lot::Mutex<Vec<T>>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> ObjectPool<T> {
    pub fn new<F>(factory: F) -> Self 
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            pool: parking_lot::Mutex::new(Vec::new()),
            factory: Box::new(factory),
        }
    }
    
    pub fn get(&self) -> PooledObject<T> {
        let object = {
            let mut pool = self.pool.lock();
            pool.pop().unwrap_or_else(|| (self.factory)())
        };
        
        PooledObject {
            object: Some(object),
            pool: &self.pool,
        }
    }
}

/// RAII wrapper that returns objects to pool on drop
pub struct PooledObject<'a, T> {
    object: Option<T>,
    pool: &'a parking_lot::Mutex<Vec<T>>,
}

impl<'a, T> PooledObject<'a, T> {
    pub fn get(&self) -> &T {
        self.object.as_ref().unwrap()
    }
    
    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<'a, T> Drop for PooledObject<'a, T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            let mut pool = self.pool.lock();
            // Limit pool size to prevent unbounded growth
            if pool.len() < 32 {
                pool.push(object);
            }
        }
    }
}

impl<'a, T> std::ops::Deref for PooledObject<'a, T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        self.object.as_ref().unwrap()
    }
}

impl<'a, T> std::ops::DerefMut for PooledObject<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.object.as_mut().unwrap()
    }
}

/// Pre-allocated hash map with known capacity for better performance
pub type FastHashMap<K, V> = std::collections::HashMap<K, V>;
pub type FastHashSet<T> = std::collections::HashSet<T>;

/// Helper to create pre-allocated HashMap with capacity
pub fn fast_hash_map_with_capacity<K, V>(capacity: usize) -> FastHashMap<K, V> {
    std::collections::HashMap::with_capacity(capacity)
}

/// Helper to create pre-allocated HashSet with capacity
pub fn fast_hash_set_with_capacity<T>(capacity: usize) -> FastHashSet<T> {
    std::collections::HashSet::with_capacity(capacity)
}

/// Helper trait for efficient formatting (example implementation)
/// In practice, use format_args! directly at call sites for best performance
pub trait LazyFormat {
    fn format_efficiently(&self) -> String;
}

impl LazyFormat for u64 {
    fn format_efficiently(&self) -> String {
        self.to_string()
    }
}

impl LazyFormat for f64 {
    fn format_efficiently(&self) -> String {
        format!("{:.2}", self)
    }
}

impl<'a> LazyFormat for &'a str {
    fn format_efficiently(&self) -> String {
        (*self).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_string_ref_no_clone() {
        let s = String::from("test");
        let cow = s.as_cow_str();
        assert!(matches!(cow, Cow::Borrowed(_)));
    }

    #[test]
    fn test_attributes_builder() {
        let attrs = AttributesBuilder::new()
            .add_str("vm_name", "test-vm")
            .add_u64("node_id", 123)
            .build();
        
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn test_shared_ref_no_unnecessary_clone() {
        let shared = SharedRef::new("test".to_string());
        let _ref1 = shared.get(); // No clone
        let _ref2 = shared.as_ref(); // No clone
        
        assert_eq!(Arc::strong_count(&shared.inner), 1);
    }
    
    #[test]
    fn test_object_pool() {
        let pool = ObjectPool::new(|| Vec::<i32>::with_capacity(10));
        
        {
            let mut obj1 = pool.get();
            obj1.push(1);
            obj1.push(2);
            assert_eq!(obj1.len(), 2);
        } // Object returned to pool here
        
        {
            let obj2 = pool.get();
            // Should reuse the object from pool, but it might be cleared
            assert!(obj2.capacity() >= 10);
        }
    }
    
    #[test]
    fn test_fast_hash_map() {
        let mut map = fast_hash_map_with_capacity::<String, i32>(10);
        map.insert("test".to_string(), 42);
        assert_eq!(map.get("test"), Some(&42));
    }
}