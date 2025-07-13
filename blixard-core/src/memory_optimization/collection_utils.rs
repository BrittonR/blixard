//! Collection optimization utilities
//!
//! This module provides utilities for optimizing collection usage,
//! including small-vector optimization, efficient maps, and capacity
//! pre-allocation helpers.

use std::collections::HashMap;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec as SmallVecImpl;

/// Re-export SmallVec for stack-allocated small collections
pub type SmallVec<T> = SmallVecImpl<[T; 8]>;

/// Type alias for small vectors with custom size
pub type SmallVecN<T, const N: usize> = SmallVecImpl<[T; N]>;

/// Fast hash map using FxHash (faster than default SipHash for small keys)
pub type FlatMap<K, V> = FxHashMap<K, V>;

/// Fast hash set using FxHash
pub type FlatSet<T> = FxHashSet<T>;

/// Pre-allocate capacity for collections based on expected size
pub trait PreallocateCapacity {
    /// Pre-allocate with exact capacity
    fn with_capacity_exact(capacity: usize) -> Self;
    
    /// Pre-allocate with some headroom (125% of expected)
    fn with_capacity_headroom(expected: usize) -> Self;
}

impl<T> PreallocateCapacity for Vec<T> {
    fn with_capacity_exact(capacity: usize) -> Self {
        Vec::with_capacity(capacity)
    }
    
    fn with_capacity_headroom(expected: usize) -> Self {
        Vec::with_capacity(expected + expected / 4)
    }
}

impl<K, V> PreallocateCapacity for HashMap<K, V> {
    fn with_capacity_exact(capacity: usize) -> Self {
        HashMap::with_capacity(capacity)
    }
    
    fn with_capacity_headroom(expected: usize) -> Self {
        HashMap::with_capacity(expected + expected / 4)
    }
}

impl<K, V> PreallocateCapacity for FlatMap<K, V> {
    fn with_capacity_exact(capacity: usize) -> Self {
        FxHashMap::with_capacity_and_hasher(capacity, Default::default())
    }
    
    fn with_capacity_headroom(expected: usize) -> Self {
        FxHashMap::with_capacity_and_hasher(expected + expected / 4, Default::default())
    }
}

/// Helper to calculate optimal capacity based on expected size
pub fn preallocate_capacity(expected_size: usize, growth_factor: f32) -> usize {
    (expected_size as f32 * growth_factor).ceil() as usize
}

/// Efficient string collection builder that minimizes allocations
pub struct StringCollectionBuilder {
    strings: Vec<String>,
    total_capacity: usize,
}

impl StringCollectionBuilder {
    pub fn new(expected_count: usize, avg_string_len: usize) -> Self {
        Self {
            strings: Vec::with_capacity(expected_count),
            total_capacity: expected_count * avg_string_len,
        }
    }
    
    pub fn add(&mut self, s: String) {
        self.strings.push(s);
    }
    
    pub fn add_str(&mut self, s: &str) {
        self.strings.push(s.to_string());
    }
    
    pub fn build(self) -> Vec<String> {
        self.strings
    }
    
    /// Build into a single concatenated string with separator
    pub fn build_concat(self, separator: &str) -> String {
        let mut result = String::with_capacity(self.total_capacity + (self.strings.len() * separator.len()));
        for (i, s) in self.strings.into_iter().enumerate() {
            if i > 0 {
                result.push_str(separator);
            }
            result.push_str(&s);
        }
        result
    }
}

/// Reusable buffer for temporary collections
pub struct ReusableBuffer<T> {
    buffer: Vec<T>,
}

impl<T> ReusableBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }
    
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
    
    pub fn get_mut(&mut self) -> &mut Vec<T> {
        self.buffer.clear();
        &mut self.buffer
    }
    
    /// Take ownership of buffer contents, leaving empty buffer
    pub fn take(&mut self) -> Vec<T> {
        let capacity = self.buffer.capacity();
        std::mem::replace(&mut self.buffer, Vec::with_capacity(capacity))
    }
}

/// Pool of reusable vectors to avoid allocations
pub struct VecPool<T> {
    pool: parking_lot::Mutex<Vec<Vec<T>>>,
    max_vecs: usize,
    vec_capacity: usize,
}

impl<T> VecPool<T> {
    pub fn new(max_vecs: usize, vec_capacity: usize) -> Self {
        Self {
            pool: parking_lot::Mutex::new(Vec::with_capacity(max_vecs)),
            max_vecs,
            vec_capacity,
        }
    }
    
