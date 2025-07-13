//! Compile-time validation and assertion utilities
//!
//! This module provides macros and functions for performing validation
//! at compile time, catching errors before runtime.

/// Const assertion that fails compilation if condition is false
#[macro_export]
macro_rules! const_assert {
    ($cond:expr) => {
        const _: () = assert!($cond);
    };
    ($cond:expr, $msg:literal) => {
        const _: () = assert!($cond, $msg);
    };
}

/// Validate a value at compile time
pub const fn const_validate<T>(value: T, condition: bool) -> T {
    assert!(condition, "Validation failed");
    value
}

/// Const function to check if a string is valid UTF-8
pub const fn is_valid_utf8(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        
        // ASCII character (0xxxxxxx)
        if byte < 0x80 {
            i += 1;
            continue;
        }
        
        // Multi-byte character
        let char_len = if byte < 0xE0 {
            // 2-byte character (110xxxxx 10xxxxxx)
            if byte < 0xC2 {
                return false; // Invalid start byte
            }
            2
        } else if byte < 0xF0 {
            // 3-byte character (1110xxxx 10xxxxxx 10xxxxxx)
            3
        } else if byte < 0xF8 {
            // 4-byte character (11110xxx 10xxxxxx 10xxxxxx 10xxxxxx)
            4
        } else {
            return false; // Invalid start byte
        };
        
        // Check if we have enough bytes
        if i + char_len > bytes.len() {
            return false;
        }
        
        // Check continuation bytes
        let mut j = 1;
        while j < char_len {
            if bytes[i + j] & 0xC0 != 0x80 {
                return false; // Invalid continuation byte
            }
            j += 1;
        }
        
        i += char_len;
    }
    
    true
}

/// Const function to validate identifier format
pub const fn is_valid_identifier(s: &str) -> bool {
    let bytes = s.as_bytes();
    
    if bytes.is_empty() {
        return false;
    }
    
    // First character must be letter or underscore
    let first = bytes[0];
    if !((first >= b'a' && first <= b'z') || 
         (first >= b'A' && first <= b'Z') || 
         first == b'_') {
        return false;
    }
    
    // Remaining characters must be alphanumeric or underscore
    let mut i = 1;
    while i < bytes.len() {
        let byte = bytes[i];
        if !((byte >= b'a' && byte <= b'z') || 
             (byte >= b'A' && byte <= b'Z') || 
             (byte >= b'0' && byte <= b'9') || 
             byte == b'_') {
            return false;
        }
        i += 1;
    }
    
    true
}

/// Const function to validate semver format (basic check)
pub const fn is_valid_semver(s: &str) -> bool {
    let bytes = s.as_bytes();
    
    if bytes.is_empty() {
        return false;
    }
    
    let mut dots = 0;
    let mut has_digit = false;
    let mut i = 0;
    
    while i < bytes.len() {
        let byte = bytes[i];
        
        if byte == b'.' {
            if !has_digit {
                return false; // No digit before dot
            }
            dots += 1;
            if dots > 2 {
                return false; // Too many dots
            }
            has_digit = false;
        } else if byte >= b'0' && byte <= b'9' {
            has_digit = true;
        } else {
            // Allow some additional characters for pre-release versions
            if !(byte == b'-' || byte == b'+' || 
                 (byte >= b'a' && byte <= b'z') || 
                 (byte >= b'A' && byte <= b'Z')) {
                return false;
            }
        }
        
        i += 1;
    }
    
    dots == 2 && has_digit
}

/// Check if a number is within range (for basic numeric types)
pub const fn is_in_range_i64(value: i64, min: i64, max: i64) -> bool {
    value >= min && value <= max
}

pub const fn is_in_range_u64(value: u64, min: u64, max: u64) -> bool {
    value >= min && value <= max
}

pub const fn is_in_range_usize(value: usize, min: usize, max: usize) -> bool {
    value >= min && value <= max
}

/// Macro for compile-time string validation
#[macro_export]
macro_rules! validate_const_str {
    ($s:expr, identifier) => {
        const _: () = assert!(
            $crate::zero_cost::compile_time_validation::is_valid_identifier($s),
            "Invalid identifier format"
        );
    };
    ($s:expr, utf8) => {
        const _: () = assert!(
            $crate::zero_cost::compile_time_validation::is_valid_utf8($s.as_bytes()),
            "Invalid UTF-8 string"
        );
    };
    ($s:expr, semver) => {
        const _: () = assert!(
            $crate::zero_cost::compile_time_validation::is_valid_semver($s),
            "Invalid semver format"
        );
    };
}

/// Macro for compile-time range validation
#[macro_export]
macro_rules! validate_range {
    ($value:expr, $min:expr, $max:expr) => {
        const _: () = assert!(
            $crate::zero_cost::compile_time_validation::is_in_range($value, $min, $max),
            "Value out of range"
        );
    };
}

/// Validated configuration struct that ensures compile-time correctness
pub struct ValidatedConfig<const MAX_SIZE: usize> {
    data: [u8; MAX_SIZE],
    size: usize,
    version: &'static str,
}

impl<const MAX_SIZE: usize> ValidatedConfig<MAX_SIZE> {
    /// Create a new validated configuration
    pub const fn new(version: &'static str) -> Self {
        Self {
            data: [0; MAX_SIZE],
            size: 0,
            version,
        }
    }

    /// Add data to the configuration
    pub const fn with_data(mut self, data: &[u8]) -> Self {
        assert!(data.len() <= MAX_SIZE, "Data too large");
        
        let mut i = 0;
        while i < data.len() {
            self.data[i] = data[i];
            i += 1;
        }
        self.size = data.len();
        
        self
    }

