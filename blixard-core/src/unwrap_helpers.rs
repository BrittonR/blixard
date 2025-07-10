//! Helper macros and utilities for replacing unwrap() calls with proper error handling
//!
//! This module provides standardized patterns for converting unwrap() calls into 
//! graceful error handling that integrates with Blixard's error system.

use crate::error::BlixardError;

/// Macro for safely accessing collections with proper error context
/// 
/// # Example
/// ```rust
/// let item = get_or_not_found!(collection.get(&key), "Resource", format!("{}", key));
/// ```
#[macro_export]
macro_rules! get_or_not_found {
    ($expr:expr, $resource_type:expr, $resource_id:expr) => {
        $expr.ok_or_else(|| BlixardError::NotFound {
            resource: format!("{} with id: {}", $resource_type, $resource_id),
        })?
    };
}

/// Macro for safely accessing mutable collections with proper error context
/// 
/// # Example
/// ```rust
/// let item = get_mut_or_not_found!(collection.get_mut(&key), "Resource", format!("{}", key));
/// ```
#[macro_export]
macro_rules! get_mut_or_not_found {
    ($expr:expr, $resource_type:expr, $resource_id:expr) => {
        $expr.ok_or_else(|| BlixardError::NotFound {
            resource: format!("{} with id: {}", $resource_type, $resource_id),
        })?
    };
}

/// Macro for parsing strings with configuration error context
/// 
/// # Example
/// ```rust
/// let addr = parse_or_config_error!("127.0.0.1:8080".parse(), "address", "IP address");
/// ```
#[macro_export]
macro_rules! parse_or_config_error {
    ($expr:expr, $field:expr, $expected_type:expr) => {
        $expr.map_err(|e| BlixardError::InvalidConfiguration {
            message: format!("Invalid {} for field '{}': {}", $expected_type, $field, e),
        })?
    };
}

/// Macro for byte array conversions with serialization error context
/// 
/// # Example
/// ```rust
/// let bytes = try_into_bytes!(value.try_into(), "node_id", "8-byte array");
/// ```
#[macro_export]
macro_rules! try_into_bytes {
    ($expr:expr, $context:expr, $expected_format:expr) => {
        $expr.map_err(|_| BlixardError::Serialization {
            operation: format!("convert {} to {}", $context, $expected_format),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid byte format for {}", $context)
            )),
        })?
    };
}

/// Macro for serialization operations with proper error context
/// 
/// # Example
/// ```rust
/// let data = serialize_or_error!(bincode::serialize(&value), "worker capabilities");
/// ```
#[macro_export]
macro_rules! serialize_or_error {
    ($expr:expr, $data_description:expr) => {
        $expr.map_err(|e| BlixardError::Serialization {
            operation: format!("serialize {}", $data_description),
            source: Box::new(e),
        })?
    };
}

/// Macro for deserialization operations with proper error context
/// 
/// # Example
/// ```rust
/// let value = deserialize_or_error!(bincode::deserialize(&data), "worker capabilities");
/// ```
#[macro_export]
macro_rules! deserialize_or_error {
    ($expr:expr, $data_description:expr) => {
        $expr.map_err(|e| BlixardError::Serialization {
            operation: format!("deserialize {}", $data_description),
            source: Box::new(e),
        })?
    };
}

/// Macro for acquiring locks with poison handling
/// 
/// # Example
/// ```rust
/// let guard = acquire_lock!(mutex.lock(), "read cluster state");
/// ```
#[macro_export]
macro_rules! acquire_lock {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::LockPoisoned {
            operation: $operation.to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Lock poisoned: {}", e)
            )),
        })?
    };
}

/// Macro for acquiring read locks with poison handling
/// 
/// # Example
/// ```rust
/// let guard = acquire_read_lock!(rwlock.read(), "read shared state");
/// ```
#[macro_export]
macro_rules! acquire_read_lock {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::LockPoisoned {
            operation: format!("read lock for {}", $operation),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Read lock poisoned: {}", e)
            )),
        })?
    };
}

