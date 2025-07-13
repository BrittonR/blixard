//! Phantom types for compile-time constraints without runtime overhead
//!
//! This module provides phantom type patterns that add compile-time safety
//! without any runtime cost.

use std::marker::PhantomData;

/// Marker trait for branded types
pub trait Brand: private::Sealed {
    /// The brand name for debugging
    const BRAND: &'static str;
}

mod private {
    pub trait Sealed {}
}

/// A branded type that ensures values from different sources aren't mixed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Branded<T, B: Brand> {
    value: T,
    _brand: PhantomData<B>,
}

impl<T, B: Brand> Branded<T, B> {
    /// Create a new branded value
    pub fn new(value: T) -> Self {
        Self {
            value,
            _brand: PhantomData,
        }
    }

    /// Extract the inner value (consuming the brand)
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get a reference to the inner value
    pub fn as_inner(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the inner value
    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Get the brand name
    pub fn brand_name(&self) -> &'static str {
        B::BRAND
    }
}

/// Phantom type to make a type !Send
#[derive(Debug)]
pub struct NotSend(PhantomData<*const ()>);

impl NotSend {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

/// Phantom type to make a type !Sync
#[derive(Debug)]
pub struct NotSync(PhantomData<std::cell::Cell<()>>);

impl NotSync {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

/// Phantom state for zero-cost state tracking
pub struct PhantomState<S> {
    _state: PhantomData<S>,
}

impl<S> PhantomState<S> {
    pub const fn new() -> Self {
        Self {
            _state: PhantomData,
        }
    }
}

/// Example: Units of measurement with phantom types
pub mod units {
    use super::*;

    // Define unit types
    pub struct Meters;
    pub struct Feet;
    pub struct Celsius;
    pub struct Fahrenheit;
    #[derive(Debug, Clone, Copy)]
    pub struct Bytes;
    #[derive(Debug, Clone, Copy)]
    pub struct Kilobytes;
    #[derive(Debug, Clone, Copy)]
    pub struct Megabytes;
    #[derive(Debug, Clone, Copy)]
    pub struct Gigabytes;

    /// A measurement with a phantom unit type
    #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
    pub struct Measurement<T, U> {
        value: T,
        _unit: PhantomData<U>,
    }

    impl<T, U> Measurement<T, U> {
        /// Create a new measurement
        pub const fn new(value: T) -> Self {
            Self {
                value,
                _unit: PhantomData,
            }
        }

        /// Get the value
        pub const fn value(&self) -> &T {
            &self.value
        }

        /// Extract the value
        pub fn into_value(self) -> T {
            self.value
        }
    }

    // Arithmetic operations for same units
    impl<T: std::ops::Add<Output = T>, U> std::ops::Add for Measurement<T, U> {
        type Output = Self;

        fn add(self, other: Self) -> Self::Output {
            Self::new(self.value + other.value)
        }
    }

    impl<T: std::ops::Sub<Output = T>, U> std::ops::Sub for Measurement<T, U> {
        type Output = Self;

        fn sub(self, other: Self) -> Self::Output {
            Self::new(self.value - other.value)
        }
    }

    // Conversions between units
    impl Measurement<f64, Meters> {
        /// Convert meters to feet
        pub fn to_feet(self) -> Measurement<f64, Feet> {
            Measurement::new(self.value * 3.28084)
        }
    }

    impl Measurement<f64, Feet> {
        /// Convert feet to meters
        pub fn to_meters(self) -> Measurement<f64, Meters> {
            Measurement::new(self.value / 3.28084)
        }
    }

    impl Measurement<f64, Celsius> {
        /// Convert Celsius to Fahrenheit
        pub fn to_fahrenheit(self) -> Measurement<f64, Fahrenheit> {
            Measurement::new(self.value * 9.0 / 5.0 + 32.0)
        }
    }

    impl Measurement<f64, Fahrenheit> {
        /// Convert Fahrenheit to Celsius
        pub fn to_celsius(self) -> Measurement<f64, Celsius> {
            Measurement::new((self.value - 32.0) * 5.0 / 9.0)
        }
    }

    impl Measurement<u64, Bytes> {
        /// Convert bytes to kilobytes
        pub fn to_kilobytes(self) -> Measurement<u64, Kilobytes> {
            Measurement::new(self.value / 1024)
        }

        /// Convert bytes to megabytes
        pub fn to_megabytes(self) -> Measurement<u64, Megabytes> {
            Measurement::new(self.value / (1024 * 1024))
        }
    }

    impl Measurement<u64, Kilobytes> {
        /// Convert kilobytes to bytes
        pub fn to_bytes(self) -> Measurement<u64, Bytes> {
            Measurement::new(self.value * 1024)
        }

        /// Convert kilobytes to megabytes
        pub fn to_megabytes(self) -> Measurement<u64, Megabytes> {
            Measurement::new(self.value / 1024)
        }
    }

    impl Measurement<u64, Megabytes> {
        /// Convert megabytes to bytes
        pub fn to_bytes(self) -> Measurement<u64, Bytes> {
            Measurement::new(self.value * 1024 * 1024)
        }

        /// Convert megabytes to kilobytes
        pub fn to_kilobytes(self) -> Measurement<u64, Kilobytes> {
            Measurement::new(self.value * 1024)
        }
    }

    impl Measurement<u64, Gigabytes> {
        /// Convert gigabytes to bytes
        pub fn to_bytes(self) -> Measurement<u64, Bytes> {
            Measurement::new(self.value * 1024 * 1024 * 1024)
        }

