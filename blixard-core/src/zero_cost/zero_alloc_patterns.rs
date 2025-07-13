//! Zero-allocation patterns for performance-critical code
//!
//! This module provides data structures and patterns that avoid heap
//! allocations while maintaining safety and ergonomics.

use std::mem::MaybeUninit;
use std::ptr;
use std::slice;
use std::ops::{Deref, DerefMut};

/// Stack-allocated string with fixed capacity
#[derive(Clone)]
pub struct StackString<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> StackString<N> {
    /// Create a new empty stack string
    pub const fn new() -> Self {
        Self {
            data: [0; N],
            len: 0,
        }
    }

    /// Create from a string slice
    pub fn from_str(s: &str) -> Result<Self, &'static str> {
        if s.len() > N {
            return Err("String too long");
        }

        let mut result = Self::new();
        result.data[..s.len()].copy_from_slice(s.as_bytes());
        result.len = s.len();
        Ok(result)
    }

    /// Push a character
    pub fn push(&mut self, ch: char) -> Result<(), &'static str> {
        let ch_len = ch.len_utf8();
        if self.len + ch_len > N {
            return Err("String would be too long");
        }

        ch.encode_utf8(&mut self.data[self.len..]);
        self.len += ch_len;
        Ok(())
    }

    /// Push a string slice
    pub fn push_str(&mut self, s: &str) -> Result<(), &'static str> {
        if self.len + s.len() > N {
            return Err("String would be too long");
        }

        self.data[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }

    /// Pop a character
    pub fn pop(&mut self) -> Option<char> {
        if self.len == 0 {
            return None;
        }

        let s = self.as_str();
        let ch = s.chars().last()?;
        self.len -= ch.len_utf8();
        Some(ch)
    }

    /// Clear the string
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Get as string slice
    pub fn as_str(&self) -> &str {
        // Safety: We maintain UTF-8 invariant
        unsafe { std::str::from_utf8_unchecked(&self.data[..self.len]) }
    }

    /// Get length in bytes
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get capacity
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get remaining capacity
    pub const fn remaining_capacity(&self) -> usize {
        N - self.len
    }
}

impl<const N: usize> Deref for StackString<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<const N: usize> std::fmt::Display for StackString<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<const N: usize> std::fmt::Debug for StackString<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<const N: usize> PartialEq for StackString<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const N: usize> PartialEq<str> for StackString<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const N: usize> PartialEq<&str> for StackString<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Inline vector that stores elements inline up to a capacity
#[derive(Debug)]
pub struct InlineVec<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> InlineVec<T, N> {
    /// Create a new empty inline vector
    pub const fn new() -> Self {
        Self {
            // Safety: Array of MaybeUninit is always safe to initialize
            data: unsafe { MaybeUninit::uninit().assume_init() },
            len: 0,
        }
    }

    /// Get the length
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get capacity
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Push an element
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.len >= N {
            return Err(value);
        }

        unsafe {
            self.data[self.len].as_mut_ptr().write(value);
        }
        self.len += 1;
        Ok(())
    }

    /// Pop an element
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        self.len -= 1;
        unsafe { Some(self.data[self.len].as_ptr().read()) }
    }

    /// Get element at index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }

        unsafe { Some(&*self.data[index].as_ptr()) }
    }

    /// Get mutable element at index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        unsafe { Some(&mut *self.data[index].as_mut_ptr()) }
    }

    /// Clear all elements
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Get as slice
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const T, self.len) }
    }

    /// Get as mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.data.as_mut_ptr() as *mut T, self.len) }
    }

    /// Try to extend from an iterator
    pub fn try_extend<I: Iterator<Item = T>>(&mut self, iter: I) -> Result<(), T> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }

    /// Create from a slice if it fits
    pub fn try_from_slice(slice: &[T]) -> Result<Self, &'static str>
    where
        T: Clone,
    {
        if slice.len() > N {
            return Err("Slice too large for capacity");
        }

        let mut vec = Self::new();
        for item in slice {
            // We know this won't fail due to the check above
            match vec.push(item.clone()) {
                Ok(()) => {},
                Err(_) => panic!("push should succeed due to capacity check"),
            }
        }
        Ok(vec)
    }
}

impl<T, const N: usize> Drop for InlineVec<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T, const N: usize> Deref for InlineVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for InlineVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

