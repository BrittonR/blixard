//! Static dispatch patterns for zero-cost polymorphism
//!
//! This module provides static dispatch alternatives to dynamic dispatch,
//! eliminating vtable overhead and enabling full compiler optimization.

use std::marker::PhantomData;

/// Trait for static dispatch
pub trait StaticDispatch {
    type Service;
    type Config;
    type Error;

    /// Create the service
    fn create(config: Self::Config) -> Result<Self::Service, Self::Error>;

    /// Service type identifier (for debugging)
    const TYPE_ID: &'static str;
}

/// Compile-time service registry
pub struct CompileTimeRegistry<S: StaticDispatch> {
    _service: PhantomData<S>,
}

impl<S: StaticDispatch> CompileTimeRegistry<S> {
    /// Register a service type at compile time
    pub const fn new() -> Self {
        Self {
            _service: PhantomData,
        }
    }

    /// Create the service
    pub fn create(&self, config: S::Config) -> Result<S::Service, S::Error> {
        S::create(config)
    }

    /// Get the service type ID
    pub const fn type_id(&self) -> &'static str {
        S::TYPE_ID
    }
}

/// Example: Database backends with static dispatch
pub mod database {
    use super::*;
    use std::collections::HashMap;

    // Database trait
    pub trait Database {
        type Connection;
        type Error;

        fn connect(&self, url: &str) -> Result<Self::Connection, Self::Error>;
        fn execute(&self, conn: &Self::Connection, query: &str) -> Result<Vec<String>, Self::Error>;
    }

    // PostgreSQL implementation
    pub struct PostgreSQLBackend;
    pub struct PostgreSQLConnection(String);
    
    #[derive(Debug)]
    pub struct PostgreSQLError(String);

    impl Database for PostgreSQLBackend {
        type Connection = PostgreSQLConnection;
        type Error = PostgreSQLError;

        fn connect(&self, url: &str) -> Result<Self::Connection, Self::Error> {
            if url.starts_with("postgresql://") {
                Ok(PostgreSQLConnection(url.to_string()))
            } else {
                Err(PostgreSQLError("Invalid PostgreSQL URL".to_string()))
            }
        }

        fn execute(&self, _conn: &Self::Connection, query: &str) -> Result<Vec<String>, Self::Error> {
            // Mock implementation
            Ok(vec![format!("PostgreSQL result for: {}", query)])
        }
    }

    // MySQL implementation
    pub struct MySQLBackend;
    pub struct MySQLConnection(String);
    
    #[derive(Debug)]
    pub struct MySQLError(String);

    impl Database for MySQLBackend {
        type Connection = MySQLConnection;
        type Error = MySQLError;

        fn connect(&self, url: &str) -> Result<Self::Connection, Self::Error> {
            if url.starts_with("mysql://") {
                Ok(MySQLConnection(url.to_string()))
            } else {
                Err(MySQLError("Invalid MySQL URL".to_string()))
            }
        }

        fn execute(&self, _conn: &Self::Connection, query: &str) -> Result<Vec<String>, Self::Error> {
            // Mock implementation
            Ok(vec![format!("MySQL result for: {}", query)])
        }
    }

    // Static dispatch implementations
    pub struct PostgreSQLDispatch;
    pub struct MySQLDispatch;

    impl StaticDispatch for PostgreSQLDispatch {
        type Service = PostgreSQLBackend;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(PostgreSQLBackend)
        }

        const TYPE_ID: &'static str = "PostgreSQL";
    }

    impl StaticDispatch for MySQLDispatch {
        type Service = MySQLBackend;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(MySQLBackend)
        }

        const TYPE_ID: &'static str = "MySQL";
    }

    // Zero-cost database client
    pub struct DatabaseClient<D: StaticDispatch>
    where
        D::Service: Database,
    {
        backend: D::Service,
        connections: HashMap<String, <D::Service as Database>::Connection>,
    }

    impl<D: StaticDispatch> DatabaseClient<D>
    where
        D::Service: Database,
    {
        pub fn new(config: D::Config) -> Result<Self, D::Error> {
            let backend = D::create(config)?;
            Ok(Self {
                backend,
                connections: HashMap::new(),
            })
        }

        pub fn connect(&mut self, name: &str, url: &str) -> Result<(), <D::Service as Database>::Error> {
            let conn = self.backend.connect(url)?;
            self.connections.insert(name.to_string(), conn);
            Ok(())
        }

        pub fn execute(&self, connection: &str, query: &str) -> Result<Vec<String>, <D::Service as Database>::Error> {
            if let Some(conn) = self.connections.get(connection) {
                self.backend.execute(conn, query)
            } else {
                // Return appropriate error - this is just a mock
                // In real implementation, we'd have a proper error type
                panic!("Connection not found")
            }
        }

        pub fn backend_type(&self) -> &'static str {
            D::TYPE_ID
        }
    }
}