/// Macro for acquiring write locks with poison handling
/// 
/// # Example
/// ```rust
/// let guard = acquire_write_lock!(rwlock.write(), "write shared state");
/// ```
#[macro_export]
macro_rules! acquire_write_lock {
    ($expr:expr, $operation:expr) => {
        $expr.map_err(|e| BlixardError::LockPoisoned {
            operation: format!("write lock for {}", $operation),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Write lock poisoned: {}", e)
            )),
        })?
    };
}

/// Safe time since UNIX_EPOCH with fallback
/// 
/// Returns 0 if the system time is before UNIX_EPOCH (should never happen in practice).
pub fn time_since_epoch_safe() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs()
}

/// Safe random choice from a slice
/// 
/// Returns an error if the slice is empty instead of panicking.
pub fn choose_random<T>(slice: &[T]) -> Result<&T, BlixardError> {
    if slice.is_empty() {
        return Err(BlixardError::InvalidOperation {
            operation: "random selection".to_string(),
            reason: "Cannot choose from empty collection".to_string(),
        });
    }
    
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    Ok(slice.choose(&mut rng).expect("slice is not empty"))
}

/// Safe minimum selection with custom comparison
/// 
/// Returns an error if the iterator is empty instead of panicking.
pub fn min_by_safe<I, F, B>(iter: I, f: F) -> Result<I::Item, BlixardError>
where
    I: Iterator,
    F: FnMut(&I::Item) -> B,
    B: Ord,
{
    iter.min_by_key(f).ok_or_else(|| BlixardError::InvalidOperation {
        operation: "minimum selection".to_string(),
        reason: "Cannot find minimum in empty collection".to_string(),
    })
}

/// Safe maximum selection with custom comparison
/// 
/// Returns an error if the iterator is empty instead of panicking.
pub fn max_by_safe<I, F, B>(iter: I, f: F) -> Result<I::Item, BlixardError>
where
    I: Iterator,
    F: FnMut(&I::Item) -> B,
    B: Ord,
{
    iter.max_by_key(f).ok_or_else(|| BlixardError::InvalidOperation {
        operation: "maximum selection".to_string(),
        reason: "Cannot find maximum in empty collection".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_get_or_not_found_success() {
        let mut map = HashMap::new();
        map.insert("key", "value");
        
        let result = get_or_not_found!(map.get("key"), "Test", "key");
        assert_eq!(*result, "value");
    }

    #[test]
    fn test_get_or_not_found_failure() {
        let map: HashMap<&str, &str> = HashMap::new();
        
        let result = std::panic::catch_unwind(|| {
            get_or_not_found!(map.get("missing"), "Test", "missing")
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_time_since_epoch_safe() {
        let time = time_since_epoch_safe();
        // Should be a reasonable timestamp (after 2020)
        assert!(time > 1_600_000_000);
    }

    #[test]
    fn test_choose_random_success() {
        let slice = &[1, 2, 3, 4, 5];
        let result = choose_random(slice);
        assert!(result.is_ok());
        assert!(slice.contains(result.unwrap()));
    }

    #[test]
    fn test_choose_random_empty() {
        let slice: &[i32] = &[];
        let result = choose_random(slice);
        assert!(result.is_err());
        match result.unwrap_err() {
            BlixardError::InvalidOperation { operation, reason } => {
                assert_eq!(operation, "random selection");
                assert!(reason.contains("empty"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    #[test]
    fn test_min_by_safe_success() {
        let items = vec![("a", 3), ("b", 1), ("c", 2)];
        let result = min_by_safe(items.into_iter(), |(_, v)| *v);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ("b", 1));
    }

    #[test]
    fn test_min_by_safe_empty() {
        let items: Vec<(String, i32)> = vec![];
        let result = min_by_safe(items.into_iter(), |(_, v)| *v);
        assert!(result.is_err());
    }
}