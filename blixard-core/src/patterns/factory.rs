//! Unified factory pattern traits for consistent object creation
//!
//! This module provides standardized factory patterns that are used throughout
//! the Blixard codebase for object creation with dependency injection and registration.

use crate::error::{BlixardError, BlixardResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Unified factory trait for object creation
pub trait Factory<T>: Send + Sync {
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Create an object with the given configuration
    fn create(&self, config: Self::Config) -> Result<T, Self::Error>;
    
    /// Check if this factory can create an object with the given configuration
    fn can_create(&self, config: &Self::Config) -> bool;
    
    /// Get the factory type identifier
    fn factory_type(&self) -> &'static str;
    
    /// Get a human-readable description of what this factory creates
    fn description(&self) -> &'static str;
    
    /// Get the priority of this factory (higher = more preferred)
    fn priority(&self) -> u32 {
        100 // Default priority
    }
}

/// Async variant of the Factory trait
#[async_trait]
pub trait AsyncFactory<T>: Send + Sync {
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Create an object with the given configuration asynchronously
    async fn create(&self, config: Self::Config) -> Result<T, Self::Error>;
    
    /// Check if this factory can create an object with the given configuration
    async fn can_create(&self, config: &Self::Config) -> bool;
    
    /// Get the factory type identifier
    fn factory_type(&self) -> &'static str;
    
    /// Get a human-readable description of what this factory creates
    fn description(&self) -> &'static str;
    
    /// Get the priority of this factory (higher = more preferred)
    fn priority(&self) -> u32 {
        100 // Default priority
    }
}

/// Registry for managing multiple factories of the same type
pub struct Registry<T> {
    factories: HashMap<String, Arc<dyn Factory<T, Config = Box<dyn std::any::Any + Send + Sync>, Error = BlixardError>>>,
    default_factory: Option<String>,
}

impl<T> Registry<T> {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            default_factory: None,
        }
    }

    /// Register a factory with the registry
    pub fn register<F>(&mut self, factory: F) -> BlixardResult<()>
    where
        F: Factory<T, Config = Box<dyn std::any::Any + Send + Sync>, Error = BlixardError> + 'static,
    {
        let factory_type = factory.factory_type().to_string();
        
        if self.factories.contains_key(&factory_type) {
            return Err(BlixardError::Internal {
                message: format!("Factory type '{}' is already registered", factory_type),
            });
        }

        self.factories.insert(factory_type, Arc::new(factory));
        Ok(())
    }

    /// Set the default factory type
    pub fn set_default(&mut self, factory_type: &str) -> BlixardResult<()> {
        if !self.factories.contains_key(factory_type) {
            return Err(BlixardError::Internal {
                message: format!("Factory type '{}' is not registered", factory_type),
            });
        }
        
        self.default_factory = Some(factory_type.to_string());
        Ok(())
    }

    /// Create an object using a specific factory type
    pub fn create(&self, factory_type: &str, config: Box<dyn std::any::Any + Send + Sync>) -> BlixardResult<T> {
        let factory = self.factories.get(factory_type).ok_or_else(|| {
            BlixardError::Internal {
                message: format!("Factory type '{}' not found", factory_type),
            }
        })?;

        factory.create(config).map_err(|e| BlixardError::Internal {
            message: format!("Factory creation failed: {}", e),
        })
    }

    /// Create an object using the default factory
    pub fn create_default(&self, config: Box<dyn std::any::Any + Send + Sync>) -> BlixardResult<T> {
        let default_type = self.default_factory.as_ref().ok_or_else(|| {
            BlixardError::Internal {
                message: "No default factory type set".to_string(),
            }
        })?;

        self.create(default_type, config)
    }

    /// Create an object using the best available factory
    pub fn create_best(&self, config: Box<dyn std::any::Any + Send + Sync>) -> BlixardResult<T> {
        // Find all factories that can create with this config
        let mut candidates = Vec::new();
        
        for (factory_type, factory) in &self.factories {
            if factory.can_create(&config) {
                candidates.push((factory_type.clone(), factory.clone(), factory.priority()));
            }
        }

        if candidates.is_empty() {
            return Err(BlixardError::Internal {
                message: "No factory can create object with given configuration".to_string(),
            });
        }

        // Sort by priority (highest first)
        candidates.sort_by(|a, b| b.2.cmp(&a.2));

        // Use the highest priority factory
        let (_, factory, _) = &candidates[0];
        factory.create(config).map_err(|e| BlixardError::Internal {
            message: format!("Factory creation failed: {}", e),
        })
    }

    /// List all registered factory types
    pub fn list_types(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }

    /// Get information about all registered factories
    pub fn list_factories(&self) -> Vec<FactoryInfo> {
        self.factories
            .values()
            .map(|factory| FactoryInfo {
                factory_type: factory.factory_type().to_string(),
                description: factory.description().to_string(),
                priority: factory.priority(),
            })
            .collect()
    }
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a registered factory
#[derive(Debug, Clone)]
pub struct FactoryInfo {
    pub factory_type: String,
    pub description: String,
    pub priority: u32,
}

/// Async registry for managing async factories
pub struct AsyncRegistry<T> {
    factories: HashMap<String, Arc<dyn AsyncFactory<T, Config = Box<dyn std::any::Any + Send + Sync>, Error = BlixardError>>>,
    default_factory: Option<String>,
}

