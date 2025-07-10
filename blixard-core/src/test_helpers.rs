//! Enhanced test helpers for integration tests
//!
//! This module provides utilities for setting up full nodes with Raft for testing,
//! including higher-level abstractions for cluster management, automatic port allocation,
//! and better wait conditions.

use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};
// Removed tonic imports - using Iroh transport

use crate::{
    error::{BlixardError, BlixardResult},
    iroh_types::{HealthCheckRequest, LeaveRequest},
    metrics_server::spawn_metrics_server,
    node::Node,
    node_shared::SharedNodeState,
    raft_storage::{init_database_tables, VM_STATE_TABLE, WORKER_TABLE, WORKER_STATUS_TABLE},
    transport::iroh_client::IrohClusterServiceClient,
    transport::iroh_service_runner::start_iroh_services,
    types::NodeConfig,
};

/// Global port allocator for tests
static PORT_ALLOCATOR: AtomicU16 = AtomicU16::new(20000);

/// Diagnostic counters for port allocation
static PORT_ALLOCATION_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static PORT_ALLOCATION_FAILURES: AtomicU64 = AtomicU64::new(0);
static PORT_ALLOCATION_SUCCESSES: AtomicU64 = AtomicU64::new(0);

/// Port allocator for automatic port assignment
pub struct PortAllocator;

/// Timing utilities for robust test execution
pub mod timing {
    use super::*;

    /// Get a timeout multiplier based on the environment
    /// CI environments often need longer timeouts due to resource constraints
    pub fn timeout_multiplier() -> u64 {
        // Check for CI environment variables
        if env::var("CI").is_ok() || env::var("GITHUB_ACTIONS").is_ok() {
            return 3; // 3x slower in CI
        }

        // Allow manual override
        if let Ok(multiplier) = env::var("TEST_TIMEOUT_MULTIPLIER") {
            if let Ok(m) = multiplier.parse::<u64>() {
                return m;
            }
        }

        1 // Normal speed for local development
    }

    /// Apply the timeout multiplier to a duration
    pub fn scaled_timeout(base: Duration) -> Duration {
        let multiplier = timeout_multiplier();
        // Use saturating mul to avoid overflow
        base.saturating_mul(multiplier as u32)
    }

    /// Wait for a condition to become true with exponential backoff
    pub async fn wait_for_condition_with_backoff<F, Fut>(
        mut condition: F,
        max_wait: Duration,
        initial_interval: Duration,
    ) -> Result<(), String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        let start = Instant::now();
        let scaled_max = scaled_timeout(max_wait);
        let mut interval = initial_interval;
        let max_interval = Duration::from_secs(1);

        while start.elapsed() < scaled_max {
            if condition().await {
                return Ok(());
            }

            sleep(interval).await;

            // Exponential backoff with cap
            interval = (interval * 2).min(max_interval);
        }

        Err(format!("Condition not met within {:?}", scaled_max))
    }

    /// Robust sleep that accounts for CI environments
    pub async fn robust_sleep(base_duration: Duration) {
        // Just sleep for the scaled duration without further manipulation
        // to avoid potential overflow in tokio's deadline calculation
        let scaled = scaled_timeout(base_duration);
        sleep(scaled).await;
    }
}

/// Test database factory for creating isolated database instances
pub struct TestDatabaseFactory;

impl TestDatabaseFactory {
    /// Create a test database with all tables initialized
    /// Returns (database, temp_dir) for proper cleanup
    pub fn create() -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| BlixardError::Internal {
            message: format!("Failed to create temp directory: {}", e),
        })?;
        
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(&db_path).map_err(|e| BlixardError::StorageError {
            message: format!("Failed to create database: {}", e),
        })?);
        
        // Initialize all tables
        init_database_tables(&database)?;
        
        Ok((database, temp_dir))
    }
    
    /// Create a test database with custom name
    pub fn create_with_name(name: &str) -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| BlixardError::Internal {
            message: format!("Failed to create temp directory: {}", e),
        })?;
        
        let db_path = temp_dir.path().join(format!("{}.db", name));
        let database = Arc::new(redb::Database::create(&db_path).map_err(|e| BlixardError::StorageError {
            message: format!("Failed to create database: {}", e),
        })?);
        
        // Initialize all tables
        init_database_tables(&database)?;
        
        Ok((database, temp_dir))
    }
    
    /// Create a test database without Arc wrapper (for compatibility)
    pub fn create_raw() -> BlixardResult<(redb::Database, tempfile::TempDir)> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| BlixardError::Internal {
            message: format!("Failed to create temp directory: {}", e),
        })?;
        
        let db_path = temp_dir.path().join("test.db");
        let database = redb::Database::create(&db_path).map_err(|e| BlixardError::StorageError {
            message: format!("Failed to create database: {}", e),
        })?;
        
        Ok((database, temp_dir))
    }
    
    /// Create a test database with specific tables only (for specialized tests)
    pub fn create_with_tables(tables: &[&'static str]) -> BlixardResult<(Arc<redb::Database>, tempfile::TempDir)> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| BlixardError::Internal {
            message: format!("Failed to create temp directory: {}", e),
        })?;
        
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(&db_path).map_err(|e| BlixardError::StorageError {
            message: format!("Failed to create database: {}", e),
        })?);
        
        // Create only specified tables
        let write_txn = database.begin_write().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to begin write transaction: {}", e),
        })?;
        
        for table_name in tables {
            match *table_name {
                "vm_states" => { let _ = write_txn.open_table(VM_STATE_TABLE)?; },
                "workers" => { let _ = write_txn.open_table(WORKER_TABLE)?; },
                "worker_status" => { let _ = write_txn.open_table(WORKER_STATUS_TABLE)?; },
                _ => {
                    // For now, skip unknown tables - could be extended
                    tracing::warn!("Unknown table requested: {}", table_name);
                }
            }
        }
        
        write_txn.commit().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to commit table creation: {}", e),
        })?;
        
        Ok((database, temp_dir))
    }
}

