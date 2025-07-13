//! Validated types that ensure invariants at compile time
//!
//! This module provides zero-cost newtypes that validate their contents,
//! moving runtime validation to compile time where possible.

use std::fmt;
use std::str::FromStr;

/// Non-empty collection wrapper
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<Vec<T>> {
    /// Create a non-empty vector
    pub fn new(vec: Vec<T>) -> Result<Self, Vec<T>> {
        if vec.is_empty() {
            Err(vec)
        } else {
            Ok(NonEmpty {
                head: vec,
                tail: Vec::new(),
            })
        }
    }
}

impl<T> NonEmpty<T> {
    /// Create from head and tail
    pub fn from_parts(head: T, tail: Vec<T>) -> Self {
        Self { head, tail }
    }

    /// Get the head element
    pub fn head(&self) -> &T {
        &self.head
    }

    /// Get the tail elements
    pub fn tail(&self) -> &[T] {
        &self.tail
    }

    /// Get the length (always >= 1)
    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    /// Convert to vector
    pub fn into_vec(self) -> Vec<T> {
        let mut vec = vec![self.head];
        vec.extend(self.tail);
        vec
    }

    /// Iterate over all elements
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.head).chain(self.tail.iter())
    }

    /// Map over all elements
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> NonEmpty<U> {
        NonEmpty {
            head: f(self.head),
            tail: self.tail.into_iter().map(f).collect(),
        }
    }

    /// Push an element
    pub fn push(&mut self, value: T) {
        self.tail.push(value);
    }
}

/// Positive integer wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Positive<T> {
    value: T,
}

macro_rules! impl_positive {
    ($($t:ty)*) => {$(
        impl Positive<$t> {
            /// Create a new positive number
            pub const fn new(value: $t) -> Result<Self, &'static str> {
                if value > 0 {
                    Ok(Self { value })
                } else {
                    Err("Value must be positive")
                }
            }

            /// Create without checking (const context)
            pub const fn new_unchecked(value: $t) -> Self {
                assert!(value > 0, "Value must be positive");
                Self { value }
            }

            /// Get the inner value
            pub const fn get(&self) -> $t {
                self.value
            }

            /// Saturating add that maintains positivity
            pub const fn saturating_add(self, other: Self) -> Self {
                Self {
                    value: self.value.saturating_add(other.value),
                }
            }

            /// Checked subtraction that maintains positivity
            pub const fn checked_sub(self, other: Self) -> Option<Self> {
                if self.value > other.value {
                    Some(Self {
                        value: self.value - other.value,
                    })
                } else {
                    None
                }
            }
        }

        impl std::ops::Deref for Positive<$t> {
            type Target = $t;

            fn deref(&self) -> &Self::Target {
                &self.value
            }
        }

        impl fmt::Display for Positive<$t> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.value.fmt(f)
            }
        }

        impl FromStr for Positive<$t> {
            type Err = &'static str;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.parse::<$t>() {
                    Ok(value) => Self::new(value),
                    Err(_) => Err("Invalid number"),
                }
            }
        }
    )*}
}

impl_positive!(u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize);

/// Validated string with custom validation
pub struct ValidatedString<V: Validator> {
    value: String,
    _validator: std::marker::PhantomData<V>,
}

/// Trait for string validators
pub trait Validator {
    /// Validate the string
    fn validate(s: &str) -> bool;

    /// Error message for validation failure
    fn error_message() -> &'static str;
}

impl<V: Validator> ValidatedString<V> {
    /// Create a new validated string
    pub fn new(value: String) -> Result<Self, &'static str> {
        if V::validate(&value) {
            Ok(Self {
                value,
                _validator: std::marker::PhantomData,
            })
        } else {
            Err(V::error_message())
        }
    }

    /// Get the inner string
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Extract the inner string
    pub fn into_inner(self) -> String {
        self.value
    }
}

impl<V: Validator> std::ops::Deref for ValidatedString<V> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<V: Validator> fmt::Display for ValidatedString<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<V: Validator> FromStr for ValidatedString<V> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

/// Example validators
pub mod validators {
    use super::Validator;

    pub struct NonEmptyString;
    impl Validator for NonEmptyString {
        fn validate(s: &str) -> bool {
            !s.is_empty()
        }

        fn error_message() -> &'static str {
            "String must not be empty"
        }
    }

    pub struct AlphanumericString;
    impl Validator for AlphanumericString {
        fn validate(s: &str) -> bool {
            !s.is_empty() && s.chars().all(|c| c.is_alphanumeric())
        }

        fn error_message() -> &'static str {
            "String must be non-empty and alphanumeric"
        }
    }

    pub struct EmailString;
    impl Validator for EmailString {
        fn validate(s: &str) -> bool {
            // Simple email validation
            s.contains('@') && s.contains('.') && s.len() >= 5
        }

        fn error_message() -> &'static str {
            "Invalid email format"
        }
    }

    pub struct MaxLength<const N: usize>;
    impl<const N: usize> Validator for MaxLength<N> {
        fn validate(s: &str) -> bool {
            s.len() <= N
        }

        fn error_message() -> &'static str {
            "String exceeds maximum length"
        }
    }
}

