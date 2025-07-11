//! Unified builder pattern traits for consistent object construction
//!
//! This module provides standardized builder patterns that are used throughout
//! the Blixard codebase for complex object construction, especially configuration objects.

use crate::error::BlixardError;
use async_trait::async_trait;

/// Unified builder trait for configuration objects
pub trait Builder<T> {
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Create a new builder with default values
    fn new() -> Self;
    
    /// Build the final object
    fn build(self) -> Result<T, Self::Error>;
    
    /// Validate the current builder state
    fn validate(&self) -> Result<(), Self::Error>;
    
    /// Reset builder to initial state
    fn reset(&mut self);
}

/// Async variant of the Builder trait for objects requiring async construction
#[async_trait]
pub trait AsyncBuilder<T>: Send {
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Create a new async builder with default values
    fn new() -> Self;
    
    /// Build the final object asynchronously
    async fn build(self) -> Result<T, Self::Error>;
    
    /// Validate the current builder state
    async fn validate(&self) -> Result<(), Self::Error>;
    
    /// Reset builder to initial state
    fn reset(&mut self);
}

/// Trait for objects that have a corresponding builder
pub trait HasBuilder {
    type Builder: Builder<Self> + Default;
    
    /// Get a builder for this type
    fn builder() -> Self::Builder {
        Self::Builder::default()
    }
}

/// Trait for objects that have a corresponding async builder
pub trait HasAsyncBuilder {
    type AsyncBuilder: AsyncBuilder<Self> + Default;
    
    /// Get an async builder for this type
    fn async_builder() -> Self::AsyncBuilder {
        Self::AsyncBuilder::default()
    }
}