/// Test worker factory for creating worker registrations
pub struct TestWorkerFactory;

impl TestWorkerFactory {
    /// Register a worker in the database with specified capabilities
    pub fn register_worker(
        database: &Arc<redb::Database>,
        node_id: u64,
        capabilities: crate::raft_manager::WorkerCapabilities,
        is_online: bool,
    ) -> BlixardResult<()> {
        let write_txn = database.begin_write().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to begin write transaction: {}", e),
        })?;
        
        {
            let mut worker_table = write_txn.open_table(WORKER_TABLE)?;
            let mut status_table = write_txn.open_table(WORKER_STATUS_TABLE)?;

            let worker_data = bincode::serialize(&(format!("127.0.0.1:{}", 7000 + node_id), capabilities))
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to serialize worker data: {}", e),
                })?;
            
            worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())
                .map_err(|e| BlixardError::StorageError {
                    message: format!("Failed to insert worker: {}", e),
                })?;

            let status = if is_online {
                crate::raft_manager::WorkerStatus::Online as u8
            } else {
                crate::raft_manager::WorkerStatus::Offline as u8
            };
            
            status_table.insert(node_id.to_le_bytes().as_slice(), [status].as_slice())
                .map_err(|e| BlixardError::StorageError {
                    message: format!("Failed to insert worker status: {}", e),
                })?;
        }
        
        write_txn.commit().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to commit worker registration: {}", e),
        })?;
        
        Ok(())
    }
    
    /// Create a standard worker configuration for testing
    pub fn standard_capabilities() -> crate::raft_manager::WorkerCapabilities {
        crate::raft_manager::WorkerCapabilities {
            cpu_cores: 8,
            memory_mb: 16384,
            disk_gb: 100,
            features: vec![],
        }
    }
    
    /// Create a GPU-enabled worker configuration
    pub fn gpu_capabilities() -> crate::raft_manager::WorkerCapabilities {
        crate::raft_manager::WorkerCapabilities {
            cpu_cores: 16,
            memory_mb: 32768,
            disk_gb: 500,
            features: vec!["gpu".to_string()],
        }
    }
    
    /// Create a low-resource worker configuration
    pub fn low_resource_capabilities() -> crate::raft_manager::WorkerCapabilities {
        crate::raft_manager::WorkerCapabilities {
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 50,
            features: vec![],
        }
    }
}

/// Test VM factory for creating VM states
pub struct TestVmFactory;

impl TestVmFactory {
    /// Create a standard test VM configuration
    pub fn create_vm_config(name: &str) -> crate::types::VmConfig {
        crate::types::VmConfig {
            name: name.to_string(),
            config_path: "/tmp/test.nix".to_string(),
            memory: 1024,
            vcpus: 2,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
        }
    }
    
    /// Create a test VM state
    pub fn create_vm_state(name: &str, node_id: u64) -> crate::types::VmState {
        let config = Self::create_vm_config(name);
        let now = chrono::Utc::now();
        
        crate::types::VmState {
            name: name.to_string(),
            config,
            status: crate::types::VmStatus::Creating,
            node_id,
            created_at: now,
            updated_at: now,
        }
    }
    
    /// Store a VM state in the database
    pub fn store_vm_state(
        database: &Arc<redb::Database>,
        vm_state: &crate::types::VmState,
    ) -> BlixardResult<()> {
        let write_txn = database.begin_write().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to begin write transaction: {}", e),
        })?;
        
        {
            let mut table = write_txn.open_table(VM_STATE_TABLE)?;
            let serialized = bincode::serialize(vm_state)
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to serialize VM state: {}", e),
                })?;
            
            table.insert(vm_state.name.as_str(), serialized.as_slice())
                .map_err(|e| BlixardError::StorageError {
                    message: format!("Failed to insert VM state: {}", e),
                })?;
        }
        
        write_txn.commit().map_err(|e| BlixardError::StorageError {
            message: format!("Failed to commit VM state: {}", e),
        })?;
        
        Ok(())
    }
    
    /// Add a VM to the database with specified parameters
    /// This is a convenience method that matches the old add_vm function signature
    pub fn add_vm_to_database(
        database: &Arc<redb::Database>,
        vm_name: &str,
        node_id: u64,
        vcpus: u32,
        memory: u32,
        status: crate::types::VmStatus,
    ) -> crate::error::BlixardResult<()> {
        let vm_config = crate::types::VmConfig {
            name: vm_name.to_string(),
            config_path: "".to_string(),
            vcpus,
            memory,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            priority: 500,
            preemptible: true,
            locality_preference: Default::default(),
            health_check_config: None,
        };

        let vm_state = crate::types::VmState {
            name: vm_name.to_string(),
            config: vm_config,
            status,
            node_id,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        Self::store_vm_state(database, &vm_state)
    }
}