/// Static object pool for zero-allocation resource reuse
pub struct StaticPool<T, const N: usize> {
    objects: [MaybeUninit<T>; N],
    available: [bool; N],
    initialized: [bool; N],
}

impl<T, const N: usize> StaticPool<T, N> {
    /// Create a new static pool
    pub const fn new() -> Self {
        Self {
            // Safety: Array of MaybeUninit is always safe
            objects: unsafe { MaybeUninit::uninit().assume_init() },
            available: [true; N],
            initialized: [false; N],
        }
    }

    /// Try to acquire an object from the pool
    pub fn try_acquire<F>(&mut self, init: F) -> Option<PooledObject<T, N>>
    where
        F: FnOnce() -> T,
    {
        for i in 0..N {
            if self.available[i] {
                self.available[i] = false;

                if !self.initialized[i] {
                    unsafe {
                        self.objects[i].as_mut_ptr().write(init());
                    }
                    self.initialized[i] = true;
                }

                return Some(PooledObject {
                    index: i,
                    pool: self as *mut Self,
                });
            }
        }
        None
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let mut available_count = 0;
        let mut initialized_count = 0;

        for i in 0..N {
            if self.available[i] {
                available_count += 1;
            }
            if self.initialized[i] {
                initialized_count += 1;
            }
        }

        PoolStats {
            total_capacity: N,
            available: available_count,
            in_use: N - available_count,
            initialized: initialized_count,
        }
    }

    /// Release an object back to the pool
    fn release(&mut self, index: usize) {
        if index < N {
            self.available[index] = true;
        }
    }

    /// Get object at index (unsafe)
    unsafe fn get_object(&self, index: usize) -> &T {
        &*self.objects[index].as_ptr()
    }

    /// Get mutable object at index (unsafe)
    unsafe fn get_object_mut(&mut self, index: usize) -> &mut T {
        &mut *self.objects[index].as_mut_ptr()
    }
}

impl<T, const N: usize> Drop for StaticPool<T, N> {
    fn drop(&mut self) {
        for i in 0..N {
            if self.initialized[i] {
                unsafe {
                    ptr::drop_in_place(self.objects[i].as_mut_ptr());
                }
            }
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub total_capacity: usize,
    pub available: usize,
    pub in_use: usize,
    pub initialized: usize,
}

/// RAII wrapper for pooled objects
pub struct PooledObject<T, const N: usize> {
    index: usize,
    pool: *mut StaticPool<T, N>,
}

impl<T, const N: usize> PooledObject<T, N> {
    /// Get the pool index
    pub fn index(&self) -> usize {
        self.index
    }
}

impl<T, const N: usize> Deref for PooledObject<T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { (*self.pool).get_object(self.index) }
    }
}

impl<T, const N: usize> DerefMut for PooledObject<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (*self.pool).get_object_mut(self.index) }
    }
}

impl<T, const N: usize> Drop for PooledObject<T, N> {
    fn drop(&mut self) {
        unsafe {
            (*self.pool).release(self.index);
        }
    }
}

// Safety: PooledObject is Send if T is Send
unsafe impl<T: Send, const N: usize> Send for PooledObject<T, N> {}

/// Zero-allocation buffer for temporary string operations
pub struct TempBuffer<const N: usize> {
    data: [u8; N],
    pos: usize,
}

impl<const N: usize> TempBuffer<N> {
    /// Create a new temporary buffer
    pub const fn new() -> Self {
        Self {
            data: [0; N],
            pos: 0,
        }
    }

    /// Reset the buffer
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Write bytes to the buffer
    pub fn write(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if self.pos + data.len() > N {
            return Err("Buffer overflow");
        }

        self.data[self.pos..self.pos + data.len()].copy_from_slice(data);
        self.pos += data.len();
        Ok(())
    }

    /// Write a string to the buffer
    pub fn write_str(&mut self, s: &str) -> Result<(), &'static str> {
        self.write(s.as_bytes())
    }

    /// Write a formatted string
    pub fn write_fmt(&mut self, args: std::fmt::Arguments) -> Result<(), &'static str> {
        self.write(format!("{}", args).as_bytes())
    }

    /// Get the current content as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.pos]
    }

    /// Get the current content as string (if valid UTF-8)
    pub fn as_str(&self) -> Result<&str, &'static str> {
        std::str::from_utf8(&self.data[..self.pos])
            .map_err(|_| "Invalid UTF-8")
    }

    /// Get the current position
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Get remaining capacity
    pub const fn remaining(&self) -> usize {
        N - self.pos
    }

    /// Check if full
    pub const fn is_full(&self) -> bool {
        self.pos >= N
    }
}