/// Example: Message serialization with static dispatch
pub mod serialization {
    use super::*;
    use serde::{Deserialize, Serialize};

    // Serialization trait
    pub trait Serializer {
        type Error;

        fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, Self::Error>;
        fn deserialize<T: for<'a> Deserialize<'a>>(&self, data: &[u8]) -> Result<T, Self::Error>;
    }

    // JSON serializer
    pub struct JsonSerializer;

    impl Serializer for JsonSerializer {
        type Error = serde_json::Error;

        fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, Self::Error> {
            serde_json::to_vec(value)
        }

        fn deserialize<T: for<'a> Deserialize<'a>>(&self, data: &[u8]) -> Result<T, Self::Error> {
            serde_json::from_slice(data)
        }
    }

    // Bincode serializer
    pub struct BincodeSerializer;

    impl Serializer for BincodeSerializer {
        type Error = bincode::Error;

        fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, Self::Error> {
            bincode::serialize(value)
        }

        fn deserialize<T: for<'a> Deserialize<'a>>(&self, data: &[u8]) -> Result<T, Self::Error> {
            bincode::deserialize(data)
        }
    }

    // Static dispatch implementations
    pub struct JsonDispatch;
    pub struct BincodeDispatch;

    impl StaticDispatch for JsonDispatch {
        type Service = JsonSerializer;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(JsonSerializer)
        }

        const TYPE_ID: &'static str = "JSON";
    }

    impl StaticDispatch for BincodeDispatch {
        type Service = BincodeSerializer;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(BincodeSerializer)
        }

        const TYPE_ID: &'static str = "Bincode";
    }

    // Zero-cost message handler
    pub struct MessageHandler<S: StaticDispatch>
    where
        S::Service: Serializer,
    {
        serializer: S::Service,
    }

    impl<S: StaticDispatch> MessageHandler<S>
    where
        S::Service: Serializer,
    {
        pub fn new(config: S::Config) -> Result<Self, S::Error> {
            let serializer = S::create(config)?;
            Ok(Self { serializer })
        }

        pub fn send_message<T: Serialize>(&self, message: &T) -> Result<Vec<u8>, <S::Service as Serializer>::Error> {
            self.serializer.serialize(message)
        }

        pub fn receive_message<T: for<'a> Deserialize<'a>>(&self, data: &[u8]) -> Result<T, <S::Service as Serializer>::Error> {
            self.serializer.deserialize(data)
        }

        pub fn serializer_type(&self) -> &'static str {
            S::TYPE_ID
        }
    }
}

/// Example: VM backend selection with static dispatch
pub mod vm_backend {
    use super::*;

    // VM backend trait
    pub trait VmBackend {
        type VmHandle;
        type Error;

        fn create_vm(&self, config: &str) -> Result<Self::VmHandle, Self::Error>;
        fn start_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error>;
        fn stop_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error>;
    }

    // Docker backend
    pub struct DockerBackend;
    pub struct DockerHandle(String);

    #[derive(Debug)]
    pub struct DockerError(String);

    impl VmBackend for DockerBackend {
        type VmHandle = DockerHandle;
        type Error = DockerError;

        fn create_vm(&self, config: &str) -> Result<Self::VmHandle, Self::Error> {
            Ok(DockerHandle(format!("docker-{}", config)))
        }

        fn start_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error> {
            println!("Starting Docker container: {}", handle.0);
            Ok(())
        }

        fn stop_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error> {
            println!("Stopping Docker container: {}", handle.0);
            Ok(())
        }
    }

    // Firecracker backend
    pub struct FirecrackerBackend;
    pub struct FirecrackerHandle(String);

    #[derive(Debug)]
    pub struct FirecrackerError(String);

    impl VmBackend for FirecrackerBackend {
        type VmHandle = FirecrackerHandle;
        type Error = FirecrackerError;

        fn create_vm(&self, config: &str) -> Result<Self::VmHandle, Self::Error> {
            Ok(FirecrackerHandle(format!("fc-{}", config)))
        }

        fn start_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error> {
            println!("Starting Firecracker VM: {}", handle.0);
            Ok(())
        }

        fn stop_vm(&self, handle: &Self::VmHandle) -> Result<(), Self::Error> {
            println!("Stopping Firecracker VM: {}", handle.0);
            Ok(())
        }
    }

    // Static dispatch implementations
    pub struct DockerDispatch;
    pub struct FirecrackerDispatch;