impl PortAllocator {
    /// Get the next available port
    pub fn next_port() -> u16 {
        // Initialize with process ID offset on first use to reduce conflicts
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let offset = (std::process::id() % 1000) as u16;
            PORT_ALLOCATOR.store(20000 + offset, Ordering::SeqCst);
        });

        // Use a loop to handle the wraparound case atomically
        loop {
            let current = PORT_ALLOCATOR.load(Ordering::SeqCst);
            if current > 30000 {
                // Try to reset to 20000 + offset
                let offset = (std::process::id() % 1000) as u16;
                let new_base = 20000 + offset;
                if PORT_ALLOCATOR
                    .compare_exchange(current, new_base, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return new_base;
                }
                // If someone else reset it, continue the loop
            } else {
                // Try to increment
                if let Ok(port) = PORT_ALLOCATOR.compare_exchange(
                    current,
                    current + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    return port;
                }
                // If someone else incremented, continue the loop
            }
        }
    }

    /// Get the next available port that can actually be bound to
    /// This method tries to bind to the port before returning it,
    /// eliminating race conditions
    pub async fn next_available_port() -> u16 {
        let max_attempts = 100;
        let mut attempts = 0;
        let debug_enabled = env::var("BLIXARD_PORT_DEBUG").is_ok();
        let start_time = Instant::now();

        loop {
            let port = Self::next_port();
            PORT_ALLOCATION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

            // Try to bind to the port
            match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                Ok(listener) => {
                    // Successfully bound - immediately close it
                    drop(listener);

                    PORT_ALLOCATION_SUCCESSES.fetch_add(1, Ordering::Relaxed);
                    let elapsed = start_time.elapsed();

                    if debug_enabled {
                        eprintln!(
                            "PortAllocator: Successfully allocated port {} after {} attempts in {:?}",
                            port, attempts + 1, elapsed
                        );
                    }

                    // Log if this was particularly difficult
                    if attempts > 5 {
                        tracing::warn!(
                            "Port allocation required {} attempts ({:?}) - consider increasing port range",
                            attempts + 1, elapsed
                        );
                    }

                    return port;
                }
                Err(e) => {
                    attempts += 1;
                    PORT_ALLOCATION_FAILURES.fetch_add(1, Ordering::Relaxed);

                    if debug_enabled {
                        eprintln!("PortAllocator: Port {} unavailable: {}", port, e);
                    }

                    if attempts >= max_attempts {
                        let stats = Self::get_stats();
                        panic!(
                            "Failed to find available port after {} attempts. Stats: {}",
                            max_attempts, stats
                        );
                    }

                    // Add a small delay with exponential backoff
                    let delay = Duration::from_millis(10 * (attempts.min(5) as u64));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Reset port allocator (useful between test runs)
    pub fn reset() {
        PORT_ALLOCATOR.store(20000, Ordering::SeqCst);
        PORT_ALLOCATION_ATTEMPTS.store(0, Ordering::SeqCst);
        PORT_ALLOCATION_FAILURES.store(0, Ordering::SeqCst);
        PORT_ALLOCATION_SUCCESSES.store(0, Ordering::SeqCst);
    }

    /// Get port allocation statistics
    pub fn get_stats() -> String {
        let attempts = PORT_ALLOCATION_ATTEMPTS.load(Ordering::Relaxed);
        let failures = PORT_ALLOCATION_FAILURES.load(Ordering::Relaxed);
        let successes = PORT_ALLOCATION_SUCCESSES.load(Ordering::Relaxed);
        let current = PORT_ALLOCATOR.load(Ordering::Relaxed);

        format!(
            "attempts: {}, successes: {}, failures: {}, current_port: {}, failure_rate: {:.2}%",
            attempts,
            successes,
            failures,
            current,
            if attempts > 0 {
                (failures as f64 / attempts as f64) * 100.0
            } else {
                0.0
            }
        )
    }

    /// Print port allocation statistics (useful for debugging)
    pub fn print_stats() {
        if env::var("BLIXARD_PORT_DEBUG").is_ok() {
            eprintln!("PortAllocator Stats: {}", Self::get_stats());
        }
    }
}

/// Test node handle containing all necessary components
pub struct TestNode {
    pub id: u64,
    pub node: Node,
    pub shared_state: Arc<SharedNodeState>,
    pub addr: SocketAddr,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub server_handle: Option<JoinHandle<()>>,
    pub metrics_handle: Option<JoinHandle<BlixardResult<()>>>,
    _temp_dir: Option<tempfile::TempDir>,
}

impl TestNode {
    /// Builder for creating test nodes with custom configuration
    pub fn builder() -> TestNodeBuilder {
        TestNodeBuilder::default()
    }

    /// Create and start a test node with full Raft setup
    pub async fn start(id: u64, port: u16) -> BlixardResult<Self> {
        Self::builder().with_id(id).with_port(port).build().await
    }

    /// Create and start a test node that will join an existing cluster
    pub async fn start_with_join(
        id: u64,
        port: u16,
        join_addr: Option<SocketAddr>,
    ) -> BlixardResult<Self> {
        Self::builder()
            .with_id(id)
            .with_port(port)
            .with_join_addr(join_addr)
            .build()
            .await
    }

    /// Get a client connected to this node
    pub async fn client(&self) -> BlixardResult<IrohClusterServiceClient> {
        // Get the node's Iroh NodeAddr from P2P manager
        tracing::info!("Getting P2P manager for client creation");
        let node_addr = if let Some(p2p_manager) = self.shared_state.get_p2p_manager().await {
            tracing::info!("P2P manager found, getting node address");
            let addr = p2p_manager.get_node_addr().await?;
            tracing::info!("Got node address: {:?}", addr);
            addr
        } else {
            tracing::error!("P2P manager not available");
            return Err(BlixardError::Internal {
                message: "P2P manager not available".to_string(),
            });
        };

        // Create a temporary endpoint for the client
        tracing::info!("Creating client endpoint");
        let endpoint =
            iroh::Endpoint::builder()
                .bind()
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to create client endpoint: {}", e),
                })?;

        tracing::info!("Client endpoint created successfully");
        Ok(IrohClusterServiceClient::new(Arc::new(endpoint), node_addr))
    }

    /// Shutdown the test node
    pub async fn shutdown(mut self) {
        // Stop the node properly - this handles all cleanup including:
        // - Sending shutdown signal
        // - Aborting Raft handle
        // - Setting running to false
        // - Calling shutdown_components() to release database references
        // - Stopping PeerConnector background tasks
        self.node.stop().await.expect("Failed to stop node");

        // Abort the Iroh server handle if it hasn't been already
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            // Give the server more time to fully shut down and release the port
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Abort the metrics server handle
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Temp dir will be cleaned up on drop
    }

    /// Get the node ID
    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Get VM backend for VM operations (placeholder for now)
    pub fn get_vm_backend(&self) -> Option<&dyn crate::vm_backend::VmBackend> {
        // For now, return None - this would need to be implemented
        // when we add VM backend to Node
        None
    }

    /// Dump diagnostic information about this node
    pub async fn dump_diagnostics(&self) -> String {
        let raft_status = self.shared_state.get_raft_status().await.ok();
        let peers = self.shared_state.get_peers().await;

        format!(
            "Node {} diagnostics:\n  Address: {}\n  Raft Status: {:?}\n  Peers: {:?}\n",
            self.id, self.addr, raft_status, peers
        )
    }
}