        /// Convert gigabytes to megabytes
        pub fn to_megabytes(self) -> Measurement<u64, Megabytes> {
            Measurement::new(self.value * 1024)
        }
    }

    // Type aliases for convenience
    pub type Distance<T> = Measurement<T, Meters>;
    pub type Temperature<T> = Measurement<T, Celsius>;
    pub type Memory<T> = Measurement<T, Bytes>;
}

/// Example: Database ID branding
pub mod database {
    use super::*;

    // Define brands for different databases
    pub struct PostgreSQL;
    pub struct MySQL;
    pub struct Redis;

    impl private::Sealed for PostgreSQL {}
    impl Brand for PostgreSQL {
        const BRAND: &'static str = "PostgreSQL";
    }

    impl private::Sealed for MySQL {}
    impl Brand for MySQL {
        const BRAND: &'static str = "MySQL";
    }

    impl private::Sealed for Redis {}
    impl Brand for Redis {
        const BRAND: &'static str = "Redis";
    }

    // Type aliases for branded IDs
    pub type PostgresId = Branded<u64, PostgreSQL>;
    pub type MySQLId = Branded<u64, MySQL>;
    pub type RedisKey = Branded<String, Redis>;

    // Example functions that only accept specific branded types
    pub fn query_postgres(id: PostgresId) -> String {
        format!("SELECT * FROM users WHERE id = {}", id.as_inner())
    }

    pub fn query_mysql(id: MySQLId) -> String {
        format!("SELECT * FROM users WHERE id = {}", id.as_inner())
    }

    pub fn get_redis_value(key: RedisKey) -> String {
        format!("GET {}", key.as_inner())
    }
}

/// Environment brands for VM identifiers
pub mod environment {
    use super::*;

    pub struct Production;
    pub struct Development;
    pub struct Testing;

    impl private::Sealed for Production {}
    impl Brand for Production {
        const BRAND: &'static str = "Production";
    }

    impl private::Sealed for Development {}
    impl Brand for Development {
        const BRAND: &'static str = "Development";
    }

    impl private::Sealed for Testing {}
    impl Brand for Testing {
        const BRAND: &'static str = "Testing";
    }

    // Type aliases for environment-specific VM IDs
    pub type ProductionVmId = Branded<u64, Production>;
    pub type DevelopmentVmId = Branded<u64, Development>;
    pub type TestingVmId = Branded<u64, Testing>;
}

/// Example: Ownership phantom types
pub mod ownership {
    use super::*;

    pub struct Owned;
    pub struct Borrowed;
    pub struct Shared;

    /// Resource with phantom ownership tracking
    pub struct Resource<T, O> {
        data: T,
        _ownership: PhantomData<O>,
    }

    impl<T> Resource<T, Owned> {
        pub fn new(data: T) -> Self {
            Self {
                data,
                _ownership: PhantomData,
            }
        }

        /// Transfer ownership
        pub fn transfer(self) -> Resource<T, Owned> {
            Resource {
                data: self.data,
                _ownership: PhantomData,
            }
        }

        /// Borrow the resource
        pub fn borrow(&self) -> Resource<&T, Borrowed> {
            Resource {
                data: &self.data,
                _ownership: PhantomData,
            }
        }

        /// Share the resource
        pub fn share(self) -> Resource<std::sync::Arc<T>, Shared> {
            Resource {
                data: std::sync::Arc::new(self.data),
                _ownership: PhantomData,
            }
        }
    }

    impl<T> Resource<&T, Borrowed> {
        /// Can only read borrowed resources
        pub fn read(&self) -> &T {
            self.data
        }
    }

    impl<T> Resource<std::sync::Arc<T>, Shared> {
        /// Clone a shared resource
        pub fn clone(&self) -> Self {
            Self {
                data: self.data.clone(),
                _ownership: PhantomData,
            }
        }

        /// Read a shared resource
        pub fn read(&self) -> &T {
            &self.data
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branded_types() {
        use database::*;

        let postgres_id = PostgresId::new(123);
        let mysql_id = MySQLId::new(456);

        // These functions only accept the correct branded type
        let _query1 = query_postgres(postgres_id);
        let _query2 = query_mysql(mysql_id);

        // This would not compile:
        // let _query3 = query_postgres(mysql_id); // Error: expected PostgresId, found MySQLId
    }

    #[test]
    fn test_units() {
        use units::*;

        let distance1 = Measurement::<f64, Meters>::new(100.0);
        let distance2 = Measurement::<f64, Meters>::new(50.0);

        // Can add measurements of the same unit
        let total = distance1 + distance2;
        assert_eq!(*total.value(), 150.0);

        // Convert between units
        let in_feet = distance1.to_feet();
        assert!((in_feet.value() - 328.084).abs() < 0.001);

        // This would not compile:
        // let invalid = distance1 + in_feet; // Error: cannot add Meters and Feet
    }

    #[test]
    fn test_ownership() {
        use ownership::*;

        let owned = Resource::new(vec![1, 2, 3]);
        let borrowed = owned.borrow();
        assert_eq!(borrowed.read(), &vec![1, 2, 3]);

        let shared = owned.share();
        let shared2 = shared.clone();
        assert_eq!(shared.read(), &vec![1, 2, 3]);
        assert_eq!(shared2.read(), &vec![1, 2, 3]);
    }
}