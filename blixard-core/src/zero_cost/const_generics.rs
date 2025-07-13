//! Const generics for compile-time sized collections and computations
//!
//! This module leverages const generics to create zero-overhead abstractions
//! for fixed-size data structures with compile-time bounds checking.

use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut};

/// A fixed-capacity buffer with compile-time size
#[derive(Debug, Clone)]
pub struct ConstBuffer<T, const N: usize> {
    data: [T; N],
}

impl<T, const N: usize> ConstBuffer<T, N> {
    /// Create a new buffer filled with the given value
    pub const fn new(value: T) -> Self
    where
        T: Copy,
    {
        Self { data: [value; N] }
    }

    /// Create from an array
    pub const fn from_array(data: [T; N]) -> Self {
        Self { data }
    }

    /// Get the capacity (always N)
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get a reference to the underlying array
    pub const fn as_array(&self) -> &[T; N] {
        &self.data
    }

    /// Get a mutable reference to the underlying array
    pub fn as_mut_array(&mut self) -> &mut [T; N] {
        &mut self.data
    }
}

impl<T, const N: usize> Deref for ConstBuffer<T, N> {
    type Target = [T; N];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T, const N: usize> DerefMut for ConstBuffer<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

/// A const-sized string for compile-time string operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstString<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> ConstString<N> {
    /// Create an empty const string
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    /// Create from a string slice (const-friendly)
    pub const fn from_str(s: &str) -> Result<Self, &'static str> {
        let bytes = s.as_bytes();
        if bytes.len() > N {
            return Err("String too long for buffer");
        }

        let mut data = [0u8; N];
        let mut i = 0;
        while i < bytes.len() {
            data[i] = bytes[i];
            i += 1;
        }

        Ok(Self {
            bytes: data,
            len: bytes.len(),
        })
    }

    /// Get as string slice
    pub fn as_str(&self) -> &str {
        // Safety: We maintain UTF-8 validity invariant
        unsafe { std::str::from_utf8_unchecked(&self.bytes[..self.len]) }
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

    /// Append another string (mutable operation)
    pub fn append<const M: usize>(&mut self, other: &ConstString<M>) -> Result<(), &'static str> {
        if self.len + other.len > N {
            return Err("Concatenated string would exceed capacity");
        }
        
        // Copy other
        let mut j = 0;
        while j < other.len {
            self.bytes[self.len + j] = other.bytes[j];
            j += 1;
        }
        
        self.len += other.len;
        Ok(())
    }
}

impl<const N: usize> Deref for ConstString<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// A vector with fixed capacity, no heap allocation
pub struct FixedCapacityVec<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> FixedCapacityVec<T, N> {
    /// Create a new empty vector
    pub const fn new() -> Self {
        Self {
            // Safety: An array of MaybeUninit is always valid
            data: unsafe { MaybeUninit::uninit().assume_init() },
            len: 0,
        }
    }

    /// Get the current length
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the capacity
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Push an element (returns error if full)
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

    /// Get a reference to an element
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }

        unsafe { Some(&*self.data[index].as_ptr()) }
    }

    /// Get a mutable reference to an element
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        unsafe { Some(&mut *self.data[index].as_mut_ptr()) }
    }

    /// Clear the vector
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Get as slice
    pub fn as_slice(&self) -> &[T] {
        unsafe {
            std::slice::from_raw_parts(self.data.as_ptr() as *const T, self.len)
        }
    }

    /// Get as mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(self.data.as_mut_ptr() as *mut T, self.len)
        }
    }
}

impl<T, const N: usize> Drop for FixedCapacityVec<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T, const N: usize> Deref for FixedCapacityVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for FixedCapacityVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, const N: usize> Index<usize> for FixedCapacityVec<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<T, const N: usize> IndexMut<usize> for FixedCapacityVec<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}

/// Const function for compile-time min/max
pub const fn const_min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

pub const fn const_max(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// Const function for compile-time power of 2 check
pub const fn is_power_of_two(n: usize) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

/// Const function for next power of 2
pub const fn next_power_of_two(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut p = 1;
    while p < n {
        p *= 2;
    }
    p
}

/// Compile-time assertions
#[macro_export]
macro_rules! const_assert_eq {
    ($left:expr, $right:expr $(,)?) => {
        const _: () = assert!($left == $right);
    };
}

#[macro_export]
macro_rules! const_assert_ne {
    ($left:expr, $right:expr $(,)?) => {
        const _: () = assert!($left != $right);
    };
}

#[macro_export]
macro_rules! const_assert_size {
    ($ty:ty, $size:expr) => {
        const _: () = assert!(std::mem::size_of::<$ty>() == $size);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_buffer() {
        let mut buffer = ConstBuffer::<i32, 10>::new(0);
        buffer[0] = 42;
        assert_eq!(buffer[0], 42);
        assert_eq!(buffer.capacity(), 10);
    }

    #[test]
    fn test_const_string() {
        const HELLO: ConstString<10> = match ConstString::from_str("Hello") {
            Ok(s) => s,
            Err(_) => panic!(),
        };

        assert_eq!(HELLO.as_str(), "Hello");
        assert_eq!(HELLO.len(), 5);
        assert_eq!(HELLO.capacity(), 10);

        const WORLD: ConstString<10> = match ConstString::from_str(" World") {
            Ok(s) => s,
            Err(_) => panic!(),
        };

        let mut combined = HELLO;
        combined.append(&WORLD).unwrap();

        assert_eq!(combined.as_str(), "Hello World");
    }

    #[test]
    fn test_fixed_capacity_vec() {
        let mut vec = FixedCapacityVec::<i32, 5>::new();
        assert!(vec.is_empty());
        assert_eq!(vec.capacity(), 5);

        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec[2], 3);

        assert_eq!(vec.pop(), Some(3));
        assert_eq!(vec.len(), 2);

        // Test capacity limit
        vec.push(4).unwrap();
        vec.push(5).unwrap();
        vec.push(6).unwrap();
        assert_eq!(vec.push(7), Err(7));
    }

    #[test]
    fn test_const_functions() {
        const MIN: usize = const_min(5, 10);
        assert_eq!(MIN, 5);

        const MAX: usize = const_max(5, 10);
        assert_eq!(MAX, 10);

        const IS_POW2: bool = is_power_of_two(16);
        assert!(IS_POW2);

        const NEXT_POW2: usize = next_power_of_two(10);
        assert_eq!(NEXT_POW2, 16);
    }

    // Compile-time assertions
    const_assert_eq!(const_min(3, 7), 3);
    const_assert_ne!(const_max(3, 7), 3);
    const_assert_size!(u64, 8);
}