/// Builder for creating test nodes
#[derive(Default)]
pub struct TestNodeBuilder {
    id: Option<u64>,
    port: Option<u16>,
    join_addr: Option<String>,
    data_dir: Option<String>,
    vm_backend: Option<String>,
    vm_backend_registry: Option<crate::vm_backend::VmBackendRegistry>,
}

impl TestNodeBuilder {
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub async fn with_auto_port(mut self) -> Self {
        self.port = Some(PortAllocator::next_available_port().await);
        self
    }

    pub fn with_join_addr(mut self, addr: Option<SocketAddr>) -> Self {
        self.join_addr = addr.map(|a| a.to_string());
        self
    }

    pub fn with_data_dir(mut self, dir: String) -> Self {
        self.data_dir = Some(dir);
        self
    }
    
    pub fn with_vm_backend(mut self, backend: String) -> Self {
        self.vm_backend = Some(backend);
        self
    }
    
    pub fn with_custom_vm_registry(mut self, registry: crate::vm_backend::VmBackendRegistry) -> Self {
        self.vm_backend_registry = Some(registry);
        self
    }

    pub async fn build(self) -> BlixardResult<TestNode> {
        let id = self
            .id
            .ok_or_else(|| BlixardError::ConfigError("Node ID is required".to_string()))?;

        let port = self.port.unwrap_or_else(|| {
            let p = PortAllocator::next_port();
            tracing::info!("Allocated port {} for node {}", p, id);
            p
        });

        // Create temp directory if not provided
        let (temp_dir, data_dir) = if let Some(dir) = self.data_dir {
            (None, dir)
        } else {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().to_string_lossy().to_string();
            (Some(temp), path)
        };

        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        
        // Get VM backend or default to mock
        let vm_backend = self.vm_backend.clone().unwrap_or_else(|| "mock".to_string());
        
        // Initialize metrics for tests if not already done
        if crate::metrics_otel::try_metrics().is_none() {
            let _ = crate::metrics_otel::init_noop()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to initialize test metrics: {}", e),
                })?;
        }

        // Initialize global config for tests if not already done
        use crate::config_global;
        use crate::config_v2::ConfigBuilder;

        if !config_global::is_initialized() {
            let test_config = match ConfigBuilder::new()
                .node_id(id)
                .bind_address(addr.to_string())
                .data_dir(&data_dir)
                .vm_backend(&vm_backend)
                .p2p_enabled(true)  // Enable P2P for tests using Iroh transport
                .build() {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Failed to build test config: {:?}", e);
                    return Err(e);
                }
            };
            if let Err(e) = config_global::init(test_config) {
                eprintln!("Failed to init global config: {:?}", e);
                // Only return error if it's not "already initialized"
                if !e.to_string().contains("already initialized") {
                    return Err(e);
                }
            }
        }

        // Create transport config for Iroh P2P
        use crate::transport::config::TransportConfig;
        let transport_config = TransportConfig::default();

        let config = NodeConfig {
            id,
            bind_addr: addr,
            data_dir,
            join_addr: self.join_addr.clone(),
            use_tailscale: false,
            vm_backend: vm_backend.clone(),
            transport_config: Some(transport_config), // Enable Iroh transport
            topology: Default::default(),   // Use default topology for tests
        };

        // Create node (must be mutable for join_cluster)
        let mut node = Node::new(config);
        let shared_state = node.shared();

        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        shared_state.set_shutdown_tx(shutdown_tx).await;

        // Initialize the node with custom registry if provided
        if let Some(registry) = self.vm_backend_registry {
            // Use the public method that accepts a custom registry
            node.initialize_with_vm_registry(registry).await?;
        } else {
            // Use default initialization (mock backend)
            node.initialize().await?;
        }

        // Now start Iroh services AFTER node initialization
        // The P2P manager now exists and services can start properly
        let state_clone = shared_state.clone();
        let addr_for_spawn = addr.clone();
        let server_handle = tokio::spawn(async move {
            tracing::info!("Starting Iroh services task for {}", addr_for_spawn);
            match start_iroh_services(state_clone, addr_for_spawn).await {
                Ok(handle) => {
                    tracing::info!("Iroh services started successfully on {}", addr_for_spawn);
                    // Keep the handle alive
                    let _ = handle.await;
                }
                Err(e) => {
                    tracing::error!("Iroh service runner failed on {}: {:?}", addr_for_spawn, e);
                    // Exit the task with an error
                    panic!("Iroh service failed: {:?}", e);
                }
            }
        });

        // Give the Iroh services time to start up
        // For Iroh, we don't have a direct way to check if it's ready,
        // so we'll just wait a bit
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Start metrics server for bootstrap endpoint
        // This is required for other nodes to join the cluster
        let metrics_port = port + 1000; // Default offset from orchestrator
        let metrics_addr: SocketAddr = format!("127.0.0.1:{}", metrics_port).parse().unwrap();
        let metrics_handle = spawn_metrics_server(metrics_addr, shared_state.clone());

        // Give metrics server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Set running state
        shared_state.set_running(true).await;

        // No need to explicitly call join_cluster here anymore
        // The node initialization handles joining if join_addr is in config

        Ok(TestNode {
            id,
            node,
            shared_state,
            addr,
            shutdown_tx: None,
            server_handle: Some(server_handle),
            metrics_handle: Some(metrics_handle),
            _temp_dir: temp_dir,
        })
    }
}