impl<T> AsyncRegistry<T> {
    /// Create a new empty async registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            default_factory: None,
        }
    }

    /// Register an async factory with the registry
    pub fn register<F>(&mut self, factory: F) -> BlixardResult<()>
    where
        F: AsyncFactory<T, Config = Box<dyn std::any::Any + Send + Sync>, Error = BlixardError> + 'static,
    {
        let factory_type = factory.factory_type().to_string();
        
        if self.factories.contains_key(&factory_type) {
            return Err(BlixardError::Internal {
                message: format!("Async factory type '{}' is already registered", factory_type),
            });
        }

        self.factories.insert(factory_type, Arc::new(factory));
        Ok(())
    }

    /// Create an object using a specific async factory type
    pub async fn create(&self, factory_type: &str, config: Box<dyn std::any::Any + Send + Sync>) -> BlixardResult<T> {
        let factory = self.factories.get(factory_type).ok_or_else(|| {
            BlixardError::Internal {
                message: format!("Async factory type '{}' not found", factory_type),
            }
        })?;

        factory.create(config).await.map_err(|e| BlixardError::Internal {
            message: format!("Async factory creation failed: {}", e),
        })
    }
}

impl<T> Default for AsyncRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for objects that can be created by a factory
pub trait Creatable<C> {
    type Factory: Factory<Self, Config = C>;
    
    /// Get the factory for creating this type
    fn factory() -> Self::Factory;
}

/// Trait for objects that can be created by an async factory
pub trait AsyncCreatable<C> {
    type AsyncFactory: AsyncFactory<Self, Config = C>;
    
    /// Get the async factory for creating this type
    fn async_factory() -> Self::AsyncFactory;
}

/// Simple factory implementation for functions
pub struct FunctionFactory<T, C, F> {
    factory_type: &'static str,
    description: &'static str,
    priority: u32,
    create_fn: F,
    _phantom: std::marker::PhantomData<(T, C)>,
}

impl<T, C, F> FunctionFactory<T, C, F>
where
    F: Fn(C) -> BlixardResult<T> + Send + Sync,
    C: Send + Sync,
{
    pub fn new(
        factory_type: &'static str,
        description: &'static str,
        create_fn: F,
    ) -> Self {
        Self {
            factory_type,
            description,
            priority: 100,
            create_fn,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

impl<T, C, F> Factory<T> for FunctionFactory<T, C, F>
where
    F: Fn(C) -> BlixardResult<T> + Send + Sync,
    C: Send + Sync,
{
    type Config = C;
    type Error = BlixardError;

    fn create(&self, config: Self::Config) -> Result<T, Self::Error> {
        (self.create_fn)(config)
    }

    fn can_create(&self, _config: &Self::Config) -> bool {
        true // Function factories can generally handle any config
    }

    fn factory_type(&self) -> &'static str {
        self.factory_type
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn priority(&self) -> u32 {
        self.priority
    }
}

/// Macro to create a simple factory for a type
#[macro_export]
macro_rules! simple_factory {
    ($factory_name:ident, $target_type:ty, $config_type:ty, $factory_type:expr, $description:expr) => {
        pub struct $factory_name;

        impl $crate::patterns::Factory<$target_type> for $factory_name {
            type Config = $config_type;
            type Error = $crate::error::BlixardError;

            fn create(&self, config: Self::Config) -> Result<$target_type, Self::Error> {
                <$target_type>::new(config)
            }

            fn can_create(&self, _config: &Self::Config) -> bool {
                true
            }

            fn factory_type(&self) -> &'static str {
                $factory_type
            }

            fn description(&self) -> &'static str {
                $description
            }
        }

        impl $crate::patterns::Creatable<$config_type> for $target_type {
            type Factory = $factory_name;

            fn factory() -> Self::Factory {
                $factory_name
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test types
    #[derive(Debug, PartialEq)]
    struct TestService {
        name: String,
        port: u16,
    }

    impl TestService {
        fn new(config: TestServiceConfig) -> BlixardResult<Self> {
            Ok(Self {
                name: config.name,
                port: config.port,
            })
        }
    }

    #[derive(Debug, Clone)]
    struct TestServiceConfig {
        name: String,
        port: u16,
    }

    // Create factory using macro
    simple_factory!(
        TestServiceFactory,
        TestService,
        TestServiceConfig,
        "test_service",
        "Factory for creating test services"
    );

    #[test]
    fn test_simple_factory() {
        let factory = TestServiceFactory;
        let config = TestServiceConfig {
            name: "test".to_string(),
            port: 8080,
        };

        let service = factory.create(config).unwrap();
        assert_eq!(service.name, "test");
        assert_eq!(service.port, 8080);
    }

    #[test]
    fn test_function_factory() {
        let factory = FunctionFactory::new(
            "string_factory",
            "Creates strings from numbers",
            |num: u32| Ok(format!("Number: {}", num)),
        );

        let result = factory.create(42).unwrap();
        assert_eq!(result, "Number: 42");
        
        assert_eq!(factory.factory_type(), "string_factory");
        assert_eq!(factory.description(), "Creates strings from numbers");
        assert!(factory.can_create(&123));
    }

    #[test]
    fn test_function_factory_with_priority() {
        let factory = FunctionFactory::new(
            "high_priority",
            "High priority factory",
            |x: i32| Ok(x * 2),
        ).with_priority(200);

        assert_eq!(factory.priority(), 200);
    }

    #[test]
    fn test_creatable_trait() {
        let factory = TestService::factory();
        let config = TestServiceConfig {
            name: "created".to_string(),
            port: 3000,
        };

        let service = factory.create(config).unwrap();
        assert_eq!(service.name, "created");
        assert_eq!(service.port, 3000);
    }

    // Note: Registry tests would require more complex setup due to type erasure
    // In practice, registries would be used with specific concrete types
}