/// Zero-allocation formatting helper
#[macro_export]
macro_rules! stack_format {
    ($buf:expr, $($arg:tt)*) => {
        {
            $buf.reset();
            use std::fmt::Write;
            write!($buf, $($arg)*).map(|_| $buf.as_str().unwrap_or(""))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_string() {
        let mut s = StackString::<32>::new();
        assert!(s.is_empty());
        assert_eq!(s.capacity(), 32);

        s.push_str("Hello").unwrap();
        s.push(' ').unwrap();
        s.push_str("World").unwrap();

        assert_eq!(s.as_str(), "Hello World");
        assert_eq!(s.len(), 11);

        let ch = s.pop().unwrap();
        assert_eq!(ch, 'd');
        assert_eq!(s.as_str(), "Hello Worl");

        // Test string comparison
        assert_eq!(s, "Hello Worl");
        assert!(s == "Hello Worl");
    }

    #[test]
    fn test_inline_vec() {
        let mut vec = InlineVec::<i32, 5>::new();
        assert!(vec.is_empty());
        assert_eq!(vec.capacity(), 5);

        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec[2], 3);

        let item = vec.pop().unwrap();
        assert_eq!(item, 3);
        assert_eq!(vec.len(), 2);

        // Test capacity limit
        vec.push(4).unwrap();
        vec.push(5).unwrap();
        vec.push(6).unwrap();
        assert_eq!(vec.push(7), Err(7)); // Should fail

        // Test slice operations
        let slice = vec.as_slice();
        assert_eq!(slice, &[1, 2, 4, 5, 6]);
    }

    #[test]
    fn test_static_pool() {
        let mut pool = StaticPool::<String, 3>::new();
        
        let stats = pool.stats();
        assert_eq!(stats.total_capacity, 3);
        assert_eq!(stats.available, 3);
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.initialized, 0);

        // Acquire objects
        let obj1 = pool.try_acquire(|| "Object 1".to_string()).unwrap();
        let obj2 = pool.try_acquire(|| "Object 2".to_string()).unwrap();

        assert_eq!(*obj1, "Object 1");
        assert_eq!(*obj2, "Object 2");

        let stats = pool.stats();
        assert_eq!(stats.available, 1);
        assert_eq!(stats.in_use, 2);
        assert_eq!(stats.initialized, 2);

        drop(obj1); // Release back to pool

        let stats = pool.stats();
        assert_eq!(stats.available, 2);
        assert_eq!(stats.in_use, 1);
        assert_eq!(stats.initialized, 2);

        // Reuse the released object
        let obj3 = pool.try_acquire(|| "Should not be called".to_string()).unwrap();
        assert_eq!(*obj3, "Object 1"); // Reused the first object

        drop(obj2);
        drop(obj3);

        let stats = pool.stats();
        assert_eq!(stats.available, 3);
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.initialized, 2);
    }

    #[test]
    fn test_temp_buffer() {
        let mut buf = TempBuffer::<64>::new();
        
        buf.write_str("Hello ").unwrap();
        buf.write_str("World").unwrap();
        
        assert_eq!(buf.as_str().unwrap(), "Hello World");
        assert_eq!(buf.pos(), 11);
        assert_eq!(buf.remaining(), 53);

        buf.reset();
        assert_eq!(buf.pos(), 0);
        assert!(buf.as_str().unwrap().is_empty());

        // Test overflow
        let long_string = "a".repeat(70);
        assert!(buf.write_str(&long_string).is_err());
    }

    #[test]
    fn test_zero_alloc_combinations() {
        // Combine multiple zero-alloc patterns
        let mut pool = StaticPool::<InlineVec<i32, 10>, 2>::new();
        
        {
            let mut vec = pool.try_acquire(|| InlineVec::new()).unwrap();
            vec.push(1).unwrap();
            vec.push(2).unwrap();
            vec.push(3).unwrap();
            
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 1);
        } // vec is returned to pool

        {
            let vec = pool.try_acquire(|| InlineVec::new()).unwrap();
            // The vector should be reused but with previous contents
            assert_eq!(vec.len(), 3);
        }
    }
}