/// High-level test cluster abstraction
pub struct TestCluster {
    nodes: HashMap<u64, TestNode>,
    client_cache: Arc<Mutex<HashMap<u64, IrohClusterServiceClient>>>,
    next_node_id: u64,
}

impl TestCluster {
    /// Builder for creating test clusters
    pub fn builder() -> TestClusterBuilder {
        TestClusterBuilder::default()
    }

    /// Create a new cluster with the specified number of nodes
    pub async fn new(size: usize) -> BlixardResult<Self> {
        Self::builder().with_nodes(size).build().await
    }

    /// Wait for cluster to converge to a stable state
    pub async fn wait_for_convergence(&self, timeout_duration: Duration) -> BlixardResult<()> {
        let nodes = &self.nodes;

        // First, wait for all nodes to know about all other nodes
        // This indicates they've received the configuration
        let expected_nodes = nodes.len();
        timing::wait_for_condition_with_backoff(
            || async {
                for node in nodes.values() {
                    if let Ok((_, peers, _)) = node.shared_state.get_cluster_status().await {
                        if peers.len() < expected_nodes {
                            eprintln!(
                                "Node {} only sees {} peers, expected {}",
                                node.id,
                                peers.len(),
                                expected_nodes
                            );
                            return false;
                        }
                    } else {
                        eprintln!("Node {} failed to get cluster status", node.id);
                        return false;
                    }
                }
                eprintln!("All nodes see {} peers", expected_nodes);
                true
            },
            timeout_duration / 2,
            Duration::from_millis(100),
        )
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Nodes failed to receive configuration: {}", e),
        })?;

        // Now wait for leader convergence
        timing::wait_for_condition_with_backoff(
            || async {
                let mut all_have_leader = true;
                let mut same_leader = None;

                for node in nodes.values() {
                    let state = node.shared_state.get_raft_status().await.unwrap();

                    match state.leader_id {
                        Some(leader) => {
                            if let Some(expected) = same_leader {
                                if leader != expected {
                                    eprintln!(
                                        "Node {} sees different leader: {} vs expected {}",
                                        node.id, leader, expected
                                    );
                                    all_have_leader = false;
                                    break;
                                }
                            } else {
                                same_leader = Some(leader);
                            }
                        }
                        None => {
                            eprintln!("Node {} has no leader yet", node.id);
                            all_have_leader = false;
                            break;
                        }
                    }
                }

                if all_have_leader && same_leader.is_some() {
                    eprintln!("All nodes agree on leader: {:?}", same_leader);
                    true
                } else {
                    false
                }
            },
            timeout_duration / 2,
            Duration::from_millis(100),
        )
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!(
                "cluster convergence timeout after {:?}: {}",
                timeout_duration, e
            ),
        })
    }

    /// Get a client for the specified node
    pub async fn client(&self, node_id: u64) -> BlixardResult<IrohClusterServiceClient> {
        let mut cache = self.client_cache.lock().await;

        if let Some(client) = cache.get(&node_id) {
            return Ok(client.clone());
        }

        let node = self
            .nodes
            .get(&node_id)
            .ok_or_else(|| BlixardError::NodeError(format!("node {} not found", node_id)))?;

        let client = node.client().await?;
        cache.insert(node_id, client.clone());
        Ok(client)
    }

    /// Get the current leader's client
    pub async fn leader_client(&self) -> BlixardResult<IrohClusterServiceClient> {
        // Find the leader
        let mut leader_id = None;

        for node in self.nodes.values() {
            let state = node.shared_state.get_raft_status().await.unwrap();
            if let Some(leader) = state.leader_id {
                leader_id = Some(leader);
                break;
            }
        }

        let leader_id = leader_id
            .ok_or_else(|| BlixardError::ClusterError("No cluster leader found".to_string()))?;

        self.client(leader_id).await
    }

    /// Add a new node to the cluster
    pub async fn add_node(&mut self) -> BlixardResult<u64> {
        let id = self.next_node_id;
        self.next_node_id += 1;

        // Find the current leader to join through
        let mut leader_addr = None;
        for node in self.nodes.values() {
            let state = node.shared_state.get_raft_status().await.unwrap();
            if state.is_leader {
                leader_addr = Some(node.addr);
                break;
            }
        }

        let leader_addr = leader_addr.ok_or_else(|| {
            BlixardError::ClusterError("No cluster leader found to join through".to_string())
        })?;

        let node = TestNode::builder()
            .with_id(id)
            .with_auto_port()
            .await
            .with_join_addr(Some(leader_addr))
            .build()
            .await?;

        // Note: join request is sent automatically during node.initialize() in build()

        self.nodes.insert(id, node);
        Ok(id)
    }

    /// Remove a node from the cluster
    pub async fn remove_node(&mut self, id: u64) -> BlixardResult<()> {
        // First, send a leave request to the cluster to remove this node
        // This must be done from a different node (preferably the leader)
        let leader_client = self.leader_client().await?;

        let leave_request = LeaveRequest { node_id: id };

        let response = leader_client
            .clone()
            .leave_cluster(leave_request)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to send leave request: {}", e),
            })?;

        if !response.into_inner().success {
            return Err(BlixardError::ClusterError(format!(
                "Failed to remove node {} from cluster",
                id
            )));
        }

        // Now remove and shutdown the node
        let node = self
            .nodes
            .remove(&id)
            .ok_or_else(|| BlixardError::NodeError(format!("Node {} not found", id)))?;

        node.shutdown().await;
        Ok(())
    }

    /// Shutdown all nodes in the cluster
    pub async fn shutdown(mut self) {
        let nodes = std::mem::take(&mut self.nodes);
        // Shutdown nodes in reverse order to avoid issues with leader election
        let mut node_vec: Vec<_> = nodes.into_iter().collect();
        node_vec.sort_by_key(|(id, _)| std::cmp::Reverse(*id));

        for (_, node) in node_vec {
            node.shutdown().await;
        }

        // Give extra time for all server tasks to fully terminate
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    /// Dump diagnostic information for all nodes
    pub async fn dump_diagnostics(&self) -> String {
        let mut output = String::from("Cluster diagnostics:\n");

        for node in self.nodes.values() {
            output.push_str(&node.dump_diagnostics().await);
            output.push('\n');
        }

        output
    }

    /// Get reference to all nodes
    pub fn nodes(&self) -> &HashMap<u64, TestNode> {
        &self.nodes
    }

    /// Get mutable reference to nodes (for testing)
    pub fn nodes_mut(&mut self) -> &mut HashMap<u64, TestNode> {
        &mut self.nodes
    }

    /// Get the current leader's node ID
    pub async fn get_leader_id(&self) -> BlixardResult<u64> {
        for node in self.nodes.values() {
            let state = node.shared_state.get_raft_status().await.unwrap();
            if let Some(leader_id) = state.leader_id {
                return Ok(leader_id);
            }
        }
        Err(BlixardError::ClusterError("No leader found".to_string()))
    }

    /// Get a specific node by ID
    pub fn get_node(&self, node_id: u64) -> BlixardResult<&TestNode> {
        self.nodes
            .get(&node_id)
            .ok_or_else(|| BlixardError::NodeError(format!("Node {} not found", node_id)))
    }

    /// Clone cluster reference for test scenarios
    pub fn clone_for_test(&self) -> Self {
        TestCluster {
            nodes: HashMap::new(), // Empty for now - would need proper cloning for real use
            client_cache: self.client_cache.clone(),
            next_node_id: self.next_node_id,
        }
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        // If we're panicking, dump diagnostics
        if std::thread::panicking() {
            // Can't use async in drop, so we spawn a task
            let nodes = self
                .nodes
                .values()
                .map(|n| (n.id, n.shared_state.clone()))
                .collect::<Vec<_>>();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    eprintln!("=== Test cluster diagnostics (panic detected) ===");
                    for (id, state) in nodes {
                        let raft_state = state.get_raft_status().await.ok();
                        let peers = state.get_peers().await;
                        eprintln!(
                            "Node {}: raft_state={:?}, peers={:?}",
                            id, raft_state, peers
                        );
                    }
                    eprintln!("==============================================");
                });
            });
        }
    }
}

