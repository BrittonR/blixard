//! Arc/Rc optimization patterns
//!
//! This module provides utilities for optimizing Arc usage patterns,
//! including caching frequently accessed Arc instances and weak
//! reference management.

use std::sync::{Arc, Weak};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::hash::Hash;

/// Cache for Arc instances to avoid repeated allocations
pub struct ArcCache<K, V> {
    cache: Arc<RwLock<FxHashMap<K, Weak<V>>>>,
    max_size: usize,
}

impl<K: Hash + Eq + Clone, V> ArcCache<K, V> {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(FxHashMap::default())),
            max_size,
        }
    }
    
    /// Get or create an Arc for the given key
    pub fn get_or_create<F>(&self, key: &K, create_fn: F) -> Arc<V>
    where
        F: FnOnce() -> V,
    {
        // Try to get existing Arc
        {
            let cache = self.cache.read();
            if let Some(weak) = cache.get(key) {
                if let Some(arc) = weak.upgrade() {
                    return arc;
                }
            }
        }
        
        // Create new Arc
        let value = Arc::new(create_fn());
        let weak = Arc::downgrade(&value);
        
        // Store in cache
        {
            let mut cache = self.cache.write();
            
            // Clean up dead weak references if cache is full
            if cache.len() >= self.max_size {
                cache.retain(|_, weak| weak.strong_count() > 0);
                
                // If still full, remove oldest entries
                if cache.len() >= self.max_size {
                    let to_remove = cache.len() - self.max_size / 2;
                    let keys_to_remove: Vec<K> = cache.keys()
                        .take(to_remove)
                        .cloned()
                        .collect();
                    for key in keys_to_remove {
                        cache.remove(&key);
                    }
                }
            }
            
            cache.insert(key.clone(), weak);
        }
        
        value
    }
    
    /// Get an existing Arc if available
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        let cache = self.cache.read();
        cache.get(key).and_then(|weak| weak.upgrade())
    }
    
    /// Remove an entry from the cache
    pub fn remove(&self, key: &K) -> Option<Arc<V>> {
        let mut cache = self.cache.write();
        cache.remove(key).and_then(|weak| weak.upgrade())
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        self.cache.write().clear();
    }
    
    /// Get cache size (including dead weak references)
    pub fn size(&self) -> usize {
        self.cache.read().len()
    }
    
    /// Clean up dead weak references
    pub fn cleanup(&self) {
        let mut cache = self.cache.write();
        cache.retain(|_, weak| weak.strong_count() > 0);
    }
}

/// Weak reference cache for avoiding Arc upgrades
pub struct WeakCache<T> {
    cache: RwLock<Vec<Weak<T>>>,
    max_size: usize,
}

impl<T> WeakCache<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: RwLock::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }
    
    /// Add an Arc to the cache (stores weak reference)
    pub fn add(&self, arc: &Arc<T>) {
        let weak = Arc::downgrade(arc);
        let mut cache = self.cache.write();
        
        if cache.len() >= self.max_size {
            // Remove dead weak references
            cache.retain(|w| w.strong_count() > 0);
            
            // If still full, remove oldest
            if cache.len() >= self.max_size {
                cache.drain(0..self.max_size/2);
            }
        }
        
        cache.push(weak);
    }
    
    /// Try to find a matching Arc in the cache
    pub fn find<F>(&self, predicate: F) -> Option<Arc<T>>
    where
        F: Fn(&T) -> bool,
    {
        let cache = self.cache.read();
        for weak in cache.iter() {
            if let Some(arc) = weak.upgrade() {
                if predicate(&*arc) {
                    return Some(arc);
                }
            }
        }
        None
    }
    
    /// Get all valid Arcs from the cache
    pub fn get_all(&self) -> Vec<Arc<T>> {
        let cache = self.cache.read();
        cache.iter()
            .filter_map(|weak| weak.upgrade())
            .collect()
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        self.cache.write().clear();
    }
    
    /// Clean up dead weak references
    pub fn cleanup(&self) {
        let mut cache = self.cache.write();
        cache.retain(|weak| weak.strong_count() > 0);
    }
}

/// Helper to convert between Arc and Weak efficiently
pub struct ArcWeakPair<T> {
    strong: Option<Arc<T>>,
    weak: Weak<T>,
}

impl<T> ArcWeakPair<T> {
    /// Create from Arc
    pub fn from_arc(arc: Arc<T>) -> Self {
        let weak = Arc::downgrade(&arc);
        Self {
            strong: Some(arc),
            weak,
        }
    }
    
