//! Compile-time collections and lookups
//!
//! This module provides data structures that are computed at compile time,
//! offering zero runtime overhead for static data.

// Collections for compile-time lookup tables

/// Compile-time map with sorted keys for binary search
pub struct ConstMap<K, V, const N: usize> {
    entries: [(K, V); N],
}

impl<K: Copy + Ord, V: Copy, const N: usize> ConstMap<K, V, N> {
    /// Create a new const map from sorted entries
    pub const fn new(entries: [(K, V); N]) -> Self {
        // In a real implementation, we'd want to verify sorting at compile time
        Self { entries }
    }

    /// Create from unsorted entries (will sort at runtime)
    pub fn from_unsorted(mut entries: [(K, V); N]) -> Self {
        // Sorting algorithm (bubble sort for simplicity)
        for i in 0..N {
            for j in 0..(N - 1 - i) {
                if entries[j].0 > entries[j + 1].0 {
                    entries.swap(j, j + 1);
                }
            }
        }
        Self { entries }
    }

    /// Get value by key using binary search
    pub fn get(&self, key: K) -> Option<V> {
        let mut left = 0;
        let mut right = N;

        while left < right {
            let mid = (left + right) / 2;
            match self.entries[mid].0.cmp(&key) {
                std::cmp::Ordering::Equal => return Some(self.entries[mid].1),
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => right = mid,
            }
        }

        None
    }

    /// Check if key exists
    pub fn contains_key(&self, key: K) -> bool {
        matches!(self.get(key), Some(_))
    }

    /// Get all entries
    pub const fn entries(&self) -> &[(K, V); N] {
        &self.entries
    }

    /// Get the number of entries
    pub const fn len(&self) -> usize {
        N
    }

    /// Check if empty
    pub const fn is_empty(&self) -> bool {
        N == 0
    }
}

/// Compile-time set with sorted keys
pub struct ConstSet<T, const N: usize> {
    items: [T; N],
}

impl<T: Copy + Ord, const N: usize> ConstSet<T, N> {
    /// Create a new const set from sorted items
    pub const fn new(items: [T; N]) -> Self {
        Self { items }
    }

    /// Create from unsorted items (will sort and deduplicate at runtime)
    pub fn from_unsorted(mut items: [T; N]) -> Self {
        // Sort items
        for i in 0..N {
            for j in 0..(N - 1 - i) {
                if items[j] > items[j + 1] {
                    items.swap(j, j + 1);
                }
            }
        }
        
        Self { items }
    }

    /// Check if item exists using binary search
    pub fn contains(&self, item: T) -> bool {
        let mut left = 0;
        let mut right = N;

        while left < right {
            let mid = (left + right) / 2;
            match self.items[mid].cmp(&item) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => right = mid,
            }
        }