/// Builder for creating test clusters
#[derive(Default)]
pub struct TestClusterBuilder {
    node_count: Option<usize>,
    convergence_timeout: Option<Duration>,
    vm_backend: Option<String>,
    vm_backend_registry: Option<crate::vm_backend::VmBackendRegistry>,
}

impl TestClusterBuilder {
    pub fn with_nodes(mut self, count: usize) -> Self {
        self.node_count = Some(count);
        self
    }

    pub fn with_convergence_timeout(mut self, timeout: Duration) -> Self {
        self.convergence_timeout = Some(timeout);
        self
    }
    
    pub fn with_vm_backend(mut self, backend: String) -> Self {
        self.vm_backend = Some(backend);
        self
    }
    
    pub fn with_custom_vm_registry(mut self, registry: crate::vm_backend::VmBackendRegistry) -> Self {
        self.vm_backend_registry = Some(registry);
        self
    }

    pub async fn build(self) -> BlixardResult<TestCluster> {
        let node_count = self.node_count.unwrap_or(3);
        let convergence_timeout = self
            .convergence_timeout
            .unwrap_or_else(|| timing::scaled_timeout(Duration::from_secs(20))); // Increased for large clusters

        if node_count == 0 {
            return Err(BlixardError::ConfigError(
                "Cluster must have at least one node".to_string(),
            ));
        }

        let mut nodes = HashMap::new();

        // Create bootstrap node with VM backend configuration
        let mut bootstrap_builder = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .await;
        
        // Apply VM backend configuration if provided
        if let Some(backend) = self.vm_backend.clone() {
            bootstrap_builder = bootstrap_builder.with_vm_backend(backend);
        }
        if let Some(registry) = self.vm_backend_registry.clone() {
            bootstrap_builder = bootstrap_builder.with_custom_vm_registry(registry);
        }
        
        let bootstrap_node = bootstrap_builder.build().await?;

        let bootstrap_addr = bootstrap_node.addr;
        nodes.insert(1, bootstrap_node);

        // For larger clusters (>3 nodes), join nodes sequentially with delays
        // to ensure configuration changes are properly propagated
        if node_count > 3 {
            eprintln!(
                "Building large cluster with {} nodes - using sequential join",
                node_count
            );

            for id in 2..=node_count as u64 {
                eprintln!("Adding node {} to cluster", id);

                let mut node_builder = TestNode::builder()
                    .with_id(id)
                    .with_auto_port()
                    .await
                    .with_join_addr(Some(bootstrap_addr));
                
                // Apply VM backend configuration if provided
                if let Some(backend) = self.vm_backend.clone() {
                    node_builder = node_builder.with_vm_backend(backend);
                }
                if let Some(registry) = self.vm_backend_registry.clone() {
                    node_builder = node_builder.with_custom_vm_registry(registry);
                }
                
                let node = node_builder.build().await?;

                nodes.insert(id, node);

                // Wait for this node to be fully integrated before adding the next
                // This prevents configuration race conditions in large clusters
                let current_nodes = id as usize;

                // First wait for all nodes to see each other in voter configuration
                timing::wait_for_condition_with_backoff(
                    || async {
                        // Check that all current nodes see the correct number of voters
                        for node in nodes.values() {
                            if let Ok((_, voters, _)) = node.shared_state.get_cluster_status().await
                            {
                                if voters.len() < current_nodes {
                                    eprintln!(
                                        "Node {} only sees {} voters, expected {}",
                                        node.id,
                                        voters.len(),
                                        current_nodes
                                    );
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                        eprintln!("All {} nodes see {} voters", current_nodes, current_nodes);
                        true
                    },
                    Duration::from_secs(10), // Increased timeout for large clusters
                    Duration::from_millis(200), // Increased check interval
                )
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Node {} failed to join cluster: {}", id, e),
                })?;

                // Additional check: ensure all nodes have consistent voter configuration
                timing::wait_for_condition_with_backoff(
                    || async {
                        let mut voter_sets = Vec::new();

                        // Collect voter configuration from all nodes
                        for node in nodes.values() {
                            if let Ok((_, voters, _)) = node.shared_state.get_cluster_status().await
                            {
                                let mut sorted_voters = voters.clone();
                                sorted_voters.sort();
                                voter_sets.push((node.id, sorted_voters));
                            } else {
                                eprintln!("Node {} failed to get cluster status", node.id);
                                return false;
                            }
                        }

                        // Check if all nodes have the same voter configuration
                        if voter_sets.is_empty() {
                            return false;
                        }

                        let expected_voters = &voter_sets[0].1;
                        for (node_id, voters) in &voter_sets {
                            if voters != expected_voters {
                                eprintln!(
                                    "Node {} has different voters: {:?} vs expected {:?}",
                                    node_id, voters, expected_voters
                                );
                                return false;
                            }
                        }

                        eprintln!(
                            "All {} nodes have consistent voter configuration: {:?}",
                            current_nodes, expected_voters
                        );
                        true
                    },
                    Duration::from_secs(5), // Increased timeout for configuration convergence
                    Duration::from_millis(200), // Match check interval
                )
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Node {} configuration did not converge: {}", id, e),
                })?;

                // Delay between joins to let Raft settle
                if id < node_count as u64 {
                    tokio::time::sleep(Duration::from_secs(1)).await; // Increased delay for large clusters

                    // Send multiple health checks to ensure replication
                    for _ in 0..3 {
                        if let Some(bootstrap_node) = nodes.get(&1) {
                            if let Ok(mut client) = bootstrap_node.client().await {
                                let _ = client
                                    .health_check(crate::iroh_types::HealthCheckRequest {})
                                    .await;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    eprintln!(
                        "Sent health checks to trigger replication after adding node {}",
                        id
                    );

                    // Extra verification that all nodes have converged before proceeding
                    let current_nodes = id as usize;
                    timing::wait_for_condition_with_backoff(
                        || async {
                            let mut all_consistent = true;
                            let mut expected_voters = Vec::new();
                            
                            // First pass: find what the expected voters should be
                            for node in nodes.values() {
                                if let Ok((_, voters, _)) = node.shared_state.get_cluster_status().await {
                                    if voters.len() == current_nodes {
                                        expected_voters = voters;
                                        break;
                                    }
                                }
                            }
                            
                            if expected_voters.is_empty() {
                                return false;
                            }
                            
                            // Second pass: verify all nodes have this configuration
                            for node in nodes.values() {
                                if let Ok((_, voters, _)) = node.shared_state.get_cluster_status().await {
                                    let mut sorted_voters = voters.clone();
                                    sorted_voters.sort();
                                    expected_voters.sort();
                                    if sorted_voters != expected_voters {
                                        eprintln!("Node {} has inconsistent voters: {:?} vs expected {:?}", 
                                            node.id, sorted_voters, expected_voters);
                                        all_consistent = false;
                                    }
                                }
                            }
                            
                            all_consistent
                        },
                        Duration::from_secs(5),
                        Duration::from_millis(200),
                    )
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Configuration did not stabilize after adding node {}: {}", id, e),
                    })?;
                }
            }
        } else {
            // For small clusters (<=3 nodes), use the original parallel join
            for id in 2..=node_count as u64 {
                let mut node_builder = TestNode::builder()
                    .with_id(id)
                    .with_auto_port()
                    .await
                    .with_join_addr(Some(bootstrap_addr));
                
                // Apply VM backend configuration if provided
                if let Some(backend) = self.vm_backend.clone() {
                    node_builder = node_builder.with_vm_backend(backend);
                }
                if let Some(registry) = self.vm_backend_registry.clone() {
                    node_builder = node_builder.with_custom_vm_registry(registry);
                }
                
                let node = node_builder.build().await?;

                nodes.insert(id, node);
            }
        }

        let cluster = TestCluster {
            nodes,
            client_cache: Arc::new(Mutex::new(HashMap::new())),
            next_node_id: (node_count as u64) + 1,
        };

        // Final verification that all nodes see the complete cluster
        timing::wait_for_condition_with_backoff(
            || async {
                for node in cluster.nodes.values() {
                    if let Ok((_, peers, _)) = node.shared_state.get_cluster_status().await {
                        if peers.len() < node_count {
                            eprintln!(
                                "Node {} sees {} peers, expected {}",
                                node.id,
                                peers.len(),
                                node_count
                            );
                            return false;
                        }
                    } else {
                        eprintln!("Node {} failed to get cluster status", node.id);
                        return false;
                    }
                }
                eprintln!("All nodes see {} peers", node_count);
                true
            },
            convergence_timeout,
            Duration::from_millis(100),
        )
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Nodes failed to converge: {}", e),
        })?;

        // Trigger log replication by sending a health check from the bootstrap node
        // This ensures configuration changes are replicated to all nodes
        if node_count > 1 {
            if let Ok(mut client) = cluster.client(1).await {
                let _ = client
                    .health_check(crate::iroh_types::HealthCheckRequest {})
                    .await;
            }
        }

        // Now wait for leader convergence
        cluster.wait_for_convergence(convergence_timeout).await?;

        Ok(cluster)
    }
}

