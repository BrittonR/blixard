use std::path::PathBuf;
use std::sync::Arc;

use blixard_core::{
    error::BlixardResult,
    types::NodeConfig,
    vm_backend::{VmManager, MockVmBackend},
    node::Node,
};
use blixard_vm::{VmBackend, MicrovmBackend};

/// Configuration for the Blixard orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Node configuration for distributed consensus
    pub node_config: NodeConfig,
    /// Path to VM configuration directory
    pub vm_config_dir: PathBuf,
    /// Path to VM data directory
    pub vm_data_dir: PathBuf,
    /// Whether to use mock VM backend for testing
    pub use_mock_vm: bool,
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
    vm_manager: Arc<VmManager>,
    _vm_backend: Arc<dyn VmBackend>,
}

impl BlixardOrchestrator {
    /// Create a new orchestrator with the given configuration
    pub async fn new(config: OrchestratorConfig) -> BlixardResult<Self> {
        tracing::info!("Initializing Blixard orchestrator");
        
        // Create VM backend based on configuration
        let vm_backend: Arc<dyn VmBackend> = if config.use_mock_vm {
            tracing::info!("Using mock VM backend for testing");
            // We'll need to pass the database to MockVmBackend, but we don't have it yet
            // For now, we'll create it after the node is created
            todo!("Need to create mock backend after node creation")
        } else {
            tracing::info!("Using microvm.nix backend");
            Arc::new(MicrovmBackend::new(config.vm_config_dir, config.vm_data_dir)?)
        };
        
        // Create the distributed consensus node (not async)
        let mut node = Node::new(config.node_config);
        
        // Initialize the node to set up the database
        node.initialize().await?;
        
        // Get the database for VM backend creation
        let database = node.shared().get_database().await
            .ok_or_else(|| blixard_core::error::BlixardError::Internal { 
                message: "Database not initialized".to_string() 
            })?;
        
        // Handle mock backend creation after we have the database
        let vm_backend: Arc<dyn VmBackend> = if config.use_mock_vm {
            Arc::new(MockVmBackend::new(database.clone()))
        } else {
            vm_backend
        };
        
        // Create VM manager that bridges consensus and VM backend
        let vm_manager = Arc::new(VmManager::new(
            database,
            vm_backend.clone(),
        ));
        
        // Wire up the VM manager with the node
        node.shared().set_vm_manager(vm_manager.clone()).await;
        
        Ok(Self {
            node,
            vm_manager,
            _vm_backend: vm_backend,
        })
    }
    
    /// Initialize the node and start all services
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        tracing::info!("Starting Blixard orchestrator services");
        // Node is already initialized in new(), but we can initialize it again if needed
        tracing::info!("Blixard orchestrator initialized successfully");
        Ok(())
    }
    
    /// Start the node and all services
    pub async fn start(&mut self) -> BlixardResult<()> {
        tracing::info!("Starting Blixard node services");
        self.node.start().await?;
        tracing::info!("Blixard node started successfully");
        Ok(())
    }
    
    /// Stop the node and all services
    pub async fn stop(&mut self) -> BlixardResult<()> {
        tracing::info!("Stopping Blixard orchestrator");
        self.node.stop().await?;
        tracing::info!("Blixard orchestrator stopped");
        Ok(())
    }
    
    /// Get the node for direct access to distributed operations
    pub fn node(&self) -> &Node {
        &self.node
    }
    
    /// Get the VM manager for direct access to VM operations
    pub fn vm_manager(&self) -> Arc<VmManager> {
        self.vm_manager.clone()
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