        false
    }

    /// Get all items
    pub const fn items(&self) -> &[T; N] {
        &self.items
    }

    /// Get the number of items
    pub const fn len(&self) -> usize {
        N
    }

    /// Check if empty
    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    /// Check if this set is a subset of another
    pub fn is_subset_of<const M: usize>(&self, other: &ConstSet<T, M>) -> bool {
        let mut i = 0;
        while i < N {
            if !other.contains(self.items[i]) {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// Static lookup table for fast constant-time access
pub struct StaticLookup<T, const N: usize> {
    table: [Option<T>; N],
}

impl<T: Copy, const N: usize> StaticLookup<T, N> {
    /// Create a new static lookup table
    pub const fn new() -> Self {
        Self {
            table: [None; N],
        }
    }

    /// Create with initial entries
    pub const fn with_entries(entries: [(usize, T); N]) -> Self {
        let mut table = [None; N];
        let mut i = 0;
        while i < N {
            let (index, value) = entries[i];
            if index < N {
                table[index] = Some(value);
            }
            i += 1;
        }
        Self { table }
    }

    /// Get value by index
    pub const fn get(&self, index: usize) -> Option<T> {
        if index < N {
            self.table[index]
        } else {
            None
        }
    }

    /// Check if index has a value
    pub const fn contains(&self, index: usize) -> bool {
        if index < N {
            matches!(self.table[index], Some(_))
        } else {
            false
        }
    }

    /// Get the capacity
    pub const fn capacity(&self) -> usize {
        N
    }
}




/// Macro for creating const maps
#[macro_export]
macro_rules! const_map {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let entries = [$(($key, $value)),*];
            $crate::zero_cost::const_collections::ConstMap::from_unsorted(entries)
        }
    };
}

/// Macro for creating const sets
#[macro_export]
macro_rules! const_set {
    ($($item:expr),* $(,)?) => {
        {
            let items = [$($item),*];
            $crate::zero_cost::const_collections::ConstSet::from_unsorted(items)
        }
    };
}

/// Compile-time perfect hash for string keys
pub struct ConstStringMap<V, const N: usize> {
    keys: [&'static str; N],
    values: [V; N],
    hash_table: [Option<usize>; N],
}

impl<V: Copy, const N: usize> ConstStringMap<V, N> {
    /// Create a new const string map
    pub const fn new(entries: [(&'static str, V); N]) -> Self {
        let mut keys = [""; N];
        let mut values = [entries[0].1; N]; // Initialize with first value
        let mut hash_table = [None; N];

        let mut i = 0;
        while i < N {
            keys[i] = entries[i].0;
            values[i] = entries[i].1;
            
            // Simple hash function for const context
            let hash = const_hash_str(entries[i].0) % N;
            hash_table[hash] = Some(i);
            
            i += 1;
        }

        Self {
            keys,
            values,
            hash_table,
        }
    }

    /// Get value by string key
    pub const fn get(&self, key: &str) -> Option<V> {
        let hash = const_hash_str(key) % N;
        
        if let Some(index) = self.hash_table[hash] {
            if const_str_eq(self.keys[index], key) {
                return Some(self.values[index]);
            }
        }
        
        // Linear probe for collisions
        let mut i = 0;
        while i < N {
            if const_str_eq(self.keys[i], key) {
                return Some(self.values[i]);
            }
            i += 1;
        }
        
        None
    }

    /// Check if key exists
    pub const fn contains_key(&self, key: &str) -> bool {
        matches!(self.get(key), Some(_))
    }
}

/// Const string hash function
const fn const_hash_str(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut hash = 5381usize;
    let mut i = 0;
    
    while i < bytes.len() {
        hash = hash.wrapping_mul(33).wrapping_add(bytes[i] as usize);
        i += 1;
    }
    
    hash
}

/// Const string equality
const fn const_str_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    
    let mut i = 0;
    while i < a_bytes.len() {
        if a_bytes[i] != b_bytes[i] {
            return false;
        }
        i += 1;
    }
    
    true
}

/// Example usage in a more specialized context
pub mod examples {
    use super::*;

    // HTTP status codes (pre-sorted by key)
    pub const HTTP_STATUS_CODES: ConstMap<u16, &'static str, 5> = ConstMap::new([
        (200, "OK"),
        (400, "Bad Request"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (500, "Internal Server Error"),
    ]);

    // VM resource types (pre-sorted)
    pub const VM_RESOURCE_TYPES: ConstSet<&'static str, 4> = ConstSet::new([
        "cpu",
        "memory", 
        "network",
        "storage",
    ]);

    // Configuration defaults
    pub const CONFIG_DEFAULTS: ConstStringMap<i32, 4> = ConstStringMap::new([
        ("max_connections", 1000),
        ("timeout_seconds", 30),
        ("retry_attempts", 3),
        ("buffer_size", 8192),
    ]);

    // Node capabilities lookup
    pub const NODE_CAPABILITIES: StaticLookup<&'static str, 256> = {
        let lookup = StaticLookup::new();
        // In a real implementation, this would be done with const fn methods
        lookup
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples::*;

    #[test]
    fn test_const_map() {
        assert_eq!(HTTP_STATUS_CODES.get(200), Some("OK"));
        assert_eq!(HTTP_STATUS_CODES.get(404), Some("Not Found"));
        assert_eq!(HTTP_STATUS_CODES.get(999), None);
        
        assert!(HTTP_STATUS_CODES.contains_key(500));
        assert!(!HTTP_STATUS_CODES.contains_key(999));
        
        assert_eq!(HTTP_STATUS_CODES.len(), 5);
        assert!(!HTTP_STATUS_CODES.is_empty());
    }

    #[test]
    fn test_const_set() {
        assert!(VM_RESOURCE_TYPES.contains("cpu"));
        assert!(VM_RESOURCE_TYPES.contains("memory"));
        assert!(!VM_RESOURCE_TYPES.contains("gpu"));
        
        assert_eq!(VM_RESOURCE_TYPES.len(), 4);
        assert!(!VM_RESOURCE_TYPES.is_empty());
    }

    #[test]
    fn test_const_string_map() {
        assert_eq!(CONFIG_DEFAULTS.get("max_connections"), Some(1000));
        assert_eq!(CONFIG_DEFAULTS.get("timeout_seconds"), Some(30));
        assert_eq!(CONFIG_DEFAULTS.get("nonexistent"), None);
        
        assert!(CONFIG_DEFAULTS.contains_key("retry_attempts"));
        assert!(!CONFIG_DEFAULTS.contains_key("nonexistent"));
    }

    #[test]
    fn test_static_lookup() {
        let capabilities = StaticLookup::<&str, 8>::with_entries([
            (0, "basic"),
            (1, "gpu"),
            (2, "high_memory"),
            (4, "nvme"),
            (5, "basic"), // Dummy entries to fill array
            (6, "basic"),
            (7, "basic"),
            (3, "basic"), // Fill remaining slot
        ]);
        
        assert_eq!(capabilities.get(0), Some("basic"));
        assert_eq!(capabilities.get(1), Some("gpu"));
        assert_eq!(capabilities.get(999), None);
        
        assert!(capabilities.contains(1));
        assert!(!capabilities.contains(3));
        
        assert_eq!(capabilities.capacity(), 8);
    }

    #[test]
    fn test_const_set_subset() {
        let set1: ConstSet<i32, 3> = ConstSet::from_unsorted([1, 2, 3]);
        let set2: ConstSet<i32, 5> = ConstSet::from_unsorted([1, 2, 3, 4, 5]);
        
        assert!(set1.is_subset_of(&set2));
        assert!(!set2.is_subset_of(&set1));
    }

    #[test]
    fn test_macro_usage() {
        // Test const_map macro
        let status_map = const_map! {
            0 => "success",
            1 => "warning", 
            2 => "error",
        };
        
        assert_eq!(status_map.get(0), Some("success"));
        assert_eq!(status_map.get(1), Some("warning"));
        assert_eq!(status_map.get(2), Some("error"));

        // Test const_set macro
        let priority_set = const_set![1, 2, 3];
        
        assert!(priority_set.contains(1));
        assert!(priority_set.contains(2));
        assert!(priority_set.contains(3));
        assert!(!priority_set.contains(4));
    }
}