    impl StaticDispatch for DockerDispatch {
        type Service = DockerBackend;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(DockerBackend)
        }

        const TYPE_ID: &'static str = "Docker";
    }

    impl StaticDispatch for FirecrackerDispatch {
        type Service = FirecrackerBackend;
        type Config = ();
        type Error = &'static str;

        fn create(_config: Self::Config) -> Result<Self::Service, Self::Error> {
            Ok(FirecrackerBackend)
        }

        const TYPE_ID: &'static str = "Firecracker";
    }

    // Zero-cost VM manager
    pub struct VmManager<B: StaticDispatch>
    where
        B::Service: VmBackend,
    {
        backend: B::Service,
        vms: Vec<<B::Service as VmBackend>::VmHandle>,
    }

    impl<B: StaticDispatch> VmManager<B>
    where
        B::Service: VmBackend,
    {
        pub fn new(config: B::Config) -> Result<Self, B::Error> {
            let backend = B::create(config)?;
            Ok(Self {
                backend,
                vms: Vec::new(),
            })
        }

        pub fn create_vm(&mut self, config: &str) -> Result<usize, <B::Service as VmBackend>::Error> {
            let handle = self.backend.create_vm(config)?;
            self.vms.push(handle);
            Ok(self.vms.len() - 1)
        }

        pub fn start_vm(&self, index: usize) -> Result<(), <B::Service as VmBackend>::Error> {
            if let Some(handle) = self.vms.get(index) {
                self.backend.start_vm(handle)
            } else {
                panic!("VM not found")
            }
        }

        pub fn stop_vm(&self, index: usize) -> Result<(), <B::Service as VmBackend>::Error> {
            if let Some(handle) = self.vms.get(index) {
                self.backend.stop_vm(handle)
            } else {
                panic!("VM not found")
            }
        }

        pub fn backend_type(&self) -> &'static str {
            B::TYPE_ID
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_static_dispatch() {
        use database::*;

        // PostgreSQL client
        let mut pg_client = DatabaseClient::<PostgreSQLDispatch>::new(()).unwrap();
        assert_eq!(pg_client.backend_type(), "PostgreSQL");

        pg_client.connect("main", "postgresql://localhost/test").unwrap();
        let result = pg_client.execute("main", "SELECT * FROM users").unwrap();
        assert!(result[0].contains("PostgreSQL"));

        // MySQL client
        let mut mysql_client = DatabaseClient::<MySQLDispatch>::new(()).unwrap();
        assert_eq!(mysql_client.backend_type(), "MySQL");

        mysql_client.connect("main", "mysql://localhost/test").unwrap();
        let result = mysql_client.execute("main", "SELECT * FROM users").unwrap();
        assert!(result[0].contains("MySQL"));
    }

    #[test]
    fn test_vm_backend_static_dispatch() {
        use vm_backend::*;

        // Docker VM manager
        let mut docker_manager = VmManager::<DockerDispatch>::new(()).unwrap();
        assert_eq!(docker_manager.backend_type(), "Docker");

        let vm_id = docker_manager.create_vm("test-config").unwrap();
        docker_manager.start_vm(vm_id).unwrap();
        docker_manager.stop_vm(vm_id).unwrap();

        // Firecracker VM manager
        let mut fc_manager = VmManager::<FirecrackerDispatch>::new(()).unwrap();
        assert_eq!(fc_manager.backend_type(), "Firecracker");

        let vm_id = fc_manager.create_vm("test-config").unwrap();
        fc_manager.start_vm(vm_id).unwrap();
        fc_manager.stop_vm(vm_id).unwrap();
    }

    #[test]
    fn test_serialization_static_dispatch() {
        use serialization::*;
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestMessage {
            id: u32,
            text: String,
        }

        let message = TestMessage {
            id: 42,
            text: "Hello, World!".to_string(),
        };

        // JSON handler
        let json_handler = MessageHandler::<JsonDispatch>::new(()).unwrap();
        assert_eq!(json_handler.serializer_type(), "JSON");

        let serialized = json_handler.send_message(&message).unwrap();
        let deserialized: TestMessage = json_handler.receive_message(&serialized).unwrap();
        assert_eq!(message, deserialized);

        // Bincode handler
        let bincode_handler = MessageHandler::<BincodeDispatch>::new(()).unwrap();
        assert_eq!(bincode_handler.serializer_type(), "Bincode");

        let serialized = bincode_handler.send_message(&message).unwrap();
        let deserialized: TestMessage = bincode_handler.receive_message(&serialized).unwrap();
        assert_eq!(message, deserialized);
    }
}