    /// Create from Weak
    pub fn from_weak(weak: Weak<T>) -> Self {
        Self {
            strong: None,
            weak,
        }
    }
    
    /// Get or upgrade to Arc
    pub fn get_arc(&mut self) -> Option<Arc<T>> {
        if self.strong.is_none() {
            self.strong = self.weak.upgrade();
        }
        self.strong.clone()
    }
    
    /// Drop the strong reference, keeping only weak
    pub fn drop_strong(&mut self) {
        self.strong = None;
    }
    
    /// Check if the value is still alive
    pub fn is_alive(&self) -> bool {
        self.weak.strong_count() > 0
    }
}

/// Optimize Arc cloning in hot paths by batching
pub struct ArcCloneBatcher<T> {
    source: Arc<T>,
    batch: Vec<Arc<T>>,
}

impl<T> ArcCloneBatcher<T> {
    pub fn new(source: Arc<T>, expected_clones: usize) -> Self {
        Self {
            source,
            batch: Vec::with_capacity(expected_clones),
        }
    }
    
    /// Pre-clone a batch of Arcs
    pub fn prepare_batch(&mut self, count: usize) {
        self.batch.reserve(count);
        for _ in 0..count {
            self.batch.push(self.source.clone());
        }
    }
    
    /// Get a pre-cloned Arc
    pub fn get_clone(&mut self) -> Arc<T> {
        self.batch.pop().unwrap_or_else(|| self.source.clone())
    }
    
    /// Get remaining pre-cloned Arcs
    pub fn drain(self) -> Vec<Arc<T>> {
        self.batch
    }
}

/// Helper trait for Arc optimization
pub trait ArcOptimize<T> {
    /// Clone only if not the sole owner
    fn clone_if_shared(&self) -> Option<Arc<T>>;
    
    /// Try to get mutable reference if sole owner
    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone;
}

impl<T> ArcOptimize<T> for Arc<T> {
    fn clone_if_shared(&self) -> Option<Arc<T>> {
        if Arc::strong_count(self) > 1 {
            Some(self.clone())
        } else {
            None
        }
    }
    
    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone,
    {
        Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arc_cache() {
        let cache = ArcCache::new(10);
        
        // Create and cache a value
        let arc1 = cache.get_or_create(&"key1", || "value1".to_string());
        assert_eq!(*arc1, "value1");
        
        // Should return the same Arc
        let arc2 = cache.get_or_create(&"key1", || panic!("Should not create"));
        assert!(Arc::ptr_eq(&arc1, &arc2));
        
        // Drop all strong references
        drop(arc1);
        drop(arc2);
        
        // Now it should create a new one
        let arc3 = cache.get_or_create(&"key1", || "value1_new".to_string());
        assert_eq!(*arc3, "value1_new");
    }
    
    #[test]
    fn test_weak_cache() {
        let cache = WeakCache::new(5);
        
        let arc1 = Arc::new(1);
        let arc2 = Arc::new(2);
        let arc3 = Arc::new(3);
        
        cache.add(&arc1);
        cache.add(&arc2);
        cache.add(&arc3);
        
        // Find specific value
        let found = cache.find(|&x| x == 2);
        assert!(found.is_some());
        assert_eq!(*found.unwrap(), 2);
        
        // Get all values
        let all = cache.get_all();
        assert_eq!(all.len(), 3);
    }
    
    #[test]
    fn test_arc_weak_pair() {
        let arc = Arc::new("test");
        let mut pair = ArcWeakPair::from_arc(arc.clone());
        
        assert!(pair.is_alive());
        assert!(pair.get_arc().is_some());
        
        // Drop strong reference in pair
        pair.drop_strong();
        
        // Should still be alive due to external arc
        assert!(pair.is_alive());
        
        // Drop external arc
        drop(arc);
        
        // Now should be dead
        assert!(!pair.is_alive());
        assert!(pair.get_arc().is_none());
    }
    
    #[test]
    fn test_arc_clone_batcher() {
        let source = Arc::new(vec![1, 2, 3]);
        let mut batcher = ArcCloneBatcher::new(source.clone(), 5);
        
        batcher.prepare_batch(3);
        
        let clone1 = batcher.get_clone();
        let clone2 = batcher.get_clone();
        
        assert!(Arc::ptr_eq(&source, &clone1));
        assert!(Arc::ptr_eq(&source, &clone2));
    }
}