    /// Get the version
    pub const fn version(&self) -> &'static str {
        self.version
    }

    /// Get the data
    pub const fn data(&self) -> &[u8] {
        // This is a bit tricky in const context, but we can work around it
        unsafe {
            std::slice::from_raw_parts(self.data.as_ptr(), self.size)
        }
    }

    /// Get the maximum size
    pub const fn max_size(&self) -> usize {
        MAX_SIZE
    }

    /// Get the current size
    pub const fn size(&self) -> usize {
        self.size
    }
}

/// Type-level validation for struct fields
#[macro_export]
macro_rules! validated_struct {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident: $field_ty:ty = $validator:expr,
            )*
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: $field_ty,
            )*
        }

        impl $name {
            /// Create a new instance with validation
            $vis const fn new($($field: $field_ty),*) -> Self {
                $(
                    const _: () = assert!($validator(&$field), "Field validation failed");
                )*
                
                Self {
                    $($field),*
                }
            }
        }
    };
}

/// Macro for creating compile-time lookup tables
#[macro_export]
macro_rules! const_lookup {
    (
        $name:ident: [$key_ty:ty; $value_ty:ty] = [
            $(($key:expr, $value:expr)),* $(,)?
        ]
    ) => {
        pub struct $name;

        impl $name {
            const ENTRIES: &'static [($key_ty, $value_ty)] = &[
                $(($key, $value)),*
            ];

            /// Lookup a value by key at compile time (linear search)
            pub const fn lookup(key: $key_ty) -> Option<$value_ty>
            where
                $key_ty: PartialEq + Copy,
                $value_ty: Copy,
            {
                let mut i = 0;
                while i < Self::ENTRIES.len() {
                    if Self::ENTRIES[i].0 == key {
                        return Some(Self::ENTRIES[i].1);
                    }
                    i += 1;
                }
                None
            }

            /// Get all entries
            pub const fn entries() -> &'static [($key_ty, $value_ty)] {
                Self::ENTRIES
            }

            /// Get the number of entries
            pub const fn len() -> usize {
                Self::ENTRIES.len()
            }
        }
    };
}

/// Example validated identifiers
pub mod identifiers {
    use super::*;

    /// A compile-time validated identifier
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ValidatedIdentifier {
        s: &'static str,
    }

    impl ValidatedIdentifier {
        /// Create a new validated identifier
        pub const fn new(s: &'static str) -> Self {
            assert!(is_valid_identifier(s), "Invalid identifier");
            Self { s }
        }

        /// Get the identifier string
        pub const fn as_str(&self) -> &'static str {
            self.s
        }
    }

    // Example usage
    pub const NAME_FIELD: ValidatedIdentifier = ValidatedIdentifier::new("user_name");
    pub const AGE_FIELD: ValidatedIdentifier = ValidatedIdentifier::new("user_age");
    // This would fail to compile:
    // pub const INVALID_FIELD: ValidatedIdentifier = ValidatedIdentifier::new("123-invalid");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_validation() {
        // These pass validation
        const _: () = assert!(is_valid_identifier("valid_identifier"));
        const _: () = assert!(is_valid_identifier("_also_valid"));
        const _: () = assert!(is_valid_identifier("ValidCamelCase"));
        
        // These would fail (commented out to avoid compilation errors)
        // const _: () = assert!(is_valid_identifier("123invalid"));
        // const _: () = assert!(is_valid_identifier("invalid-name"));
        
        const _: () = assert!(is_valid_semver("1.0.0"));
        const _: () = assert!(is_valid_semver("2.1.3-alpha"));
        const _: () = assert!(is_valid_semver("1.0.0+build.1"));
        
        const _: () = assert!(is_in_range(50, 0, 100));
        const _: () = assert!(!is_in_range(150, 0, 100));
    }

    #[test]
    fn test_validated_config() {
        const CONFIG: ValidatedConfig<1024> = ValidatedConfig::new("1.0.0")
            .with_data(b"test data");
        
        assert_eq!(CONFIG.version(), "1.0.0");
        assert_eq!(CONFIG.size(), 9);
        assert_eq!(CONFIG.max_size(), 1024);
    }

    #[test]
    fn test_validated_identifiers() {
        use identifiers::*;
        
        let name_field = NAME_FIELD;
        assert_eq!(name_field.as_str(), "user_name");
        
        let age_field = AGE_FIELD;
        assert_eq!(age_field.as_str(), "user_age");
    }

    // Example of const lookup usage
    const_lookup!(
        HttpStatusCodes: [u16; &'static str] = [
            (200, "OK"),
            (404, "Not Found"),
            (500, "Internal Server Error"),
        ]
    );

    #[test]
    fn test_const_lookup() {
        assert_eq!(HttpStatusCodes::lookup(200), Some("OK"));
        assert_eq!(HttpStatusCodes::lookup(404), Some("Not Found"));
        assert_eq!(HttpStatusCodes::lookup(999), None);
        assert_eq!(HttpStatusCodes::len(), 3);
    }

    // Test the macro usage
    validate_const_str!("valid_name", identifier);
    validate_const_str!("1.2.3", semver);
    validate_range!(42, 0, 100);

    // Example validated struct
    validated_struct!(
        #[derive(Debug)]
        pub struct ServerConfig {
            pub port: u16 = |&p| p > 1000 && p < 65536,
            pub max_connections: u32 = |&c| c > 0 && c <= 10000,
        }
    );

    #[test]
    fn test_validated_struct() {
        const CONFIG: ServerConfig = ServerConfig::new(8080, 1000);
        assert_eq!(CONFIG.port, 8080);
        assert_eq!(CONFIG.max_connections, 1000);

        // This would fail to compile:
        // const INVALID: ServerConfig = ServerConfig::new(0, 20000);
    }
}