/// Macro to generate builder patterns for configuration structs
#[macro_export]
macro_rules! config_builder {
    (
        $(#[$builder_attr:meta])*
        pub struct $builder_name:ident for $target:ident {
            $(
                $(#[$field_attr:meta])*
                $field:ident: $field_type:ty = $default:expr,
            )*
        }
        
        $(
            validate {
                $($validation:expr;)*
            }
        )?
    ) => {
        $(#[$builder_attr])*
        #[derive(Debug, Clone)]
        pub struct $builder_name {
            $(
                $field: Option<$field_type>,
            )*
        }

        impl $builder_name {
            /// Create a new builder with default values
            pub fn new() -> Self {
                Self {
                    $(
                        $field: None,
                    )*
                }
            }

            $(
                $(#[$field_attr])*
                pub fn $field(mut self, value: $field_type) -> Self {
                    self.$field = Some(value);
                    self
                }

                paste::paste! {
                    /// Get the current value for this field
                    pub fn [<get_ $field>](&self) -> Option<&$field_type> {
                        self.$field.as_ref()
                    }

                    /// Set the field value by reference
                    pub fn [<set_ $field>](&mut self, value: $field_type) -> &mut Self {
                        self.$field = Some(value);
                        self
                    }

                    /// Clear the field value
                    pub fn [<clear_ $field>](&mut self) -> &mut Self {
                        self.$field = None;
                        self
                    }
                }
            )*
        }

        impl Default for $builder_name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $crate::patterns::Builder<$target> for $builder_name {
            type Error = $crate::error::BlixardError;

            fn new() -> Self {
                Self::new()
            }

            fn build(self) -> Result<$target, Self::Error> {
                self.validate()?;
                
                Ok($target {
                    $(
                        $field: self.$field.unwrap_or_else(|| $default),
                    )*
                })
            }

            fn validate(&self) -> Result<(), Self::Error> {
                $($(
                    $validation
                )*)?
                Ok(())
            }

            fn reset(&mut self) {
                $(
                    self.$field = None;
                )*
            }
        }

        impl $crate::patterns::HasBuilder for $target {
            type Builder = $builder_name;
        }
    };
}

/// Macro for async builder patterns
#[macro_export]
macro_rules! async_config_builder {
    (
        $(#[$builder_attr:meta])*
        pub struct $builder_name:ident for $target:ident {
            $(
                $(#[$field_attr:meta])*
                $field:ident: $field_type:ty = $default:expr,
            )*
        }
        
        $(
            async_validate {
                $($validation:expr;)*
            }
        )?
    ) => {
        $(#[$builder_attr])*
        #[derive(Debug, Clone)]
        pub struct $builder_name {
            $(
                $field: Option<$field_type>,
            )*
        }

        impl $builder_name {
            /// Create a new async builder with default values
            pub fn new() -> Self {
                Self {
                    $(
                        $field: None,
                    )*
                }
            }

            $(
                $(#[$field_attr])*
                pub fn $field(mut self, value: $field_type) -> Self {
                    self.$field = Some(value);
                    self
                }
            )*
        }

        impl Default for $builder_name {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait::async_trait]
        impl $crate::patterns::AsyncBuilder<$target> for $builder_name {
            type Error = $crate::error::BlixardError;

            fn new() -> Self {
                Self::new()
            }

            async fn build(self) -> Result<$target, Self::Error> {
                self.validate().await?;
                
                Ok($target {
                    $(
                        $field: self.$field.unwrap_or_else(|| $default),
                    )*
                })
            }

            async fn validate(&self) -> Result<(), Self::Error> {
                $($(
                    $validation
                )*)?
                Ok(())
            }

            fn reset(&mut self) {
                $(
                    self.$field = None;
                )*
            }
        }

        impl $crate::patterns::HasAsyncBuilder for $target {
            type AsyncBuilder = $builder_name;
        }
    };
}

/// Builder for collections with validation
pub struct CollectionBuilder<T> {
    items: Vec<T>,
    capacity_limit: Option<usize>,
    validator: Option<fn(&T) -> Result<(), BlixardError>>,
}

impl<T> CollectionBuilder<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            capacity_limit: None,
            validator: None,
        }
    }

    pub fn with_capacity_limit(mut self, limit: usize) -> Self {
        self.capacity_limit = Some(limit);
        self
    }

    pub fn with_validator(mut self, validator: fn(&T) -> Result<(), BlixardError>) -> Self {
        self.validator = Some(validator);
        self
    }

    pub fn add(mut self, item: T) -> Result<Self, BlixardError> {
        if let Some(limit) = self.capacity_limit {
            if self.items.len() >= limit {
                return Err(BlixardError::Validation {
                    field: "collection".to_string(),
                    message: format!("Collection capacity limit {} exceeded", limit),
                });
            }
        }

        if let Some(validator) = self.validator {
            validator(&item)?;
        }

        self.items.push(item);
        Ok(self)
    }

    pub fn extend<I: IntoIterator<Item = T>>(mut self, items: I) -> Result<Self, BlixardError> {
        for item in items {
            self = self.add(item)?;
        }
        Ok(self)
    }
}

impl<T> Builder<Vec<T>> for CollectionBuilder<T> {
    type Error = BlixardError;

    fn new() -> Self {
        Self::new()
    }

    fn build(self) -> Result<Vec<T>, Self::Error> {
        Ok(self.items)
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if let Some(validator) = self.validator {
            for item in &self.items {
                validator(item)?;
            }
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for CollectionBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test configuration struct
    #[derive(Debug, PartialEq)]
    struct TestConfig {
        name: String,
        port: u16,
        enabled: bool,
        timeout: std::time::Duration,
    }

    // Generate builder using macro
    config_builder! {
        /// Builder for TestConfig
        pub struct TestConfigBuilder for TestConfig {
            /// The name of the service
            name: String = "default".to_string(),
            /// The port to listen on  
            port: u16 = 8080,
            /// Whether the service is enabled
            enabled: bool = true,
            /// Request timeout duration
            timeout: std::time::Duration = std::time::Duration::from_secs(30),
        }
        
        validate {
            if self.port.unwrap_or(8080) == 0 {
                return Err(BlixardError::Validation {
                    field: "port".to_string(),
                    message: "Port cannot be 0".to_string(),
                });
            };
        }
    }

    #[test]
    fn test_builder_basic_usage() {
        let config = TestConfig::builder()
            .name("test-service".to_string())
            .port(9090)
            .build()
            .unwrap();

        assert_eq!(config.name, "test-service");
        assert_eq!(config.port, 9090);
        assert_eq!(config.enabled, true); // default value
        assert_eq!(config.timeout, std::time::Duration::from_secs(30)); // default value
    }

    #[test]
    fn test_builder_validation() {
        let result = TestConfig::builder()
            .port(0) // Invalid port
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_builder_reset() {
        let mut builder = TestConfig::builder()
            .name("test".to_string())
            .port(3000);

        builder.reset();
        
        let config = builder.build().unwrap();
        assert_eq!(config.name, "default"); // Should use default after reset
        assert_eq!(config.port, 8080); // Should use default after reset
    }

    #[test]
    fn test_collection_builder() {
        let numbers = CollectionBuilder::new()
            .with_capacity_limit(3)
            .add(1).unwrap()
            .add(2).unwrap()
            .add(3).unwrap()
            .build()
            .unwrap();

        assert_eq!(numbers, vec![1, 2, 3]);

        // Test capacity limit
        let result = CollectionBuilder::new()
            .with_capacity_limit(2)
            .add(1).unwrap()
            .add(2).unwrap()
            .add(3); // Should fail

        assert!(result.is_err());
    }

    #[test]
    fn test_collection_builder_with_validator() {
        let validator = |x: &i32| {
            if *x < 0 {
                Err(BlixardError::Validation {
                    field: "number".to_string(),
                    message: "Number must be positive".to_string(),
                })
            } else {
                Ok(())
            }
        };

        let result = CollectionBuilder::new()
            .with_validator(validator)
            .add(1).unwrap()
            .add(-1); // Should fail validation

        assert!(result.is_err());
    }
}