    pub fn acquire(&self) -> Vec<T> {
        let mut pool = self.pool.lock();
        pool.pop().unwrap_or_else(|| Vec::with_capacity(self.vec_capacity))
    }
    
    pub fn release(&self, mut vec: Vec<T>) {
        vec.clear();
        let mut pool = self.pool.lock();
        if pool.len() < self.max_vecs {
            pool.push(vec);
        }
    }
}

/// Scoped vector that returns to pool on drop
pub struct ScopedVec<'a, T> {
    vec: Option<Vec<T>>,
    pool: &'a VecPool<T>,
}

impl<'a, T> ScopedVec<'a, T> {
    pub fn new(pool: &'a VecPool<T>) -> Self {
        Self {
            vec: Some(pool.acquire()),
            pool,
        }
    }
}

impl<'a, T> Drop for ScopedVec<'a, T> {
    fn drop(&mut self) {
        if let Some(vec) = self.vec.take() {
            self.pool.release(vec);
        }
    }
}

impl<'a, T> std::ops::Deref for ScopedVec<'a, T> {
    type Target = Vec<T>;
    
    fn deref(&self) -> &Self::Target {
        self.vec.as_ref().unwrap()
    }
}

impl<'a, T> std::ops::DerefMut for ScopedVec<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.vec.as_mut().unwrap()
    }
}

/// Efficient batch operations on collections
pub trait BatchOperations<T> {
    /// Extend from iterator with pre-allocated capacity
    fn extend_from_iter_sized<I>(&mut self, iter: I, size_hint: usize)
    where
        I: IntoIterator<Item = T>;
}

impl<T> BatchOperations<T> for Vec<T> {
    fn extend_from_iter_sized<I>(&mut self, iter: I, size_hint: usize)
    where
        I: IntoIterator<Item = T>,
    {
        self.reserve(size_hint);
        self.extend(iter);
    }
}

/// Helper to efficiently collect into pre-sized collection
pub fn collect_sized<T, I>(iter: I, size_hint: usize) -> Vec<T>
where
    I: IntoIterator<Item = T>,
{
    let mut vec = Vec::with_capacity(size_hint);
    vec.extend(iter);
    vec
}

/// Helper to efficiently collect into SmallVec
pub fn collect_small<T, I, const N: usize>(iter: I) -> SmallVecN<T, N>
where
    I: IntoIterator<Item = T>,
{
    iter.into_iter().collect()
}

/// Global vec pools for common types
pub mod global_vec_pools {
    use super::*;
    use once_cell::sync::Lazy;
    
    pub static STRING_VEC_POOL: Lazy<VecPool<String>> = Lazy::new(|| {
        VecPool::new(100, 32)
    });
    
    pub static U64_VEC_POOL: Lazy<VecPool<u64>> = Lazy::new(|| {
        VecPool::new(100, 64)
    });
    
    pub static BYTES_VEC_POOL: Lazy<VecPool<u8>> = Lazy::new(|| {
        VecPool::new(50, 4096)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_small_vec() {
        let mut sv: SmallVec<u32> = SmallVec::new();
        
        // Should be stack allocated for small sizes
        for i in 0..5 {
            sv.push(i);
        }
        assert_eq!(sv.len(), 5);
        
        // Should spill to heap for larger sizes
        for i in 5..20 {
            sv.push(i);
        }
        assert_eq!(sv.len(), 20);
    }
    
    #[test]
    fn test_string_collection_builder() {
        let mut builder = StringCollectionBuilder::new(3, 10);
        builder.add_str("hello");
        builder.add_str("world");
        builder.add_str("test");
        
        let concat = builder.build_concat(", ");
        assert_eq!(concat, "hello, world, test");
    }
    
    #[test]
    fn test_vec_pool() {
        let pool = VecPool::<u32>::new(5, 10);
        
        let mut vec1 = pool.acquire();
        vec1.push(1);
        vec1.push(2);
        
        assert_eq!(vec1.len(), 2);
        
        pool.release(vec1);
        
        let vec2 = pool.acquire();
        assert_eq!(vec2.len(), 0); // Should be cleared
        assert!(vec2.capacity() >= 10); // Should maintain capacity
    }
    
    #[test]
    fn test_scoped_vec() {
        let pool = VecPool::<String>::new(5, 10);
        
        {
            let mut scoped = ScopedVec::new(&pool);
            scoped.push("test".to_string());
            assert_eq!(scoped.len(), 1);
        } // Automatically returned to pool
        
        let vec = pool.acquire();
        assert_eq!(vec.len(), 0); // Should be cleared
    }
}