/// Bounded integer with compile-time bounds
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedInt<const MIN: i64, const MAX: i64> {
    value: i64,
}

impl<const MIN: i64, const MAX: i64> BoundedInt<MIN, MAX> {
    /// Create a new bounded integer
    pub const fn new(value: i64) -> Result<Self, &'static str> {
        if value >= MIN && value <= MAX {
            Ok(Self { value })
        } else {
            Err("Value out of bounds")
        }
    }

    /// Create without checking (const context)
    pub const fn new_unchecked(value: i64) -> Self {
        assert!(value >= MIN && value <= MAX, "Value out of bounds");
        Self { value }
    }

    /// Get the value
    pub const fn get(&self) -> i64 {
        self.value
    }

    /// Get the minimum bound
    pub const fn min_bound() -> i64 {
        MIN
    }

    /// Get the maximum bound
    pub const fn max_bound() -> i64 {
        MAX
    }

    /// Clamp a value to the bounds
    pub const fn clamp(value: i64) -> Self {
        let clamped = if value < MIN {
            MIN
        } else if value > MAX {
            MAX
        } else {
            value
        };
        Self { value: clamped }
    }

    /// Saturating add
    pub const fn saturating_add(self, other: i64) -> Self {
        Self::clamp(self.value.saturating_add(other))
    }

    /// Saturating subtract
    pub const fn saturating_sub(self, other: i64) -> Self {
        Self::clamp(self.value.saturating_sub(other))
    }
}

impl<const MIN: i64, const MAX: i64> std::ops::Deref for BoundedInt<MIN, MAX> {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<const MIN: i64, const MAX: i64> fmt::Display for BoundedInt<MIN, MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<const MIN: i64, const MAX: i64> FromStr for BoundedInt<MIN, MAX> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i64>() {
            Ok(value) => Self::new(value),
            Err(_) => Err("Invalid number"),
        }
    }
}

// Type aliases for common bounded integers
pub type Percentage = BoundedInt<0, 100>;
pub type Port = BoundedInt<1, 65535>;
pub type Priority = BoundedInt<0, 255>;
pub type HttpStatusCode = BoundedInt<100, 599>;

// Type alias for common validated strings
pub type NonEmptyString = ValidatedString<validators::NonEmptyString>;
pub type AlphanumericString = ValidatedString<validators::AlphanumericString>;
pub type EmailString = ValidatedString<validators::EmailString>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_non_empty() {
        let ne = NonEmpty::from_parts(1, vec![2, 3, 4]);
        assert_eq!(ne.len(), 4);
        assert_eq!(ne.head(), &1);
        assert_eq!(ne.tail(), &[2, 3, 4]);

        let vec = ne.into_vec();
        assert_eq!(vec, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_positive() {
        let p1 = Positive::new(42u32).unwrap();
        let p2 = Positive::new(8u32).unwrap();

        assert_eq!(*p1, 42);

        let sum = p1.saturating_add(p2);
        assert_eq!(*sum, 50);

        let diff = p1.checked_sub(p2).unwrap();
        assert_eq!(*diff, 34);

        assert!(p2.checked_sub(p1).is_none());

        assert!(Positive::new(0u32).is_err());
    }

    #[test]
    fn test_validated_string() {
        let email = EmailString::new("user@example.com".to_string()).unwrap();
        assert_eq!(email.as_str(), "user@example.com");

        assert!(EmailString::new("invalid".to_string()).is_err());

        let alphanum = AlphanumericString::new("Test123".to_string()).unwrap();
        assert_eq!(alphanum.as_str(), "Test123");

        assert!(AlphanumericString::new("Test 123".to_string()).is_err());
    }

    #[test]
    fn test_bounded_int() {
        let percentage: Percentage = BoundedInt::new(50).unwrap();
        assert_eq!(*percentage, 50);

        let clamped = Percentage::clamp(150);
        assert_eq!(*clamped, 100);

        let port: Port = BoundedInt::new(8080).unwrap();
        assert_eq!(*port, 8080);

        assert!(Port::new(70000).is_err());

        let p1 = Percentage::new(80).unwrap();
        let p2 = p1.saturating_add(30);
        assert_eq!(*p2, 100); // Clamped to max
    }

    #[test]
    fn test_const_functions() {
        const POS: Positive<u32> = Positive::new_unchecked(42);
        assert_eq!(*POS, 42);

        const BOUNDED: BoundedInt<0, 100> = BoundedInt::new_unchecked(50);
        assert_eq!(*BOUNDED, 50);

        const CLAMPED: BoundedInt<0, 100> = BoundedInt::clamp(200);
        assert_eq!(*CLAMPED, 100);
    }
}