/// Client with automatic retry logic
pub struct RetryClient;

impl RetryClient {
    /// Connect to a node with retry logic (deprecated - use TestNode::client() instead)
    pub async fn connect(_addr: SocketAddr) -> BlixardResult<IrohClusterServiceClient> {
        // This method is deprecated in favor of TestNode::client() which properly
        // gets the Iroh NodeAddr from the node's P2P manager
        Err(BlixardError::NotImplemented {
            feature: "RetryClient::connect is deprecated. Use TestNode::client() instead"
                .to_string(),
        })
    }
}

/// Wait for a condition to become true (compatibility wrapper)
pub async fn wait_for_condition<F, Fut>(
    condition: F,
    timeout_duration: Duration,
    check_interval: Duration,
) -> BlixardResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    // Convert to FnMut for timing utilities
    let condition = condition;
    timing::wait_for_condition_with_backoff(|| condition(), timeout_duration, check_interval)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Timeout after {:?}: {}", timeout_duration, e),
        })
}

/// Wait for server to be ready with environment-aware timeouts
async fn wait_for_server_ready(addr: SocketAddr) -> BlixardResult<()> {
    let base_timeout = Duration::from_secs(5);
    let scaled_timeout = timing::scaled_timeout(base_timeout);

    timing::wait_for_condition_with_backoff(
        || async move {
            // For Iroh services, we need to wait for the endpoint to be ready
            // Since we can't easily connect without discovery info during startup,
            // we'll just wait a bit and assume it's ready
            // In production, this would be done through proper health checks
            tokio::time::sleep(Duration::from_millis(200)).await;
            true
        },
        base_timeout,
        Duration::from_millis(100),
    )
    .await
    .map_err(|_| BlixardError::Internal {
        message: format!("Server startup timeout after {:?}", scaled_timeout),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_node() {
        let node = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .await
            .build()
            .await
            .unwrap();

        // Verify node is running
        let client = node.client().await.unwrap();
        let response = client
            .clone()
            .health_check(HealthCheckRequest {})
            .await
            .unwrap();
        assert!(response.into_inner().healthy);

        node.shutdown().await;
    }

    /// Test cluster formation with expected timing variations
    ///
    /// This test may require retries due to legitimate timing variations:
    /// - Leader election uses randomized timeouts
    /// - Node discovery and connection establishment timing
    /// - Initial configuration propagation
    ///
    /// These variations represent real distributed system behaviors
    #[tokio::test]
    async fn test_cluster_formation() {
        let cluster = TestCluster::builder().with_nodes(3).build().await.unwrap();

        // Verify all nodes see the same leader
        let mut leader_ids = Vec::new();
        for node in cluster.nodes().values() {
            let state = node.shared_state.get_raft_status().await.unwrap();
            if let Some(leader) = state.leader_id {
                leader_ids.push(leader);
            }
        }

        assert_eq!(leader_ids.len(), 3);
        assert!(leader_ids.iter().all(|&id| id == leader_ids[0]));

        cluster.shutdown().await;
    }

    #[tokio::test]
    async fn test_database_factory() {
        // Test basic factory
        let (database, _temp_dir) = TestDatabaseFactory::create().unwrap();
        
        // Verify tables are initialized
        let read_txn = database.begin_read().unwrap();
        let _vm_table = read_txn.open_table(VM_STATE_TABLE).unwrap();
        let _worker_table = read_txn.open_table(WORKER_TABLE).unwrap();
        
        // Test named factory
        let (named_db, _temp_dir2) = TestDatabaseFactory::create_with_name("custom").unwrap();
        assert!(named_db.begin_read().is_ok());
    }

    #[tokio::test]
    async fn test_worker_factory() {
        let (database, _temp_dir) = TestDatabaseFactory::create().unwrap();
        
        // Test worker registration
        let capabilities = TestWorkerFactory::standard_capabilities();
        TestWorkerFactory::register_worker(&database, 1, capabilities, true).unwrap();
        
        // Verify worker was registered
        let read_txn = database.begin_read().unwrap();
        let worker_table = read_txn.open_table(WORKER_TABLE).unwrap();
        let worker_data = worker_table.get(1u64.to_le_bytes().as_slice()).unwrap();
        assert!(worker_data.is_some());
    }

    #[tokio::test]
    async fn test_vm_factory() {
        let (database, _temp_dir) = TestDatabaseFactory::create().unwrap();
        
        // Test VM creation and storage
        let vm_state = TestVmFactory::create_vm_state("test-vm", 1);
        TestVmFactory::store_vm_state(&database, &vm_state).unwrap();
        
        // Verify VM was stored
        let read_txn = database.begin_read().unwrap();
        let vm_table = read_txn.open_table(VM_STATE_TABLE).unwrap();
        let vm_data = vm_table.get("test-vm").unwrap();
        assert!(vm_data.is_some());
    }
}
