use std::sync::Arc;
use std::net::SocketAddr;

use blixard_core::{
    error::BlixardResult,
    types::NodeConfig,
    vm_backend::{VmManager, VmBackendRegistry},
    node::Node,
    metrics_otel_v2,
    metrics_server,
};
use blixard_vm::{MicrovmBackendFactory};
use tokio::task::JoinHandle;

/// Configuration for the Blixard orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Node configuration for distributed consensus
    pub node_config: NodeConfig,
    /// VM backend type to use ("mock", "microvm", "docker", etc.)
    pub vm_backend_type: String,
}

/// Main orchestrator that coordinates between distributed systems and VM management
/// 
/// This is the primary entry point for the Blixard system. It creates and manages
/// both the distributed consensus layer (blixard-core) and the VM backend (blixard-vm),
/// providing a unified interface for cluster and VM operations.
/// 
/// ## Architecture
/// 
/// ```text
/// ┌─────────────────────────────────────────┐
/// │           BlixardOrchestrator           │
/// ├─────────────────────────────────────────┤
/// │  • CLI Command Processing              │
/// │  • Configuration Management            │
/// │  • Component Lifecycle                 │
/// └─────────────────────────────────────────┘
///          │                      │
///          ▼                      ▼
/// ┌─────────────────┐    ┌─────────────────┐
/// │  blixard-core   │    │   blixard-vm    │
/// │                 │    │                 │
/// │ • Raft consensus │    │ • VM lifecycle  │
/// │ • gRPC server   │    │ • microvm.nix   │
/// │ • Storage       │    │ • Monitoring    │
/// │ • Peer mgmt     │    │                 │
/// └─────────────────┘    └─────────────────┘
/// ```
pub struct BlixardOrchestrator {
    node: Node,
    metrics_handle: Option<JoinHandle<BlixardResult<()>>>,
}

impl BlixardOrchestrator {
    /// Create a new orchestrator with the given configuration
    pub async fn new(config: OrchestratorConfig) -> BlixardResult<Self> {
        tracing::info!("Creating Blixard orchestrator with VM backend: {}", config.vm_backend_type);
        
        // Update node config to include VM backend type
        let mut node_config = config.node_config;
        node_config.vm_backend = config.vm_backend_type.clone();
        
        // Create the distributed consensus node
        let node = Node::new(node_config);
        
        tracing::info!("Blixard orchestrator created with {} VM backend", config.vm_backend_type);
        
        Ok(Self { 
            node,
            metrics_handle: None,
        })
    }
    
    /// Initialize the node and start all services
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        tracing::info!("Initializing Blixard orchestrator services");
        
        // Initialize metrics with Prometheus exporter
        metrics_otel_v2::init_prometheus()
            .map_err(|e| blixard_core::error::BlixardError::Internal {
                message: format!("Failed to initialize metrics: {}", e),
            })?;
        tracing::info!("Metrics initialized with Prometheus exporter");
        
        // Set up the VM backend registry
        let mut registry = VmBackendRegistry::default(); // Includes built-in mock backend
        
        // Register the microvm backend
        registry.register(Arc::new(MicrovmBackendFactory));
        
        // TODO: Future backends can be registered here
        // registry.register(Arc::new(DockerBackendFactory));
        // registry.register(Arc::new(FirecrackerBackendFactory));
        
        // Initialize with the VM backend registry
        self.node.initialize_with_vm_registry(registry).await?;
        
        tracing::info!("Blixard orchestrator initialized successfully");
        Ok(())
    }
    
    /// Start the node and all services
    pub async fn start(&mut self) -> BlixardResult<()> {
        tracing::info!("Starting Blixard node services");
        self.node.start().await?;
        
        // Start metrics server on port bind_port + 1000
        let bind_addr = *self.node.shared().get_bind_addr();
        let metrics_port = bind_addr.port() + 1000;
        let metrics_addr = SocketAddr::new(bind_addr.ip(), metrics_port);
        
        self.metrics_handle = Some(metrics_server::spawn_metrics_server(metrics_addr));
        tracing::info!("Metrics server started on http://{}/metrics", metrics_addr);
        
        tracing::info!("Blixard node started successfully");
        Ok(())
    }
    
    /// Stop the node and all services
    pub async fn stop(&mut self) -> BlixardResult<()> {
        tracing::info!("Stopping Blixard orchestrator");
        
        // Stop metrics server
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
        }
        
        self.node.stop().await?;
        tracing::info!("Blixard orchestrator stopped");
        Ok(())
    }
    
    /// Get the node for direct access to distributed operations
    pub fn node(&self) -> &Node {
        &self.node
    }
    
    /// Get the VM manager for direct access to VM operations
    pub async fn vm_manager(&self) -> Option<Arc<VmManager>> {
        self.node.shared().get_vm_manager().await
    }
    
    /// Get the bind address of the gRPC server
    pub fn bind_address(&self) -> std::net::SocketAddr {
        *self.node.shared().get_bind_addr()
    }
}

impl Drop for BlixardOrchestrator {
    fn drop(&mut self) {
        tracing::debug!("BlixardOrchestrator dropped");
    }
}